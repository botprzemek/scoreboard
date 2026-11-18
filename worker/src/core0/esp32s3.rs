use core::sync::atomic::Ordering;

use crate::core0::dhcp::run_dhcp_server;
use crate::state::{GAME_STATE, IS_CONNECTED_TO_MASTER};

use basketball::Game;
use config::ConfigBuilder;
use embassy_executor::Spawner;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{Config as NetConfig, DhcpConfig, IpAddress, IpEndpoint, Ipv4Address, Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Ticker, Timer};
use embedded_storage::{ReadStorage, Storage};
use esp_hal::peripherals::{FLASH, WIFI};
use esp_hal::rng::Rng;
use esp_hal::system::AppCoreGuard;
use esp_radio::wifi::{
    Config as WifiConfig, ControllerConfig, Interface, WifiController, ap::AccessPointConfig,
    sta::StationConfig,
};
use esp_storage::FlashStorage;
use heapless::{String, Vec};
use net::event::{ClientEvent, ServerEvent};
use net::layout::PACKET_SIZE;
use net::{
    ClientPacket, Device, DeviceConfigPacket, DeviceType, Heartbeat, ServerPacket,
};
use static_cell::StaticCell;

const CONFIG_FLASH_OFFSET: u32 = 0x3F_0000;
const CONFIG_MAGIC: [u8; 4] = *b"SBC1";
const CONFIG_RECORD_LEN: usize = 256;

static SOCKETS: StaticCell<StackResources<5>> = StaticCell::new();
static UDP_RX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();
static UDP_TX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();

struct StoredConfig {
    serial_number: String<32>,
    device_id: String<32>,
    wifi_ssid: String<64>,
    wifi_password: String<64>,
}

fn read_stored_config(flash: &mut FlashStorage<'_>) -> Option<StoredConfig> {
    let mut buf = [0xFFu8; CONFIG_RECORD_LEN];
    if let Err(e) = flash.read(CONFIG_FLASH_OFFSET, &mut buf) {
        log::error!("Flash read failed at {CONFIG_FLASH_OFFSET:#x}: {:?}", e);
        return None;
    }
    if buf[..4] != CONFIG_MAGIC {
        log::warn!("No valid config in flash (magic={:?})", &buf[..4]);
        return None;
    }

    let sn_len = usize::from(buf[4]);
    let id_len = usize::from(buf[5]);
    let ssid_len = usize::from(buf[6]);
    let pass_len = usize::from(buf[7]);

    let total = 8usize + sn_len + id_len + ssid_len + pass_len;
    if total > CONFIG_RECORD_LEN {
        return None;
    }

    let mut pos = 8usize;
    let sn_str = core::str::from_utf8(&buf[pos..pos + sn_len]).ok()?;
    pos += sn_len;
    let id_str = core::str::from_utf8(&buf[pos..pos + id_len]).ok()?;
    pos += id_len;
    let ssid_str = core::str::from_utf8(&buf[pos..pos + ssid_len]).ok()?;
    pos += ssid_len;
    let pass_str = core::str::from_utf8(&buf[pos..pos + pass_len]).ok()?;

    let mut builder = ConfigBuilder::new();
    builder
        .serial_number(sn_str)
        .device_id(id_str)
        .wifi_ssid(ssid_str)
        .wifi_password(pass_str);
    builder.build().ok()?;

    let mut serial_number = String::new();
    let mut device_id = String::new();
    let mut wifi_ssid = String::new();
    let mut wifi_password = String::new();

    serial_number.push_str(sn_str).ok()?;
    device_id.push_str(id_str).ok()?;
    wifi_ssid.push_str(ssid_str).ok()?;
    wifi_password.push_str(pass_str).ok()?;

    Some(StoredConfig {
        serial_number,
        device_id,
        wifi_ssid,
        wifi_password,
    })
}

fn write_stored_config(
    flash: &mut FlashStorage<'_>,
    sn: &str,
    id: &str,
    ssid: &str,
    pass: &str,
) -> Result<(), ()> {
    let mut builder = ConfigBuilder::new();
    builder
        .serial_number(sn)
        .device_id(id)
        .wifi_ssid(ssid)
        .wifi_password(pass);
    builder.build().map_err(|_| ())?;

    let sn_b = sn.as_bytes();
    let id_b = id.as_bytes();
    let ssid_b = ssid.as_bytes();
    let pass_b = pass.as_bytes();

    if sn_b.len() > u8::MAX as usize
        || id_b.len() > u8::MAX as usize
        || ssid_b.len() > u8::MAX as usize
        || pass_b.len() > u8::MAX as usize
    {
        return Err(());
    }

    let total = 8 + sn_b.len() + id_b.len() + ssid_b.len() + pass_b.len();
    if total > CONFIG_RECORD_LEN {
        return Err(());
    }

    let mut buf = [0xFFu8; CONFIG_RECORD_LEN];
    buf[..4].copy_from_slice(&CONFIG_MAGIC);
    buf[4] = sn_b.len() as u8;
    buf[5] = id_b.len() as u8;
    buf[6] = ssid_b.len() as u8;
    buf[7] = pass_b.len() as u8;

    let mut pos = 8usize;
    buf[pos..pos + sn_b.len()].copy_from_slice(sn_b);
    pos += sn_b.len();
    buf[pos..pos + id_b.len()].copy_from_slice(id_b);
    pos += id_b.len();
    buf[pos..pos + ssid_b.len()].copy_from_slice(ssid_b);
    pos += ssid_b.len();
    buf[pos..pos + pass_b.len()].copy_from_slice(pass_b);

    if let Err(e) = flash.write(CONFIG_FLASH_OFFSET, &buf) {
        log::error!("Flash write failed at {CONFIG_FLASH_OFFSET:#x}: {:?}", e);
        return Err(());
    }

    let mut verify = [0u8; CONFIG_RECORD_LEN];
    if let Err(e) = flash.read(CONFIG_FLASH_OFFSET, &mut verify) {
        log::error!("Flash verify read failed: {:?}", e);
        return Err(());
    }
    if verify != buf {
        log::error!("Flash verify mismatch after write!");
        return Err(());
    }

    Ok(())
}

#[embassy_executor::task]
async fn esp_net_task(mut runner: Runner<'static, Interface<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn esp_dhcp_task(stack: Stack<'static>) -> ! {
    run_dhcp_server(stack).await
}

pub async fn run_esp32s3_core0(
    spawner: Spawner,
    wifi_peri: WIFI<'static>,
    flash_peri: FLASH<'static>,
    app_core_guard: AppCoreGuard<'static>,
) -> ! {
    let mut flash = FlashStorage::new(flash_peri);
    let stored = read_stored_config(&mut flash);

    let (mut controller, interfaces) =
        esp_radio::wifi::new(wifi_peri, ControllerConfig::default())
            .expect("failed to init Wi-Fi");

    let rng = Rng::new();
    let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());

    let mut ap_ssid = String::<32>::new();
    if let Some(ref cfg) = stored
        && !cfg.serial_number.is_empty()
    {
        let _ = ap_ssid.push_str(cfg.serial_number.as_str());
    }
    if ap_ssid.is_empty() {
        let _ = ap_ssid.push_str("Worker-Setup");
    }

    let mut connected_sta = false;
    let mut sta_stack: Option<Stack<'static>> = None;

    if let Some(ref cfg) = stored
        && !cfg.wifi_ssid.is_empty()
        && !cfg.wifi_password.is_empty()
    {
        let client_cfg = WifiConfig::Station(
            StationConfig::default()
                .with_ssid(cfg.wifi_ssid.as_str())
                .with_password(cfg.wifi_password.as_str().into()),
        );
        if controller.set_config(&client_cfg).is_ok() {
            log::info!("Connecting to Wi-Fi '{}'...", cfg.wifi_ssid);
            for attempt in 1..=3 {
                if controller.connect_async().await.is_ok() {
                    log::info!("Wi-Fi connected on attempt {attempt}");
                    let net_cfg = NetConfig::dhcpv4(DhcpConfig::default());
                    let resources = SOCKETS.init(StackResources::<5>::new());
                    let (stack, runner) =
                        embassy_net::new(interfaces.station, net_cfg, resources, seed);
                    spawner.spawn(esp_net_task(runner).expect("net task"));

                    for _ in 0..10 {
                        if stack.is_config_up() {
                            if let Some(v4) = stack.config_v4() {
                                log::info!("DHCP IPv4: {}", v4.address);
                            }
                            connected_sta = true;
                            sta_stack = Some(stack);
                            break;
                        }
                        Timer::after_millis(500).await;
                    }
                    break;
                }
                log::warn!("Wi-Fi connect attempt {attempt}/3 failed");
                Timer::after_secs(1).await;
            }
        }
    }

    if let (true, Some(stack)) = (connected_sta, sta_stack) {
        let device_bytes = stored
            .as_ref()
            .and_then(|cfg| {
                Device::try_new(
                    DeviceType::Worker,
                    cfg.serial_number.as_str(),
                    cfg.device_id.as_str(),
                )
                .ok()
            })
            .unwrap_or_else(|| {
                Device::try_new(DeviceType::Worker, "0000-0000", "esp32s3-worker")
                    .expect("valid default worker device")
            })
            .to_bytes();

        run_sta_udp(stack, &mut flash, device_bytes, app_core_guard).await;
    } else {
        log::warn!("Wi-Fi unavailable; starting AP '{}'", ap_ssid);
        let ap_cfg = WifiConfig::AccessPoint(
            AccessPointConfig::default().with_ssid(ap_ssid.as_str()),
        );
        let _ = controller.set_config(&ap_cfg);

        let ap_ip = Ipv4Address::new(192, 168, 4, 1);
        let static_cfg = NetConfig::ipv4_static(StaticConfigV4 {
            address: Ipv4Cidr::new(ap_ip, 24),
            gateway: Some(ap_ip),
            dns_servers: Vec::new(),
        });
        let resources = SOCKETS.init(StackResources::<5>::new());
        let (stack, runner) =
            embassy_net::new(interfaces.access_point, static_cfg, resources, seed);
        spawner.spawn(esp_net_task(runner).expect("net task"));
        spawner.spawn(esp_dhcp_task(stack).expect("dhcp task"));

        run_ap_udp(stack, &mut flash, &mut controller, app_core_guard).await;
    }
}

async fn run_ap_udp(
    stack: Stack<'static>,
    flash: &mut FlashStorage<'_>,
    _controller: &mut WifiController<'static>,
    app_core_guard: AppCoreGuard<'static>,
) -> ! {
    let mut app_core = Some(app_core_guard);
    let rx_buf = UDP_RX_BUFFER.init([0; 1024]);
    let tx_buf = UDP_TX_BUFFER.init([0; 1024]);
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, rx_buf, &mut tx_meta, tx_buf);
    let _ = socket.bind(9000);

    let mut packet_buffer = [0u8; 512];
    loop {
        let Ok((size, remote_endpoint)) = socket.recv_from(&mut packet_buffer).await else {
            continue;
        };

        if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(&packet_buffer[..size]) {
            handle_device_config(&mut socket, remote_endpoint, config_pkt, flash, &mut app_core)
                .await;
        }
    }
}

async fn run_sta_udp(
    stack: Stack<'static>,
    flash: &mut FlashStorage<'_>,
    device_bytes: [u8; net::layout::DATA_SIZE],
    app_core_guard: AppCoreGuard<'static>,
) -> ! {
    let mut app_core = Some(app_core_guard);
    let rx_buf = UDP_RX_BUFFER.init([0; 1024]);
    let tx_buf = UDP_TX_BUFFER.init([0; 1024]);
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, rx_buf, &mut tx_meta, tx_buf);
    let _ = socket.bind(9000);

    let mut packet_buffer = [0u8; 512];
    let broadcast_endpoint = IpEndpoint::new(IpAddress::v4(255, 255, 255, 255), 9000);

    loop {
        let (master_endpoint, mut session_token) = loop {
            let client_packet = ClientPacket::new(ClientEvent::HandshakeRequest, 0, device_bytes);
            let _ = socket
                .send_to(&client_packet.to_bytes(), broadcast_endpoint)
                .await;

            let receive_future = socket.recv_from(&mut packet_buffer);
            let Ok(Ok((size, remote_endpoint))) =
                embassy_time::with_timeout(Duration::from_millis(500), receive_future).await
            else {
                continue;
            };

            if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(&packet_buffer[..size]) {
                handle_device_config(&mut socket, remote_endpoint, config_pkt, flash, &mut app_core)
                    .await;
                continue;
            }

            if size != PACKET_SIZE {
                continue;
            }

            let Ok(server_packet) = ServerPacket::from_bytes(
                packet_buffer[..PACKET_SIZE]
                    .try_into()
                    .unwrap_or([0; PACKET_SIZE]),
            ) else {
                continue;
            };

            if server_packet.event() != ServerEvent::HandshakeAck {
                continue;
            }

            log::info!("Connected to Master at {}", remote_endpoint.endpoint);
            IS_CONNECTED_TO_MASTER.store(true, Ordering::Release);
            break (remote_endpoint.endpoint, server_packet.session_token());
        };

        let mut keepalive_timer = Ticker::every(Duration::from_secs(3));
        let mut missed_heartbeats: u8 = 0;
        let mut sequence_id: u16 = 0;
        const MAX_MISSED_HEARTBEATS: u8 = 3;

        loop {
            let receive_future = socket.recv_from(&mut packet_buffer);
            let tick_future = keepalive_timer.next();

            match embassy_futures::select::select(receive_future, tick_future).await {
                embassy_futures::select::Either::First(Ok((size, remote_endpoint))) => {
                    if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(&packet_buffer[..size]) {
                        handle_device_config(
                            &mut socket,
                            remote_endpoint,
                            config_pkt,
                            flash,
                            &mut app_core,
                        )
                        .await;
                        continue;
                    }

                    if size != PACKET_SIZE || remote_endpoint.endpoint.addr != master_endpoint.addr
                    {
                        continue;
                    }

                    let Ok(packet) = ServerPacket::from_bytes(
                        packet_buffer[..PACKET_SIZE]
                            .try_into()
                            .unwrap_or([0; PACKET_SIZE]),
                    ) else {
                        continue;
                    };

                    let packet_token = packet.session_token();
                    if session_token != 0 && packet_token != session_token {
                        continue;
                    }

                    match packet.event() {
                        ServerEvent::HeartbeatAck => {
                            missed_heartbeats = 0;
                        }
                        ServerEvent::HandshakeAck => {
                            missed_heartbeats = 0;
                            if packet_token != 0 {
                                session_token = packet_token;
                            }
                        }
                        ServerEvent::GameUpdate => {
                            missed_heartbeats = 0;
                            if let Ok(game) = Game::try_from(packet) {
                                GAME_STATE.set(game);
                            }
                        }
                        _ => {}
                    }
                }
                embassy_futures::select::Either::First(Err(_)) => {}
                embassy_futures::select::Either::Second(()) => {
                    sequence_id = sequence_id.wrapping_add(1);
                    let heartbeat_bytes = Heartbeat::new(0).to_bytes();
                    let keepalive_packet = ClientPacket::with_session(
                        ClientEvent::HandshakeHeartbeat,
                        sequence_id,
                        session_token,
                        Heartbeat::PAYLOAD_LEN,
                        heartbeat_bytes,
                    );

                    if socket
                        .send_to(&keepalive_packet.to_bytes(), master_endpoint)
                        .await
                        .is_ok()
                    {
                        missed_heartbeats += 1;
                        if missed_heartbeats >= MAX_MISSED_HEARTBEATS {
                            log::warn!("Lost Master connection (missed {MAX_MISSED_HEARTBEATS} heartbeats)");
                            IS_CONNECTED_TO_MASTER.store(false, Ordering::Release);
                            break;
                        }
                    }
                }
            }
        }
    }
}

async fn handle_device_config(
    socket: &mut UdpSocket<'_>,
    remote_endpoint: embassy_net::udp::UdpMetadata,
    packet: DeviceConfigPacket,
    flash: &mut FlashStorage<'_>,
    app_core: &mut Option<AppCoreGuard<'static>>,
) {
    let ack_packet = ServerPacket::new(
        ServerEvent::DeviceConfigAck,
        0,
        [0u8; net::layout::DATA_SIZE],
    );
    let _ = socket
        .send_to(&ack_packet.to_bytes(), remote_endpoint.endpoint)
        .await;

    let (Ok(sn), Ok(id), Ok(ssid), Ok(pass)) = (
        packet.serial_number(),
        packet.device_id(),
        packet.wifi_ssid(),
        packet.wifi_password(),
    ) else {
        return;
    };

    drop(app_core.take());

    if write_stored_config(flash, sn, id, ssid, pass).is_ok() {
        log::info!("Saved config SN='{sn}', ID='{id}', SSID='{ssid}'; rebooting");
        let success_packet = ServerPacket::new(
            ServerEvent::DeviceConfigSuccess,
            0,
            [0u8; net::layout::DATA_SIZE],
        );
        let _ = socket
            .send_to(&success_packet.to_bytes(), remote_endpoint.endpoint)
            .await;

        Timer::after_millis(300).await;
        esp_hal::system::software_reset();
    }
}
