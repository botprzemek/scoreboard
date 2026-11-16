use super::connection::{self, FalliblePacketSender};
use crate::services::{DeviceService, GameService};

use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderValue;
use axum::response::IntoResponse;
use axum::routing::{self, Router};
use futures::SinkExt;
use futures::stream::{SplitSink, SplitStream, StreamExt};
use net::layout::PACKET_SIZE;
use net::{PacketReceiver, RawPacket};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

struct WebSocketSender {
    sink: SplitSink<WebSocket, Message>,
}

impl FalliblePacketSender for WebSocketSender {
    type Error = axum::Error;

    async fn send_packet(&mut self, data: &[u8; PACKET_SIZE]) -> Result<(), Self::Error> {
        let payload = Bytes::copy_from_slice(data);
        self.sink.send(Message::Binary(payload)).await
    }
}

struct WebSocketReceiver {
    stream: SplitStream<WebSocket>,
}

impl PacketReceiver for WebSocketReceiver {
    async fn receive_packet(&mut self) -> Result<RawPacket, &'static str> {
        while let Some(message_result) = self.stream.next().await {
            match message_result {
                Ok(Message::Binary(binary)) if binary.len() == PACKET_SIZE => {
                    let mut buffer = [0u8; PACKET_SIZE];
                    buffer.copy_from_slice(&binary);
                    return Ok(RawPacket::new(buffer, None));
                }
                Ok(Message::Close(_)) | Err(_) => return Err("disconnected"),
                _ => continue,
            }
        }
        Err("disconnected")
    }
}

pub(super) struct WebsocketHandler;

impl WebsocketHandler {
    pub(super) async fn run(
        listener: TcpListener,
        game: GameService,
        devices: DeviceService,
        cors_origins: &[String],
        shutdown: CancellationToken,
    ) {
        let router = Self::router(game, devices, cors_origins, shutdown.clone());
        if let Err(error) = axum::serve(listener, router)
            .with_graceful_shutdown(shutdown.cancelled_owned())
            .await
        {
            tracing::error!("HTTP server failed: {error}");
        }
    }

    fn router(
        game: GameService,
        devices: DeviceService,
        cors_origins: &[String],
        shutdown: CancellationToken,
    ) -> Router {
        Router::new()
            .route("/ws", routing::get(Self::handle))
            .layer(Self::cors_layer(cors_origins))
            .with_state((game, devices, shutdown))
    }

    fn cors_layer(cors_origins: &[String]) -> CorsLayer {
        if cors_origins.is_empty() {
            return CorsLayer::permissive();
        }

        let origins: Vec<HeaderValue> = cors_origins
            .iter()
            .filter_map(|origin| match HeaderValue::from_str(origin) {
                Ok(value) => Some(value),
                Err(error) => {
                    tracing::warn!("Ignoring invalid CORS origin '{origin}': {error}");
                    None
                }
            })
            .collect();

        if origins.is_empty() {
            tracing::error!("All configured CORS origins are invalid; falling back to permissive CORS");
            return CorsLayer::permissive();
        }

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(Any)
            .allow_headers(Any)
    }

    async fn upgrade(
        socket: WebSocket,
        game: GameService,
        devices: DeviceService,
        shutdown: CancellationToken,
    ) {
        let (sink, stream) = socket.split();
        let sender = WebSocketSender { sink };
        let receiver = WebSocketReceiver { stream };

        connection::run(receiver, sender, game, devices, shutdown).await;
    }

    async fn handle(
        websocket: WebSocketUpgrade,
        State((game, devices, shutdown)): State<(GameService, DeviceService, CancellationToken)>,
    ) -> impl IntoResponse {
        websocket.on_upgrade(move |socket| Self::upgrade(socket, game, devices, shutdown))
    }
}
