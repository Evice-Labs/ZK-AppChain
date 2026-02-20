// crates/solver-service/src/main.rs

use anyhow::Result;
use dotenv::dotenv;
use std::env;
use tracing::info;

mod executor;
mod strategy;

use executor::VelocityExecutor;
use simulator::EvmSimulator;
use strategy::ArbStrategy;

#[derive(Debug, Clone)]
pub struct InternalOrder {
    pub user_id: u64,
    pub order_id: u64,
    pub is_bid: bool,
    pub price: u64,
    pub quantity: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    info!("Starting Evice Intelligent Solver...");

    // 1. Konfigurasi
    let sequencer_url =
        env::var("SEQUENCER_URL").unwrap_or_else(|_| "http://[::1]:50051".to_string());
    // Wajib punya RPC URL (bisa pakai Alchemy/Infura/Anvil localhost)
    let rpc_url = env::var("RPC_URL").expect("RPC_URL wajib diset di .env untuk simulasi!");
    let solver_id = "solver-smart-01".to_string();

    // 2. Init Executor (Kaki Tangan)
    let executor = VelocityExecutor::new(sequencer_url, solver_id).await?;

    // 3. Init Simulator (Otak Kiri - Kalkulator EVM)
    info!("Initializing EVM Simulator (Forking Mainnet)...");
    let simulator = EvmSimulator::new(rpc_url)?;

    // 4. Init Strategy (Otak Kanan - Pengambil Keputusan)
    let mut strategy = ArbStrategy::new(executor, simulator, 777);

    // 5. Run
    strategy.run().await?;

    Ok(())
}
