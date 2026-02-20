// crates/sequencer-node/src/settlement.rs

use aegis_node::rpc_client::RpcClient;
use aegis_node::Transaction; 
use tracing::{info, error, warn};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SettlementEngine {
    client: Arc<Mutex<Option<RpcClient>>>, 
}

impl SettlementEngine {
    pub async fn new(aegis_l1_url: String) -> Self {
        info!("🌉 Initializing Bridge to Aegis Settlement Layer at {}...", aegis_l1_url);
        
        let l2_url_dummy = "http://localhost:3000".to_string();

        let client_option = match RpcClient::new(aegis_l1_url.clone(), l2_url_dummy).await {
            Ok(client) => {
                info!("✅ Connected to Aegis L1 Settlement Layer!");
                Some(client)
            }
            Err(e) => {
                warn!("⚠️ Failed to connect to Aegis L1: {}. Running in Standalone Mode.", e);
                None
            }
        };

        Self {
            client: Arc::new(Mutex::new(client_option)),
        }
    }

    // Fungsi untuk mengirim batch transaksi (State Root Update) ke L1
    pub async fn submit_batch(&self, tx: Transaction) {
        let mut guard = self.client.lock().await;
        
        if let Some(client) = guard.as_mut() {
            info!("📦 Submitting Settlement Tx to Aegis...");
            
            match client.submit_l1_transaction(&tx).await {
                Ok(tx_hash) => info!("✅ Settlement Success! Aegis TxHash: {}", tx_hash),
                Err(e) => error!("❌ Settlement Failed: {:?}", e),
            }
        } else {
            warn!("Cannot submit batch: Aegis L1 disconnected.");
        }
    }
}