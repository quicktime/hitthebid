use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use clap::Parser;
use databento::{
    dbn::{Record, TradeMsg},
    live::Subscription,
    LiveClient, Venue,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::{broadcast, RwLock};
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Databento API key
    #[arg(short, long, env = "DATABENTO_API_KEY")]
    api_key: String,

    /// Symbols to subscribe to (comma-separated)
    #[arg(short, long, default_value = "NQ.c.0,ES.c.0")]
    symbols: String,

    /// Port to run the web server on
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Minimum trade size to broadcast
    #[arg(short, long, default_value = "1")]
    min_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub symbol: String,
    pub price: f64,
    pub size: u32,
    pub side: String, // "buy" or "sell"
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    Trade(Trade),
    Connected { symbols: Vec<String> },
    Error { message: String },
    SymbolUpdate { symbols: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientMessage {
    pub action: String,
    pub symbol: Option<String>,
    pub min_size: Option<u32>,
}

struct AppState {
    tx: broadcast::Sender<WsMessage>,
    active_symbols: RwLock<HashSet<String>>,
    min_size: RwLock<u32>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("orderflow_bubbles=info".parse().unwrap())
                .add_directive("databento=info".parse().unwrap()),
        )
        .init();

    let args = Args::parse();

    info!("Starting Orderflow Bubbles server");
    info!("Symbols: {}", args.symbols);
    info!("Port: {}", args.port);
    info!("Min size filter: {}", args.min_size);

    // Create broadcast channel for trades
    let (tx, _rx) = broadcast::channel::<WsMessage>(1000);

    let symbols: Vec<String> = args.symbols.split(',').map(|s| s.trim().to_string()).collect();

    let state = Arc::new(AppState {
        tx: tx.clone(),
        active_symbols: RwLock::new(symbols.iter().cloned().collect()),
        min_size: RwLock::new(args.min_size),
    });

    // Spawn Databento streaming task
    let api_key = args.api_key.clone();
    let tx_clone = tx.clone();
    let state_clone = state.clone();
    
    tokio::spawn(async move {
        if let Err(e) = run_databento_stream(api_key, symbols, tx_clone, state_clone).await {
            error!("Databento stream error: {}", e);
        }
    });

    // Build router
    let app = Router::new()
        .route("/ws", get(ws_handler))
        .nest_service("/", ServeDir::new("frontend"))
        .layer(CorsLayer::new().allow_origin(Any))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], args.port));
    info!("Server running at http://{}", addr);
    info!("Open http://localhost:{} in your browser", args.port);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn run_databento_stream(
    api_key: String,
    symbols: Vec<String>,
    tx: broadcast::Sender<WsMessage>,
    state: Arc<AppState>,
) -> Result<()> {
    info!("Connecting to Databento...");

    let mut client = LiveClient::builder()
        .key(api_key)?
        .dataset(databento::Dataset::GlbxMdp3)
        .build()
        .await
        .context("Failed to connect to Databento")?;

    info!("Connected to Databento");

    // Subscribe to symbols
    let subscription = Subscription::builder()
        .symbols(symbols.clone())
        .schema(databento::dbn::Schema::Trades)
        .stype_in(databento::dbn::SType::RawSymbol)
        .build();

    client.subscribe(subscription).await.context("Failed to subscribe")?;
    
    info!("Subscribed to: {:?}", symbols);

    // Notify clients we're connected
    let _ = tx.send(WsMessage::Connected { symbols: symbols.clone() });

    // Start streaming
    client.start().await.context("Failed to start stream")?;

    // Process incoming records
    while let Some(record) = client.next_record().await? {
        if let Some(trade) = record.get::<TradeMsg>() {
            let min_size = *state.min_size.read().await;
            
            if trade.size >= min_size {
                // Determine buy/sell from aggressor side
                // 'A' = Ask side (buyer aggressor), 'B' = Bid side (seller aggressor)
                let side = match trade.side as char {
                    'A' => "buy",
                    'B' => "sell",
                    _ => "buy", // Default
                };

                // Get symbol from instrument ID (simplified - you may want symbol mapping)
                let symbol = get_symbol_from_record(&record, &symbols);

                let trade_msg = Trade {
                    symbol,
                    price: trade.price as f64 / 1_000_000_000.0, // Fixed-point conversion
                    size: trade.size,
                    side: side.to_string(),
                    timestamp: trade.hd.ts_event / 1_000_000, // Nanos to millis
                };

                // Broadcast to all connected clients
                if tx.send(WsMessage::Trade(trade_msg)).is_err() {
                    // No receivers, that's okay
                }
            }
        }
    }

    warn!("Databento stream ended");
    Ok(())
}

fn get_symbol_from_record(record: &dyn Record, symbols: &[String]) -> String {
    // For simplicity, if we only have one symbol, return it
    // In production, you'd map instrument_id to symbol
    if symbols.len() == 1 {
        return symbols[0].clone();
    }
    
    // Default to first symbol - proper implementation would use symbol mapping
    symbols.first().cloned().unwrap_or_else(|| "UNKNOWN".to_string())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    // Send current state to new client
    let symbols: Vec<String> = state.active_symbols.read().await.iter().cloned().collect();
    let welcome = WsMessage::Connected { symbols };
    if let Ok(json) = serde_json::to_string(&welcome) {
        let _ = sender.send(Message::Text(json)).await;
    }

    // Spawn task to forward trades to this client
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&msg) {
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Handle incoming messages from client
    let state_clone = state.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                    match client_msg.action.as_str() {
                        "set_min_size" => {
                            if let Some(size) = client_msg.min_size {
                                *state_clone.min_size.write().await = size;
                                info!("Min size filter set to: {}", size);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }

    info!("WebSocket client disconnected");
}
