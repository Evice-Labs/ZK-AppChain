// crates/user-client/src/main.rs

use anyhow::Result;
use tracing::{info, warn};
use trading::trading_engine_client::TradingEngineClient;
use trading::{PlaceOrderRequest, Side};

pub mod trading {
    tonic::include_proto!("trading");
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("User Client Starting...");

    // 1. Koneksi ke Sequencer
    let mut client = TradingEngineClient::connect("http://[::1]:50051").await?;
    info!("✅ Connected to Exchange");

    // 2. Definisi "Intent" User
    // User ingin MEMBELI (Bid) dengan harga 9100.
    // Karena Solver menjual (Ask) di harga 9100 dengan quantity 10,
    // Kita akan membeli SEMUANYA (Qty: 10) agar likuiditas habis.
    let user_intent = PlaceOrderRequest {
        user_id: 12345,  // ID User Bebas
        order_id: 99901, // ID Unik
        side: Side::Bid as i32,
        price: 9100,  // Harga Match dengan Solver (Ask)
        quantity: 10, // Menghabiskan stok Solver
    };

    info!(
        "Sending BUY Intent: {} units @ ${}",
        user_intent.quantity, user_intent.price
    );

    // 3. Eksekusi
    let response = client.place_limit_order(user_intent).await?;
    let report = response.into_inner();

    if report.success {
        info!("✅ Trade Successful!");
        info!("Engine Message: {}", report.message);

        // Tampilkan detail 'Fills' (Bukti match terjadi)
        for fill in report.fills {
            info!(
                "FILLED: Matched with Order #{} | Price: {} | Qty: {}",
                fill.maker_order_id, fill.price, fill.quantity
            );
        }
    } else {
        warn!("❌ Trade Failed: {}", report.message);
    }

    Ok(())
}
