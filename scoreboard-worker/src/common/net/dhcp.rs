pub const AP_SERVER_IP: [u8; 4] = [192, 168, 4, 1];
pub const AP_OFFERED_IP: [u8; 4] = [192, 168, 4, 2];
pub const AP_SUBNET_MASK: [u8; 4] = [255, 255, 255, 0];

pub struct DhcpReply {
    pub msg_type: u8,
    pub reply_type: u8,
    pub payload: [u8; 300],
    pub len: usize,
}

pub fn build_dhcp_reply(packet: &[u8]) -> Option<DhcpReply> {
    if packet.len() < 240 || packet[0] != 1 {
        return None;
    }

    if packet[236..240] != [0x63, 0x82, 0x53, 0x63] {
        return None;
    }

    let xid = &packet[4..8];
    let chaddr = &packet[28..44];

    let mut msg_type = 0u8;
    let mut idx = 240;
    while idx < packet.len() {
        let opt = packet[idx];
        if opt == 255 {
            break;
        }
        if opt == 0 {
            idx += 1;
            continue;
        }
        if idx + 1 >= packet.len() {
            break;
        }
        let len = packet[idx + 1] as usize;
        if idx + 2 + len > packet.len() {
            break;
        }
        if opt == 53 && len == 1 {
            msg_type = packet[idx + 2];
        }
        idx += 2 + len;
    }

    let reply_type = match msg_type {
        1 => 2,
        3 => 5,
        _ => return None,
    };

    let mut resp = [0u8; 300];
    resp[0] = 2;
    resp[1] = 1;
    resp[2] = 6;
    resp[3] = 0;
    resp[4..8].copy_from_slice(xid);
    resp[8..10].copy_from_slice(&[0, 0]);
    resp[10..12].copy_from_slice(&[0x80, 0x00]);
    resp[16..20].copy_from_slice(&AP_OFFERED_IP);
    resp[20..24].copy_from_slice(&AP_SERVER_IP);
    resp[28..44].copy_from_slice(chaddr);
    resp[236..240].copy_from_slice(&[0x63, 0x82, 0x53, 0x63]);

    let mut opt_idx = 240;
    resp[opt_idx..opt_idx + 3].copy_from_slice(&[53, 1, reply_type]);
    opt_idx += 3;

    resp[opt_idx..opt_idx + 6].copy_from_slice(&[
        54,
        4,
        AP_SERVER_IP[0],
        AP_SERVER_IP[1],
        AP_SERVER_IP[2],
        AP_SERVER_IP[3],
    ]);
    opt_idx += 6;

    resp[opt_idx..opt_idx + 6].copy_from_slice(&[51, 4, 0x00, 0x01, 0x51, 0x80]);
    opt_idx += 6;

    resp[opt_idx..opt_idx + 6].copy_from_slice(&[
        1,
        4,
        AP_SUBNET_MASK[0],
        AP_SUBNET_MASK[1],
        AP_SUBNET_MASK[2],
        AP_SUBNET_MASK[3],
    ]);
    opt_idx += 6;

    resp[opt_idx..opt_idx + 6].copy_from_slice(&[
        3,
        4,
        AP_SERVER_IP[0],
        AP_SERVER_IP[1],
        AP_SERVER_IP[2],
        AP_SERVER_IP[3],
    ]);
    opt_idx += 6;

    resp[opt_idx..opt_idx + 6].copy_from_slice(&[
        6,
        4,
        AP_SERVER_IP[0],
        AP_SERVER_IP[1],
        AP_SERVER_IP[2],
        AP_SERVER_IP[3],
    ]);
    opt_idx += 6;

    resp[opt_idx] = 255;
    opt_idx += 1;

    Some(DhcpReply {
        msg_type,
        reply_type,
        payload: resp,
        len: opt_idx,
    })
}
