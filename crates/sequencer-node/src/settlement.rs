// crates/sequencer-node/src/settlement.rs

use alloy::{
    network::EthereumWallet,
    providers::{Provider, ProviderBuilder},
    signers::local::PrivateKeySigner,
    sol,
};
use alloy_primitives::{Address, Bytes, FixedBytes};
use std::{
    str::FromStr,
    sync::Arc,
};
use tracing::{error, info};
use url::Url;

// 1. GENERATE RUST BINDINGS DARI SOLIDITY
// Makro sol! otomatis membaca ABI fungsi dan membuat struct Rust
sol! {
    #[sol(rpc)]
    interface IEviceRollup {
        function updateStateWithIntents(
            bytes32 _newStateRoot,
            bytes calldata _proof,
            bytes32[] calldata _resolvedIntentIds
        ) external;
    }
}

// Mendefinisikan apa yang dikirim ke L1 Contract
#[derive(Debug, Clone)]
pub struct SettlementBatch {
    pub new_state_root: [u8; 32],
    pub proof: Vec<u8>,
    pub resolved_intent_ids: Vec<[u8; 32]>,
}

pub struct SettlementEngine<P> {
    provider: Arc<P>,
    contract_address: Address,
}

impl SettlementEngine<()> {
    pub async fn new(
        l1_rpc_url: String, 
        private_key: String, 
        contract_address: String,
    ) -> SettlementEngine<impl Provider + Clone> {
        info!("Initializing EVM Bridge to L1 Smart Contract (EviceRollup)...");

        let url = Url::parse(&l1_rpc_url).expect("Format L1_RPC_URL tidak valid");
        let address = Address::from_str(&contract_address).expect("Format Contract Address tidak valid");

        // Setup Signer (Wallet Sequencer)
        let signer = PrivateKeySigner::from_str(&private_key)
            .expect("Format Private Key tidak valid");
        let wallet = EthereumWallet::from(signer.clone());

        // Setup Provider pembawa dompet
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(url);
        
        info!("Settlement Engine terhubung! Sequencer L1 Address: {}", signer.address());

        SettlementEngine { 
            provider: Arc::new(provider),
            contract_address: address,
        }
    }
}

impl<P: Provider + Clone> SettlementEngine<P> {
    // Fungsi untuk mengirim batch penyelesain ZK (State Root Update) ke L1.
    pub async fn submit_zk_batch(&self, batch: SettlementBatch) {
        info!("Preparing Settelemnt Batch for L1...");

        // Konversi tipe data Rust mentah (Vec, Array) ke tipe data EVM (Alloy Primitives)
        let new_state_root = FixedBytes::<32>::from(batch.new_state_root);
        let proof_bytes = Bytes::from(batch.proof);
        let resolved_ids: Vec<FixedBytes<32>> = batch.resolved_intent_ids
            .into_iter()
            .map(FixedBytes::from)
            .collect();

        info!("  - New State Root: {}", new_state_root);
        info!("  - ZK Proof Size: {} bytes", proof_bytes.len());
        info!("  - Resolved Intents: {}", resolved_ids.len());

        // Membuat instance kontrak virtual di Rust
        let contract = IEviceRollup::new(self.contract_address, self.provider.clone());

        // Membuat fungsi `updateStateWithIntents` secara on-chain
        let tx_call = contract.updateStateWithIntents(
            new_state_root,
            proof_bytes,
            resolved_ids,
        );

        info!("Mengirim transaksi L1 Settlement...");

        // Eksekusi transaksi dan tunggu konfirmasi blok
        match tx_call.send().await {
            Ok(pending_tx) => {
                info!("Transaksi terkirim! TxHash: {}", pending_tx.tx_hash());

                // Tunggu receipt (bukti transaksi masuk ke dalam block L1)
                match pending_tx.get_receipt().await {
                    Ok(receipt) => {
                        if receipt.status() {
                            info!("SETTLEMENT SUCCESS! L2 State Root telah diperbarui di Ethereum.");
                        } else {
                            error!("SETTLEMENT FAILED! Transaksi di-revert oleh L1 Contract. (Kemungkinan InvalidProof atau Paused)");
                        }
                    }
                    Err(e) => error!("Gagal mendapatkan receipt: {:?}", e),
                }
            }
            Err(e) => error!("Gagal mengirim transaksi L1: {:?}", e),
        }
    }
}