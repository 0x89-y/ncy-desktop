use std::collections::HashMap;
use std::net::IpAddr;

use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceInfo};

/// Lifetime guard for an mDNS-SD registration. Drop unregisters the service.
pub struct MdnsRegistration {
    daemon: ServiceDaemon,
    fullname: String,
}

impl MdnsRegistration {
    /// Advertise this desktop as `_ncy-sync._tcp.local.` so phones on the same LAN
    /// can list it without needing the user to type its IP. The TXT record carries
    /// the desktop's stable `deviceId` so reconnecting peers recognize it across IP
    /// changes (DHCP renewal, switching networks, etc.).
    pub fn announce(
        instance_name: &str,
        host_ip: &str,
        port: u16,
        device_id: &str,
    ) -> Result<Self> {
        let daemon = ServiceDaemon::new().context("create mDNS daemon")?;

        let mut props: HashMap<String, String> = HashMap::new();
        props.insert("deviceId".to_string(), device_id.to_string());

        // mdns-sd insists on a `<host>.local.` for the host record. Use the device id
        // as the host stem so two desktops with the same friendly name don't collide.
        let host_label = format!("ncy-{}.local.", &device_id[..device_id.len().min(8)]);

        let addrs: Vec<IpAddr> = host_ip
            .parse::<IpAddr>()
            .ok()
            .into_iter()
            .collect();

        let info = ServiceInfo::new(
            "_ncy-sync._tcp.local.",
            instance_name,
            &host_label,
            &addrs[..],
            port,
            Some(props),
        )
        .context("build ServiceInfo")?;
        let fullname = info.get_fullname().to_string();
        daemon.register(info).context("register mDNS service")?;
        tracing::info!("mDNS announced as {fullname} on {host_ip}:{port}");
        Ok(Self { daemon, fullname })
    }
}

impl Drop for MdnsRegistration {
    fn drop(&mut self) {
        let _ = self.daemon.unregister(&self.fullname);
        let _ = self.daemon.shutdown();
    }
}
