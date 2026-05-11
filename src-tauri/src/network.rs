use std::net::IpAddr;

/// Lower is better. Mirrors the Python reference's `_ip_rank`: prefer 192.168/x,
/// then 172.16-31/x, then 10/x (often VPN), then everything else.
fn rank(ip: &str) -> u8 {
    if ip.starts_with("192.168.") {
        return 0;
    }
    let parts: Vec<&str> = ip.split('.').collect();
    if ip.starts_with("172.") && parts.len() == 4 {
        if let Ok(b) = parts[1].parse::<u8>() {
            if (16..=31).contains(&b) {
                return 1;
            }
        }
    }
    if ip.starts_with("10.") {
        return 2;
    }
    3
}

pub fn detect_lan_ips() -> Vec<String> {
    let mut ips: Vec<String> = match local_ip_address::list_afinet_netifas() {
        Ok(list) => list
            .into_iter()
            .filter_map(|(_, ip)| match ip {
                IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_link_local() => {
                    Some(v4.to_string())
                }
                _ => None,
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    ips.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.cmp(b)));
    ips.dedup();
    ips
}

pub fn best_lan_ip() -> String {
    detect_lan_ips()
        .into_iter()
        .next()
        .unwrap_or_else(|| "127.0.0.1".to_string())
}
