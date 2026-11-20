use crate::common::{
    ConfigStore, HandshakePacketResult, MASTER_UDP_PORT, StoredConfig, WORKER_UDP_PORT,
    WorkerProtocol,
};
use crate::esp32s3::dhcp::dhcp_task;
use crate::esp32s3::platform::FlashConfigStore;

use embassy_executor::Spawner;
use embassy_net::udp::{PacketMetadata, UdpMetadata, UdpSocket};
use embassy_net::{
    Config as NetConfig, DhcpConfig, IpAddress, IpEndpoint, Ipv4Address, Ipv4Cidr, Runner, Stack,
    StackResources, StaticConfigV4,
};
use embassy_time::{Duration, Ticker, Timer};
use esp_hal::peripherals::{FLASH, WIFI};
use esp_hal::rng::Rng;
use esp_hal::system::AppCoreGuard;
use esp_radio::wifi::{
    Config as WifiConfig, ControllerConfig, Interface, WifiController, ap::AccessPointConfig,
    sta::StationConfig,
};
use heapless::Vec;
use scoreboard_net::DeviceConfigPacket;
use static_cell::StaticCell;

static SOCKETS: StaticCell<StackResources<5>> = StaticCell::new();
static UDP_RX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();
static UDP_TX_BUFFER: StaticCell<[u8; 1024]> = StaticCell::new();

#[embassy_executor::task]
async fn esp_net_task(mut runner: Runner<'static, Interface<'static>>) -> ! {
    runner.run().await
}

pub struct Core0;

impl Core0 {
    pub async fn run(
        spawner: Spawner,
        wifi_peri: WIFI<'static>,
        flash_peri: FLASH<'static>,
        app_core_guard: AppCoreGuard<'static>,
    ) -> ! {
        let mut store = FlashConfigStore::new(flash_peri);
        let stored = store.read();

        let (mut controller, interfaces) =
            esp_radio::wifi::new(wifi_peri, ControllerConfig::default())
                .expect("failed to init Wi-Fi");

        let rng = Rng::new();
        let seed = (u64::from(rng.random()) << 32) | u64::from(rng.random());
        let ap_ssid = StoredConfig::ap_ssid(stored.as_ref());

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
            let device = stored
                .as_ref()
                .and_then(|cfg| cfg.to_device())
                .unwrap_or_else(|| WorkerProtocol::fallback_device("0000-0000", "esp32s3-worker"));
            let protocol = WorkerProtocol::new(device);

            run_sta_udp(stack, &mut store, protocol, app_core_guard).await;
        } else {
            log::warn!("Wi-Fi unavailable; starting AP '{}'", ap_ssid);
            let ap_cfg =
                WifiConfig::AccessPoint(AccessPointConfig::default().with_ssid(ap_ssid.as_str()));
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
            spawner.spawn(dhcp_task(stack).expect("dhcp task"));

            run_ap_udp(stack, &mut store, &mut controller, app_core_guard).await;
        }
    }
}

async fn run_ap_udp(
    stack: Stack<'static>,
    store: &mut FlashConfigStore<'_>,
    _controller: &mut WifiController<'static>,
    app_core_guard: AppCoreGuard<'static>,
) -> ! {
    let mut app_core = Some(app_core_guard);
    let rx_buf = UDP_RX_BUFFER.init([0; 1024]);
    let tx_buf = UDP_TX_BUFFER.init([0; 1024]);
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, rx_buf, &mut tx_meta, tx_buf);
    let _ = socket.bind(WORKER_UDP_PORT);

    let mut packet_buffer = [0u8; 512];
    loop {
        let Ok((size, remote_endpoint)) = socket.recv_from(&mut packet_buffer).await else {
            continue;
        };

        if let Ok(config_pkt) = DeviceConfigPacket::from_bytes(&packet_buffer[..size]) {
            handle_device_config(
                &mut socket,
                remote_endpoint,
                config_pkt,
                store,
                &mut app_core,
            )
            .await;
        }
    }
}

async fn run_sta_udp(
    stack: Stack<'static>,
    store: &mut FlashConfigStore<'_>,
    mut protocol: WorkerProtocol,
    app_core_guard: AppCoreGuard<'static>,
) -> ! {
    let mut app_core = Some(app_core_guard);
    let rx_buf = UDP_RX_BUFFER.init([0; 1024]);
    let tx_buf = UDP_TX_BUFFER.init([0; 1024]);
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, rx_buf, &mut tx_meta, tx_buf);
    let _ = socket.bind(WORKER_UDP_PORT);

    let mut packet_buffer = [0u8; 512];
    let broadcast_endpoint = IpEndpoint::new(IpAddress::v4(255, 255, 255, 255), MASTER_UDP_PORT);

    loop {
        let master_endpoint = loop {
            let _ = socket
                .send_to(&protocol.next_handshake_request(), broadcast_endpoint)
                .await;

            let receive_future = socket.recv_from(&mut packet_buffer);
            let Ok(Ok((size, remote_endpoint))) =
                embassy_time::with_timeout(Duration::from_millis(500), receive_future).await
            else {
                continue;
            };

            match protocol.handle_handshake_packet(&packet_buffer[..size]) {
                HandshakePacketResult::DeviceConfig(config_pkt) => {
                    handle_device_config(
                        &mut socket,
                        remote_endpoint,
                        config_pkt,
                        store,
                        &mut app_core,
                    )
                    .await;
                }
                HandshakePacketResult::Connected { .. } => {
                    log::info!("Connected to Master at {}", remote_endpoint.endpoint);
                    break remote_endpoint.endpoint;
                }
                HandshakePacketResult::Ignored => {}
            }
        };

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
                            store,
                            &mut app_core,
                        )
                        .await;
                        continue;
                    }

                    if remote_endpoint.endpoint.addr != master_endpoint.addr {
                        continue;
                    }

                    let _ = protocol.handle_connected_packet(raw);
                }
                embassy_futures::select::Either::First(Err(_)) => {}
                embassy_futures::select::Either::Second(()) => {
                    let packet_bytes = protocol.next_heartbeat();
                    if socket.send_to(&packet_bytes, master_endpoint).await.is_ok()
                        && protocol.record_heartbeat_sent()
                    {
                        log::warn!("Lost Master connection (missed heartbeats)");
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
    app_core: &mut Option<AppCoreGuard<'static>>,
) {
    let _ = socket
        .send_to(
            &WorkerProtocol::device_config_ack_bytes(),
            remote_endpoint.endpoint,
        )
        .await;

    let Some(cfg) = StoredConfig::from_packet(&packet) else {
        return;
    };

    drop(app_core.take());

    if store.write(&cfg).is_ok() {
        log::info!(
            "Saved config SN='{}', ID='{}', SSID='{}'; rebooting",
            cfg.serial_number,
            cfg.device_id,
            cfg.wifi_ssid
        );
        let _ = socket
            .send_to(
                &WorkerProtocol::device_config_success_bytes(),
                remote_endpoint.endpoint,
            )
            .await;

        Timer::after_millis(300).await;
        esp_hal::system::software_reset();
    }
}
