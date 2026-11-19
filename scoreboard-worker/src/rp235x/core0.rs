use crate::common::{Input, Machine, StoredConfig};
use crate::rp235x::cyw43::{Cyw43, cyw43_task};
use crate::rp235x::dhcp::dhcp_task;
use crate::rp235x::net::{Net, net_task};
use crate::rp235x::peripherals::{NetPeripherals, TrngPeripherals};
use crate::rp235x::platform::{CORE1_READY_SIGNAL, FlashConfigStore, STATE_SIGNAL};
use crate::rp235x::udp::unified_udp_task;

use config::Config;
use config::flash::FlashReader;
use config::print::Print;
use embassy_executor::{Executor, Spawner};
use embassy_net::{Ipv4Address, Ipv4Cidr, StaticConfigV4};
use embassy_rp::Peri;
use embassy_rp::peripherals::{FLASH, WATCHDOG};
use embassy_time::Timer;
use static_cell::StaticCell;

pub static EXECUTOR: StaticCell<Executor> = StaticCell::new();

pub struct Core0;

impl Core0 {
    pub fn entry(
        net: NetPeripherals,
        trng: TrngPeripherals,
        watchdog: Peri<'static, WATCHDOG>,
        flash: Peri<'static, FLASH>,
    ) -> ! {
        let executor = Executor::new();

        EXECUTOR.init(executor).run(|spawner| {
            spawner.spawn(defmt::unwrap!(core0_task(
                spawner, net, trng, watchdog, flash
            )));
        })
    }
}

fn transition(machine: &mut Machine, input: Input) {
    defmt::debug!("Input::{:?}", defmt::Debug2Format(&input));
    if let Some(new_state) = machine.transition(input) {
        STATE_SIGNAL.signal(new_state);
    }
}

#[embassy_executor::task]
async fn core0_task(
    spawner: Spawner,
    net: NetPeripherals,
    trng: TrngPeripherals,
    watchdog: Peri<'static, WATCHDOG>,
    flash: Peri<'static, FLASH>,
) {
    let mut machine = Machine::new();

    // Boot
    match Config::read() {
        Ok(config) if config.print().is_ok() => transition(&mut machine, Input::BootSuccess),
        _ => transition(&mut machine, Input::BootFailed),
    }

    // Sync with Core 1
    CORE1_READY_SIGNAL.wait().await;
    transition(&mut machine, Input::CoreSynced);

    // Networking
    let seed = TrngPeripherals::init(trng).await;
    let mut cyw43_driver = Cyw43::init(net).await;
    spawner.spawn(defmt::unwrap!(cyw43_task(cyw43_driver.runner)));

    let initialized_net =
        Net::init_stack(&mut cyw43_driver.control, cyw43_driver.net_driver, seed).await;
    spawner.spawn(defmt::unwrap!(net_task(initialized_net.runner)));

    let stored_config = FlashConfigStore::read_static();
    let ap_ssid = StoredConfig::ap_ssid(stored_config.as_ref());

    let mut connected_sta = false;
    if let Ok(config) = Config::read() {
        defmt::info!("Attempting connection with existing config...");
        if Net::try_join_wifi(&mut cyw43_driver.control, config, 3).await {
            initialized_net.stack.wait_link_up().await;

            let mut dhcp_ok = false;
            for _ in 0..10 {
                if initialized_net.stack.is_config_up() {
                    dhcp_ok = true;
                    break;
                }
                Timer::after_millis(500).await;
            }

            if !dhcp_ok {
                return;
            }

            let Some(config_v4) = initialized_net.stack.config_v4() else {
                return;
            };

            defmt::info!(
                "NetworkConfig::Address               {}",
                &config_v4.address
            );
            if let Some(ref gateway) = config_v4.gateway {
                defmt::info!("NetworkConfig::Gateway               {}", gateway);
            }

            transition(&mut machine, Input::WifiConnected);
            connected_sta = true;
        }
    }

    if connected_sta {
        spawner.spawn(defmt::unwrap!(unified_udp_task(
            initialized_net.stack,
            watchdog,
            flash,
            false
        )));
    } else {
        defmt::warn!("Wi-Fi connection unavailable. Launching AP Setup Hotspot...");
        transition(&mut machine, Input::WifiFailed);

        let ap_ip = Ipv4Address::new(192, 168, 4, 1);
        let subnet = Ipv4Cidr::new(ap_ip, 24);
        initialized_net
            .stack
            .set_config_v4(embassy_net::ConfigV4::Static(StaticConfigV4 {
                address: subnet,
                gateway: Some(ap_ip),
                dns_servers: heapless::Vec::new(),
            }));

        cyw43_driver.control.start_ap_open(&ap_ssid, 1).await;
        defmt::info!(
            "AP '{}' started! Listening on UDP port 9000...",
            ap_ssid.as_str()
        );

        spawner.spawn(defmt::unwrap!(dhcp_task(initialized_net.stack)));
        spawner.spawn(defmt::unwrap!(unified_udp_task(
            initialized_net.stack,
            watchdog,
            flash,
            true
        )));
    }
}
