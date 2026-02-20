// crates/solver-service/src/strategy.rs

use crate::executor::VelocityExecutor;
use crate::InternalOrder;
use alloy::providers::{Provider, ProviderBuilder};
use alloy::sol;
use alloy::sol_types::SolCall;
use alloy_primitives::{Address, Bytes, U256};
use anyhow::Result;
use rand::Rng;
use simulator::EvmSimulator;
use std::env;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use url::Url;

// Definisi Interface Uniswap V2 Router
sol! {
    interface IUniswapV2Router {
        function getAmountsOut(uint amountIn, address[] calldata path) external view returns (uint[] memory amounts);
    }
}

// Konstanta Mainnet
const UNISWAP_ROUTER: &str = "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D";
const WETH: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const USDC: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";

pub struct ArbStrategy {
    executor: VelocityExecutor,
    #[allow(dead_code)] 
    simulator: EvmSimulator,
    user_id: u64,
    rpc_url: String, 
}

impl ArbStrategy {
    pub fn new(executor: VelocityExecutor, simulator: EvmSimulator, user_id: u64) -> Self {
        let rpc_url = env::var("RPC_URL").expect("RPC_URL must be set");
        Self {
            executor,
            simulator,
            user_id,
            rpc_url,
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        info!("Strategy Engine: REAL Cross-Chain Arbitrage (Velocity <-> Alchemy Mainnet)");
        let mut rng = rand::rng();

        // Setup Solver Address (Dummy)
        let solver_addr = Address::from_str("0x1234567890123456789012345678901234567890").unwrap();

        self.simulator
            .set_balance(solver_addr, U256::from(100) * U256::from(1e18));

        loop {
            // 1. Cek Pasar Velocity
            match self.executor.get_market_depth("ETH_USDC".to_string()).await {
                Ok(depth) => {
                    if let Some(best_bid) = depth.bids.first() {
                        let velocity_bid_price = best_bid.price;
                        let quantity_needed = 1;

                        info!(
                            "Opportunity Check: User wants to BUY at ${}",
                            velocity_bid_price
                        );

                        // 2. Cek Harga Real di Uniswap via Alchemy
                        match self.check_uniswap_price_real().await {
                            Ok((uniswap_price, gas_cost_usd)) => {
                                // Hitung Net Profit: (Jual di Velocity - Beli di Uniswap) - Biaya Gas
                                let gross_profit =
                                    (velocity_bid_price as i64) - (uniswap_price as i64);
                                let net_profit = gross_profit - (gas_cost_usd as i64);

                                if net_profit > 0 {
                                    info!("✅ ARBITRAGE FOUND! Velocity: ${} | Uniswap: ${} | Gas: ${} | NET PROFIT: ${}", 
                                        velocity_bid_price, uniswap_price, gas_cost_usd, net_profit);

                                    // 3. Eksekusi
                                    let order_id = rng.random_range(100000..999999);
                                    let bundle = vec![InternalOrder {
                                        user_id: self.user_id,
                                        order_id: order_id,
                                        is_bid: false, // ASK
                                        price: velocity_bid_price,
                                        quantity: quantity_needed,
                                    }];

                                    info!("Executing Hedge on Velocity...");
                                    self.executor.send_bundle(bundle).await?;
                                } else {
                                    debug!("❌ No Profit. Uniswap: ${} | Gas: ${} | Net: ${} (Too Expensive)", 
                                        uniswap_price, gas_cost_usd, net_profit);
                                }
                            }
                            Err(e) => warn!("RPC Error: {}", e),
                        }
                    } else {
                        debug!("Orderbook Empty.");
                    }
                }
                Err(e) => warn!("Sequencer RPC Error: {}", e),
            }

            sleep(Duration::from_secs(3)).await;
        }
    }

    async fn check_uniswap_price_real(&self) -> Result<(u64, u64)> {
        // 1. Setup Provider (Koneksi ke Alchemy)
        let rpc_url = Url::parse(&self.rpc_url)?;
        let provider = ProviderBuilder::new().connect_http(rpc_url);

        // 2. Setup Contract Call
        let router = Address::from_str(UNISWAP_ROUTER)?;
        let path = vec![Address::from_str(WETH)?, Address::from_str(USDC)?];
        let one_eth = U256::from(1_000_000_000_000_000_000u64); // 1 ETH

        // Encoder call data: getAmountsOut(1 ETH, [WETH, USDC])
        let call = IUniswapV2Router::getAmountsOutCall {
            amountIn: one_eth,
            path,
        };

        // 3. Lakukan Call (View) ke Mainnet
        // Ini tidak menggunakan gas ETH asli, hanya call data read-only
        let result = call.abi_encode();

        // Gunakan call_raw dari provider standard alloy
        let tx_req = alloy::rpc::types::eth::TransactionRequest::default()
            .to(router)
            .input(Bytes::from(result).into());

        let output_bytes = provider.call(tx_req).await?;

        // 4. Decode Output
        let decoded = IUniswapV2Router::getAmountsOutCall::abi_decode_returns(&output_bytes)?;

        // Output adalah array [amountIn, amountOut]. Kita ambil index 1 (USDC amount)
        let amount_usdc_wei = decoded[1];

        // Konversi ke satuan User (USDC decimals 6). Asumsi engine kita pakai basis data integer sederhana.
        // Untuk demo, kita bagi 1e6 agar dapat harga USD integer.
        let price_usd = (amount_usdc_wei / U256::from(1_000_000)).to::<u64>();

        // 5. Estimasi Gas Cost (Simplified)
        // Kita anggap swap butuh 150,000 gas.
        // Kita ambil gas price asli dari network.
        let gas_price = provider.get_gas_price().await?;
        let gas_used = U256::from(150_000);
        let gas_cost_wei = gas_used * U256::from(gas_price);

        // Konversi gas cost ke USD. Asumsi ETH = $2500 (Hardcoded rate for gas calc only)
        // Gas (Wei) / 1e18 * 2500
        // (Wei * 2500) / 1e18
        let gas_cost_usd = (gas_cost_wei * U256::from(2500)
            / U256::from(1_000_000_000_000_000_000u64))
        .to::<u64>();

        Ok((price_usd, gas_cost_usd))
    }
}
