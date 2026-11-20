use crate::common::{
    ConfigStore, ConnectedPacketResult, HandshakePacketResult, Input, MASTER_UDP_PORT, Machine,
    StoredConfig, WORKER_UDP_PORT, WorkerProtocol,
};
use crate::linux::platform::{CORE1_READY_SIGNAL, MemoryConfigStore};

use scoreboard_net::DeviceConfigPacket;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time;

pub struct Core0;

impl Core0 {
    pub fn entry() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime for Core 0");

        rt.block_on(async {
            Self::task().await;
        });
    }

    async fn task() {
        let mut machine = Machine::new();
        let mut store = MemoryConfigStore::new();

        println!("🔧 [Core 0 Thread] [BOOT] Initializing Linux worker state...");
        log_transition(&mut machine, Input::BootSuccess);

        println!("⏳ [Core 0 Thread] [SYNC] Waiting for Core 1 ready signal...");
        CORE1_READY_SIGNAL.wait();
        println!("✅ [Core 0 Thread] [SYNC] Core 1 is ready! Handshake complete.");
        log_transition(&mut machine, Input::CoreSynced);

        println!("🌐 [Core 0 Thread] [NETWORKING] Initializing network stack...");
        log_transition(&mut machine, Input::WifiConnected);
        println!(
            "🚀 [Core 0 Thread] [RUNNING] Starting UDP worker loop on port {WORKER_UDP_PORT}..."
        );

        if let Err(e) = run_udp_worker(&mut store).await {
            eprintln!("⚠️ [Core 0 Thread] Networking error: {e}");
            log_transition(&mut machine, Input::ErrorOccurred);
        }
    }
}

fn log_transition(machine: &mut Machine, input: Input) {
    println!("[STATE MACHINE] Input::{input:?}");
    let _ = machine.transition(input);
}

pub async fn run_udp_worker<S: ConfigStore>(
    store: &mut S,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind_addr = format!("0.0.0.0:{WORKER_UDP_PORT}");
    let socket = UdpSocket::bind(&bind_addr).await?;
    socket.set_broadcast(true)?;
    println!("📡 [Core 0 Thread] UDP Worker socket bound to {bind_addr}");

    let device = store
        .read()
        .and_then(|cfg| cfg.to_device())
        .unwrap_or_else(|| WorkerProtocol::fallback_device("0000-0001", "worker-linux"));

    let mut protocol = WorkerProtocol::new(device);
    let broadcast_target: SocketAddr = format!("255.255.255.255:{MASTER_UDP_PORT}").parse()?;
    let mut receive_buffer = [0u8; 1024];

    println!("🔍 [Core 0 Thread] Searching for Master server on {broadcast_target}...");

    let mut master_address: Option<SocketAddr> = None;
    let mut heartbeat_interval = time::interval(Duration::from_millis(500));

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                let target = master_address.unwrap_or(broadcast_target);
                let is_connected = master_address.is_some() && protocol.session_token() != 0;
                let packet = if is_connected {
                    protocol.next_heartbeat()
                } else {
                    protocol.next_handshake_request()
                };
                if socket.send_to(&packet, target).await.is_ok()
                    && is_connected
                    && protocol.record_heartbeat_sent()
                {
                    println!("⚠️ [Core 0 Thread] Lost Master connection (missed heartbeats)");
                    master_address = None;
                }
            }

            recv_res = socket.recv_from(&mut receive_buffer) => {
                let (len, remote_addr) = match recv_res {
                    Ok(res) => res,
                    Err(e) => {
                        eprintln!("⚠️ [Core 0 Thread] UDP receive error: {e}");
                        continue;
                    }
                };

                let raw = &receive_buffer[..len];
                if master_address.is_none() {
                    match protocol.handle_handshake_packet(raw) {
                        HandshakePacketResult::DeviceConfig(config_pkt) => {
                            handle_device_config(&socket, remote_addr, config_pkt, store).await;
                        }
                        HandshakePacketResult::Connected { session_token, assigned_game_id } => {
                            println!(
                                "🎉 [Core 0 Thread] Connected to Master server at {remote_addr} (token={session_token:#x}, game_id={assigned_game_id})"
                            );
                            master_address = Some(remote_addr);
                        }
                        HandshakePacketResult::Ignored => {}
                    }
                } else {
                    match protocol.handle_connected_packet(raw) {
                        ConnectedPacketResult::DeviceConfig(config_pkt) => {
                            handle_device_config(&socket, remote_addr, config_pkt, store).await;
                        }
                        ConnectedPacketResult::HeartbeatAck
                        | ConnectedPacketResult::HandshakeAck
                        | ConnectedPacketResult::GameUpdated
                        | ConnectedPacketResult::InvalidToken
                        | ConnectedPacketResult::Ignored => {}
                    }
                }
            }
        }
    }
}

async fn handle_device_config<S: ConfigStore>(
    socket: &UdpSocket,
    remote_addr: SocketAddr,
    packet: DeviceConfigPacket,
    store: &mut S,
) {
    println!("⚙️ [Core 0 Thread] Received DeviceConfigPacket from {remote_addr}");
    let _ = socket
        .send_to(&WorkerProtocol::device_config_ack_bytes(), remote_addr)
        .await;

    let Some(cfg) = StoredConfig::from_packet(&packet) else {
        return;
    };

    if store.write(&cfg).is_ok() {
        println!(
            "📋 [Core 0 Thread] Applied new config: SN='{}', ID='{}', SSID='{}'",
            cfg.serial_number, cfg.device_id, cfg.wifi_ssid
        );
        let _ = socket
            .send_to(&WorkerProtocol::device_config_success_bytes(), remote_addr)
            .await;
    }
}
