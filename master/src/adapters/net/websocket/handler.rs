use std::sync::Arc;

use super::connection;
use crate::services::Services;

use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderValue;
use axum::response::IntoResponse;
use axum::routing::{self, Router};
use futures::SinkExt;
use futures::stream::{SplitSink, SplitStream, StreamExt};
use net::layout::PACKET_SIZE;
use net::{PacketReceiver, PacketSender, RawPacket};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

pub struct WebSocketSender {
    sink: SplitSink<WebSocket, Message>,
}

impl PacketSender for WebSocketSender {
    async fn send_packet(&mut self, data: &[u8; PACKET_SIZE]) {
        let payload = Bytes::copy_from_slice(data);
        let _ = self.sink.send(Message::Binary(payload)).await;
    }
}

pub struct WebSocketReceiver {
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

pub struct WebsocketHandler;

impl WebsocketHandler {
    pub fn router(services: Arc<Services>, cors_origins: &[String]) -> Router {
        Router::new()
            .route("/ws", routing::get(Self::handle))
            .layer(Self::cors_layer(cors_origins))
            .with_state(services)
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
                    eprintln!("WARN: Ignoring invalid CORS origin '{origin}': {error}");
                    None
                }
            })
            .collect();

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(Any)
            .allow_headers(Any)
    }

    async fn upgrade(socket: WebSocket, services: Arc<Services>) {
        let (sink, stream) = socket.split();
        let sender = WebSocketSender { sink };
        let receiver = WebSocketReceiver { stream };

        connection::run(receiver, sender, services).await;
    }

    async fn handle(
        websocket: WebSocketUpgrade,
        State(services): State<Arc<Services>>,
    ) -> impl IntoResponse {
        websocket.on_upgrade(move |socket| Self::upgrade(socket, services))
    }
}
