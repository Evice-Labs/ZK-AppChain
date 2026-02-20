// crates/solver-service/src/executor.rs

use anyhow::Result;
use tonic::transport::Channel;
use tracing::{error, info};
use trading::trading_engine_client::TradingEngineClient;
use trading::{DepthRequest, DepthResponse, IntentBundle, PlaceOrderRequest, Side as ProtoSide};

pub mod trading {
    tonic::include_proto!("trading");
}

#[derive(Clone)]
pub struct VelocityExecutor {
    client: TradingEngineClient<Channel>,
    pub solver_id: String,
}

impl VelocityExecutor {
    // Connect ke Sequencer Node (Velocity-DEX)
    pub async fn new(endpoint: String, solver_id: String) -> Result<Self> {
        info!("Connecting to Velocity Sequencer at {}...", endpoint);
        // Connect lazily
        let client = TradingEngineClient::connect(endpoint).await?;
        info!("✅ Connected to Sequencer!");

        Ok(Self { client, solver_id })
    }

    // "The Eye Solver" Melihat Orderbook
    pub async fn get_market_depth(&self, symbol: String) -> Result<DepthResponse> {
        let mut client = self.client.clone();

        let request = tonic::Request::new(DepthRequest { symbol, limit: 5 });

        let response = client.get_order_book_depth(request).await?;
        Ok(response.into_inner())
    }

    // Mengirim Bundle Transaksi untuk dieksekusi secara atomik
    pub async fn send_bundle(&self, orders: Vec<crate::InternalOrder>) -> Result<()> {
        let mut client = self.client.clone();

        // 1. Konversi Internal Order -> Proto Request
        let proto_orders: Vec<PlaceOrderRequest> = orders
            .into_iter()
            .map(|o| PlaceOrderRequest {
                user_id: o.user_id,
                order_id: o.order_id,
                side: if o.is_bid {
                    ProtoSide::Bid as i32
                } else {
                    ProtoSide::Ask as i32
                },
                price: o.price,
                quantity: o.quantity,
            })
            .collect();

        // 2. Bungkus dalam IntentBundle
        let bundle = IntentBundle {
            orders: proto_orders,
            solver_id: self.solver_id.clone(),
            nonce: chrono::Utc::now().timestamp_millis() as u64,
        };

        info!("Sending Bundle with {} orders...", bundle.orders.len());

        // 3. Kirim via gRPC
        let request = tonic::Request::new(bundle);

        match client.execute_solver_bundle(request).await {
            Ok(response) => {
                let report = response.into_inner();
                if report.success {
                    info!("✅ Bundle Executed! Msg: {}", report.message);
                    // Disini Anda bisa log 'fills' untuk menghitung profit
                } else {
                    error!("❌ Execution Failed: {}", report.message);
                }
            }
            Err(status) => {
                error!("❌ gRPC Error: status={:?}", status);
            }
        }

        Ok(())
    }
}
