use crate::common::dhcp::build_dhcp_reply;

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpAddress, IpEndpoint, Ipv4Address, Stack};

#[embassy_executor::task]
pub async fn dhcp_task(stack: Stack<'static>) -> ! {
    run_dhcp_server(stack).await
}

pub async fn run_dhcp_server(stack: Stack<'static>) -> ! {
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx_buf = [0u8; 1024];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_buf = [0u8; 1024];

    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);

    if let Err(e) = socket.bind(67) {
        log::error!("Failed to bind DHCP socket on port 67: {:?}", e);
        loop {
            embassy_time::Timer::after_secs(60).await;
        }
    }

    let mut packet_buf = [0u8; 1024];
    let broadcast_endpoint = IpEndpoint {
        addr: IpAddress::Ipv4(Ipv4Address::new(255, 255, 255, 255)),
        port: 68,
    };

    loop {
        let Ok((n, _remote_endpoint)) = socket.recv_from(&mut packet_buf).await else {
            continue;
        };

        let Some(reply) = build_dhcp_reply(&packet_buf[..n]) else {
            continue;
        };

        let _ = socket
            .send_to(&reply.payload[..reply.len], broadcast_endpoint)
            .await;
    }
}
