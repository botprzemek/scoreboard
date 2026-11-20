use crate::common::{
    ConfigStore, ConnectedPacketResult, HandshakePacketResult, MASTER_UDP_PORT, StoredConfig,
    WORKER_UDP_PORT, WorkerProtocol,
};
use crate::rp235x::platform::FlashConfigStore;

use embassy_net::udp::{PacketMetadata, UdpMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Stack};
use embassy_rp::Peri;
use embassy_rp::peripherals::{FLASH, WATCHDOG};
use embassy_rp::watchdog::Watchdog;
use embassy_time::{Duration, Ticker, Timer};
use scoreboard_net::DeviceConfigPacket;
use static_cell::StaticCell;

static UDP_RX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();
static UDP_TX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();

#[embassy_executor::task]
pub async fn unified_udp_task(
    stack: Stack<'static>,
    watchdog_peri: Peri<'static, WATCHDOG>,
    flash_peri: Peri<'static, FLASH>,
    is_ap_mode: bool,
) {
    let receive_buffer = UDP_RX_BUFFER.init([0; 1024]);
    let transmit_buffer = UDP_TX_BUFFER.init([0; 1024]);

    let mut receive_metadata = [PacketMetadata::EMPTY; 4];
    let mut transmit_metadata = [PacketMetadata::EMPTY; 4];

    let mut socket = UdpSocket::new(
        stack,
        &mut receive_metadata,
        receive_buffer,
        &mut transmit_metadata,
        transmit_buffer,
    );

    if let Err(error) = socket.bind(WORKER_UDP_PORT) {
        defmt::error!("socket_error: {:?}", error);
        return;
    }

    defmt::info!(
        "[WORKER UDP] Bound to port {} (AP Mode: {})",
        WORKER_UDP_PORT,
        is_ap_mode
    );

    let mut watchdog = Watchdog::new(watchdog_peri);
    let mut store = FlashConfigStore::new(flash_peri);
    let mut packet_buffer = [0u8; 512];

    if is_ap_mode {
        loop {
            let (size, remote_endpoint) = match socket.recv_from(&mut packet_buffer).await {
                Ok(res) => res,
                Err(e) => {
                    defmt::warn!("UDP recv error: {:?}", e);
                    continue;
                }
            };

            if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(&packet_buffer[..size]) {
                handle_device_config(
                    &mut socket,
                    remote_endpoint,
                    config_pkt,
                    &mut store,
                    &mut watchdog,
                )
                .await;
            }
        }
    }

    let device = store
        .read()
        .and_then(|cfg| cfg.to_device())
        .unwrap_or_else(|| WorkerProtocol::fallback_device("0000-0000", "hmi-worker"));
    let mut protocol = WorkerProtocol::new(device);

    let broadcast_endpoint = IpEndpoint::new(IpAddress::v4(255, 255, 255, 255), MASTER_UDP_PORT);

    loop {
        defmt::info!("[WORKER] Szukanie mastera w sieci (Broadcast)...");

        let master_endpoint = loop {
            let packet_bytes = protocol.next_handshake_request();
            if let Err(error) = socket.send_to(&packet_bytes, broadcast_endpoint).await {
                defmt::error!("broadcast_send_error: {:?}", error);
            }

            let receive_future = socket.recv_from(&mut packet_buffer);
            let (size, remote_endpoint) = match embassy_time::with_timeout(
                Duration::from_millis(500),
                receive_future,
            )
            .await
            {
                Ok(Ok(res)) => res,
                Ok(Err(_)) | Err(_) => continue,
            };

            match protocol.handle_handshake_packet(&packet_buffer[..size]) {
                HandshakePacketResult::DeviceConfig(config_pkt) => {
                    handle_device_config(
                        &mut socket,
                        remote_endpoint,
                        config_pkt,
                        &mut store,
                        &mut watchdog,
                    )
                    .await;
                }
                HandshakePacketResult::Connected {
                    session_token,
                    assigned_game_id,
                } => {
                    defmt::info!(
                        "[WORKER] Znaleziono mastera pod adresem: {}. Handshake zakończony (Token: {:x}, GameId: {})!",
                        remote_endpoint,
                        session_token,
                        assigned_game_id
                    );
                    break remote_endpoint.endpoint;
                }
                HandshakePacketResult::Ignored => {}
            }
        };

        defmt::info!("[WORKER] Przejście do głównej pętli odbierania danych.");

        let mut keepalive_timer = Ticker::every(Duration::from_secs(3));

        loop {
            let receive_future = socket.recv_from(&mut packet_buffer);
            let tick_future = keepalive_timer.next();

            match embassy_futures::select::select(receive_future, tick_future).await {
                embassy_futures::select::Either::First(Ok((size, remote_endpoint))) => {
                    let raw = &packet_buffer[..size];
                    if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(raw) {
                        handle_device_config(
                            &mut socket,
                            remote_endpoint,
                            config_pkt,
                            &mut store,
                            &mut watchdog,
                        )
                        .await;
                        continue;
                    }

                    if remote_endpoint.endpoint.addr != master_endpoint.addr {
                        continue;
                    }

                    if let ConnectedPacketResult::InvalidToken =
                        protocol.handle_connected_packet(raw)
                    {
                        defmt::warn!("[WORKER] Odrzucono pakiet UDP: nieprawidłowy token sesji!");
                    }
                }
                embassy_futures::select::Either::First(Err(error)) => {
                    defmt::error!("udp_rx_error: {:?}", error);
                }
                embassy_futures::select::Either::Second(()) => {
                    let packet_bytes = protocol.next_heartbeat();
                    if let Err(error) = socket.send_to(&packet_bytes, master_endpoint).await {
                        defmt::error!("keepalive_send_error: {:?}", error);
                    } else if protocol.record_heartbeat_sent() {
                        defmt::info!(
                            "[WORKER] Brak odpowiedzi od Mastera przez 9s! Utracono połączenie."
                        );
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_device_config(
    socket: &mut UdpSocket<'_>,
    remote_endpoint: UdpMetadata,
    packet: DeviceConfigPacket,
    store: &mut FlashConfigStore<'_>,
    watchdog: &mut Watchdog,
) {
    defmt::info!(
        "[WORKER] Received DeviceConfigPacket from {}",
        remote_endpoint
    );

    let _ = socket
        .send_to(
            &WorkerProtocol::device_config_ack_bytes(),
            remote_endpoint.endpoint,
        )
        .await;

    let Some(cfg) = StoredConfig::from_packet(&packet) else {
        defmt::error!("[WORKER] Invalid Config fields");
        return;
    };

    defmt::info!(
        "[WORKER] Applying config: SN='{}', ID='{}', SSID='{}'",
        cfg.serial_number.as_str(),
        cfg.device_id.as_str(),
        cfg.wifi_ssid.as_str()
    );

    if store.write(&cfg).is_ok() {
        defmt::info!("[WORKER] Flash programmed successfully! Sending DeviceConfigSuccess...");

        let _ = socket
            .send_to(
                &WorkerProtocol::device_config_success_bytes(),
                remote_endpoint.endpoint,
            )
            .await;

        Timer::after_millis(300).await;
        defmt::info!("[WORKER] Rebooting via Watchdog into new network...");
        watchdog.trigger_reset();
    } else {
        defmt::error!("[WORKER] Failed to write config to flash!");
    }
}
