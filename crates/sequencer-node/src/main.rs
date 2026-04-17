// crates/sequencer-node/src/main.rs

use std::{
    pin::Pin,
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
};
use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use alloy_primitives::FixedBytes;
use engine::processor::{Command, MarketProcessor};
use engine::{EngineEvent, Side as EngineSide};
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio::sync::{broadcast, mpsc, oneshot};
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;
use trading::trading_engine_server::{TradingEngine, TradingEngineServer};
use trading::{
    CancelOrderRequest, CancelOrderResponse, DepthRequest, DepthResponse, ExecutionReport,
    IntentBidRequest, IntentBidResponse, IntentBundle, OrderLevel as ProtoOrderLevel,
    PlaceOrderRequest, PlaceOrderResponse, Side as ProtoSide, TradeExecution,
};

mod settlement;

pub mod trading {
    tonic::include_proto!("trading");
}

pub struct TradingService {
    // Channel untuk mengirim command ke MarketProcessor (Actor)
    processor_sender: mpsc::Sender<Command>,
}

#[tonic::async_trait]
impl TradingEngine for TradingService {
    async fn place_limit_order(
        &self,
        request: Request<PlaceOrderRequest>,
    ) -> Result<Response<PlaceOrderResponse>, Status> {
        let req = request.into_inner();

        // 1. Validasi & Konversi Input (Proto -> Internal)
        let side = match ProtoSide::try_from(req.side).unwrap_or(ProtoSide::Unspecified) {
            ProtoSide::Bid => EngineSide::Bid,
            ProtoSide::Ask => EngineSide::Ask,
            ProtoSide::Unspecified => return Err(Status::invalid_argument("Side is required")),
        };

        // 2. Siapkan Response Channel (One-Shot)
        let (resp_tx, resp_rx) = oneshot::channel();

        // 3. Kirim Command ke Engine
        let command = Command::PlaceOrder {
            user_id: req.user_id,
            order_id: req.order_id,
            side,
            price: req.price,
            quantity: req.quantity,
            responder: resp_tx,
        };

        // Kirim ke actor (jika channel penuh/tutup, berarti engine mati)
        self.processor_sender
            .send(command)
            .await
            .map_err(|_| Status::internal("Engine is down"))?;

        // 4. Tunggu Hasil dari Engine
        let events = resp_rx
            .await
            .map_err(|_| Status::internal("Engine failed to respond"))?;

        // 5. Konversi Event Engine ke Response Proto
        let mut fills = Vec::new();
        let mut success = false;

        for event in events {
            match event {
                EngineEvent::OrderPlaced { id, .. } if id == req.order_id => {
                    success = true; // Order masuk book (Maker)
                }
                EngineEvent::TradeExecuted {
                    maker_id,
                    taker_id,
                    price,
                    quantity,
                } => {
                    // Jika kita adalah taker, catat eksekusi ini
                    if taker_id == req.order_id {
                        fills.push(TradeExecution {
                            maker_order_id: maker_id,
                            price,
                            quantity,
                        });
                        success = true; // Terjadi trade (Taker)
                    }
                }
                EngineEvent::OrderCancelled { .. } => {}
                _ => {}
            }
        }

        Ok(Response::new(PlaceOrderResponse {
            success,
            message: if success {
                "Order Processed".to_string()
            } else {
                "Order Rejected".to_string()
            },
            fills,
        }))
    }

    async fn execute_solver_bundle(
        &self,
        request: Request<IntentBundle>,
    ) -> Result<Response<ExecutionReport>, Status> {
        let req = request.into_inner();

        // 1. Konversi Proto Orders ke Internal Command Orders
        let mut bundle_orders = Vec::new();

        for order in req.orders {
            let side = match ProtoSide::try_from(order.side).unwrap_or(ProtoSide::Unspecified) {
                ProtoSide::Bid => EngineSide::Bid,
                ProtoSide::Ask => EngineSide::Ask,
                ProtoSide::Unspecified => return Err(Status::invalid_argument("Side Invalid")),
            };

            bundle_orders.push(engine::processor::BundleRequest {
                user_id: order.user_id,
                order_id: order.order_id,
                side,
                price: order.price,
                quantity: order.quantity,
            });
        }

        let (resp_tx, resp_rx) = oneshot::channel();

        //  2. Kirim Command::ExecuteBundle ke Engine
        self.processor_sender
            .send(Command::ExecuteBundle {
                orders: bundle_orders,
                responder: resp_tx,
            })
            .await
            .map_err(|_| Status::internal("Engine is down"))?;

        // 3. Tunggu hasil
        let events = resp_rx
            .await
            .map_err(|_| Status::internal("Engine faield to respond"))?;

        // 4. Buat Laporan Eksekusi (ExecutionReport)
        // A. Filter dan Map event menjadi TradeExecution
        let fills: Vec<TradeExecution> = events
            .iter()
            .filter_map(|event| {
                match event {
                    // Kita hanya peduli event TradeExecuted
                    engine::EngineEvent::TradeExecuted {
                        maker_id,
                        price,
                        quantity,
                        ..
                    } => Some(TradeExecution {
                        maker_order_id: *maker_id,
                        price: *price,
                        quantity: *quantity,
                    }),
                    _ => None,
                }
            })
            .collect();

        // B. Return Response
        Ok(Response::new(ExecutionReport {
            success: true,
            message: format!("Bundle Executed. Total Events: {}", events.len()),
            fills,
        }))
    }

    async fn cancel_order(
        &self,
        request: Request<CancelOrderRequest>,
    ) -> Result<Response<CancelOrderResponse>, Status> {
        let req = request.into_inner();
        let (resp_tx, resp_rx) = oneshot::channel();

        // 1. Kirim Command ke Actor
        self.processor_sender
            .send(Command::CancelOrder {
                user_id: req.user_id,
                order_id: req.order_id,
                responder: resp_tx,
            })
            .await
            .map_err(|_| Status::internal("Engine down"))?;

        // 2. Tunggu hasil
        let events = resp_rx.await.map_err(|_| Status::internal("No response"))?;

        // 3. Cek apakah ada event OrderCancelled
        let success = events
            .iter()
            .any(|e| matches!(e, EngineEvent::OrderCancelled { .. }));

        Ok(Response::new(CancelOrderResponse {
            success,
            remaining_qty: 0,
        }))
    }

    async fn get_order_book_depth(
        &self,
        request: Request<DepthRequest>,
    ) -> Result<Response<DepthResponse>, Status> {
        let req = request.into_inner();
        let limit = if req.limit == 0 {
            10
        } else {
            req.limit as usize
        };

        let (resp_tx, resp_rx) = oneshot::channel();

        // Kirim command ke Engine Actor
        self.processor_sender
            .send(Command::GetDepth {
                limit,
                responder: resp_tx,
            })
            .await
            .map_err(|_| Status::internal("Engine down"))?;

        // Tunggu hasil
        let (asks, bids) = resp_rx.await.map_err(|_| Status::internal("No response"))?;

        // Mapping dari Engine struct ke Proto struct
        let proto_asks = asks
            .into_iter()
            .map(|l| ProtoOrderLevel {
                price: l.price,
                total_quantity: l.quantity,
            })
            .collect();

        let proto_bids = bids
            .into_iter()
            .map(|l| ProtoOrderLevel {
                price: l.price,
                total_quantity: l.quantity,
            })
            .collect();

        Ok(Response::new(DepthResponse {
            bids: proto_bids,
            asks: proto_asks,
            sequence_id: 0,
        }))
    }

    async fn submit_intent_bid(
        &self,
        request: Request<IntentBidRequest>,
    ) -> Result<Response<IntentBidResponse>, Status> {
        let req = request.into_inner();

        // 1. Validasi Bid (Tanda tangan, format)
        if req.solver_signature.is_empty() {
            return Err(Status::invalid_argument("Signature required"));
        }

        // 2. Siapkan channel untuk respon lelang
        let (resp_tx, _resp_rx) = oneshot::channel();

        // 3. Kirim Bid ke Engine/Processor yang baru (Kita akan modifikasi ini)
        self.processor_sender
            .send(Command::SubmitBid {
                solver_id: req.solver_id,
                intent_id: req.intent_id.clone(),
                proposed_output_amount: req.proposed_output_amount,
                estimated_gas_cost: req.estimated_gas_cost,
                solver_signature: req.solver_signature,
                responder: resp_tx,
            })
            .await
            .map_err(|_| Status::internal("Engine is down"))?;

        // 4. Berikan respons instan bahwa Bid diterima dalam kolam lelang
        Ok(Response::new(IntentBidResponse {
            accepted: true,
            message: "Bid queued for OFA evaluation".to_string(),
            auction_id: format!("auc-{}", req.intent_id), // ID lelang berdasarkan ID Intent
        }))
    }

    // Definisi tipe aliran data untuk gRPC Stream
    type SubscribeIntentMempoolStream = Pin<Box<dyn Stream<Item = Result<trading::IntentEvent, Status>> + Send>>;

    async fn subscribe_intent_mempool(
        &self,
        request: Request<trading::MempoolSubscribeRequest>,
    ) -> Result<Response<Self::SubscribeIntentMempoolStream>, Status> {
        let req = request.into_inner();

        // Logika penyambungan asli ke Mempool L2 akan dilakukan di sini nanti.
        let (_, rx) = tokio::sync::mpsc::channel(128);
        let output_stream = ReceiverStream::new(rx);

        Ok(Response::new(Box::pin(output_stream) as Self::SubscribeIntentMempoolStream))
    }
}

// Handler WebSocket
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(broadcast_tx): State<broadcast::Sender<EngineEvent>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, broadcast_tx))
}

async fn handle_socket(mut socket: WebSocket, broadcast_tx: broadcast::Sender<EngineEvent>) {
    // Subcribe ke channel broadcast
    let mut rx = broadcast_tx.subscribe();

    while let Ok(event) = rx.recv().await {
        // Konversi EngineEvent ke JSON
        let json_msg = match event {
            EngineEvent::TradeExecuted {
                maker_id,
                taker_id,
                price,
                quantity,
            } => serde_json::json! ({
                "type": "TRADE",
                "maker_id": maker_id,
                "taker_id": taker_id,
                "price": price,
                "quantity": quantity,
            }),
            EngineEvent::OrderPlaced {
                id,
                price,
                quantity,
                side,
                ..
            } => serde_json::json! ({
                "type": "ORDER_PLACED",
                "id": id,
                "price": price,
                "quantity": quantity,
                "side": format!("{:?}", side),
            }),
            EngineEvent::OrderCancelled { id } => serde_json::json! ({
                "type": "ORDER_CANCELLED",
                "id": id,
            }),
            EngineEvent::IntentResolved {
                intent_id,
                winning_solver,
                winning_amount,
            } => serde_json::json!({
                "type": "INTENT_RESOLVED",
                "intent_id": intent_id,
                "winning_solver": winning_solver,
                "winning_amount": winning_amount,
            }),
        };

        // Kirim string JSON ke Client WebSocket
        if let Ok(msg_text) = serde_json::to_string(&json_msg) {
            if socket.send(Message::Text(msg_text)).await.is_err() {
                break;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup Channel: Buffer 1024 command antrian
    let (tx, rx) = mpsc::channel(1024);
    // Channel Broadcast: kapasitas 100 pesan. Jika client lambat, pesan lama didrop (lag).
    let (broadcast_tx, _) = broadcast::channel(100);

    // 2. Ambil variabel dari .env atau environment
    let l1_rpc_url = std::env::var("L1_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".to_string());
    
    // Private Key dummy Anvil (Account #0) untuk development
    let private_key = std::env::var("SEQUENCER_PRIVATE_KEY")
        .unwrap_or_else(|_| "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string());
    
    // Address kontrak EviceRollup setelah di-deploy di Anvil
    let rollup_address = std::env::var("ROLLUP_CONTRACT_ADDRESS")
        .unwrap_or_else(|_| "0x5FbDB2315678afecb367f032d93F642f64180aa3".to_string());
    
    // 3. Inisialisasi engine settlement
    // (Opsional) Kita bisa memasukkan settlement_engine ini ke dalam Actor/Processor
    // agar processor bisa memanggil `submit_zk_batch` saat lelang selesai!
    let settlement_engine = Arc::new(
        settlement::SettlementEngine::new(l1_rpc_url, private_key, rollup_address).await
    );

    // 4. Channel untuk Event Latar Belakang (Jembatan Engine -> L1)
    let (outbound_tx, mut outbound_rx) = tokio::sync::mpsc::channel(1024);
    
    let processor_broadcast_tx = broadcast_tx.clone();

    // 5. Inisialisasi Market Processor 
    let mut processor = engine::processor::MarketProcessor::new(
        rx, 
        processor_broadcast_tx,
        outbound_tx
    );
    tokio::spawn(async move { 
        processor.run().await; 
    });

    // 6. THE SETTLEMENT TASK (Menangkap kabel dari Engine dan menembaknya ke L1)
    let engine_clone = settlement_engine.clone();
    tokio::spawn(async move {
        info!("Settlement Background Task listening for OFA events...");
        
        while let Some(event) = outbound_rx.recv().await {
            match event {
                // Tangkap event kemenangan lelang
                EngineEvent::IntentResolved { intent_id, winning_solver, winning_amount } => {
                    info!("Meneruskan Intent {} ke Ethereum L1...", intent_id);

                    // A. Di sistem produksi, kita memanggil ZK-Prover di sini.
                    // Untuk saat ini, kita buat dummy proof & state root
                    let dummy_state_root = [0u8; 32]; 
                    let dummy_proof = vec![1, 2, 3, 4, 5]; // Dummy Plonky3 Proof
                    
                    // Konversi string intent_id menjadi byte32 
                    // (Diasumsikan intent_id adalah representasi hex 32 byte)
                    let intent_bytes = FixedBytes::<32>::from_str(&intent_id)
                        .unwrap_or(FixedBytes::<32>::ZERO);

                    // B. Buat Settlement Batch
                    let batch = settlement::SettlementBatch {
                        new_state_root: dummy_state_root,
                        proof: dummy_proof,
                        resolved_intent_ids: vec![intent_bytes.0],
                    };

                    // C. TEMBAK KE L1 SECARA ASINKRON!
                    engine_clone.submit_zk_batch(batch).await;
                }
                _ => { /* Abaikan event lain seperti OrderPlaced */ }
            }
        }
    });

    // 7. Setup WebSocket Server (Axum)
    let app = axum::Router::new()
        .route("/ws", axum::routing::get(ws_handler))
        .with_state(broadcast_tx); 

    let ws_addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    info!(">>> WebSocket Market Data Server Listening on ws://127.0.0.1:3000/ws");

    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(ws_addr).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    // 8. Setup gRPC Server (Main Task)
    let addr = "[::1]:50051".parse()?;
    let trading_service = TradingService {
        processor_sender: tx,
    };

    info!("DEX Engine listening on {}", addr);

    Server::builder()
        .add_service(TradingEngineServer::new(trading_service))
        .serve(addr)
        .await?;

    Ok(())
}
