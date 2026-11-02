//! Finds other devices in the group on the local network with mDNS.
use crate::hub::Hub;
use crate::protocol::{GroupKey, group_tag};
use anyhow::{Context, Result};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

const SERVICE_TYPE: &str = "_copyblip._tcp.local.";

pub fn start(hub: Arc<Hub>, key: &GroupKey, device: &str, port: u16) -> Result<()> {
    let daemon = ServiceDaemon::new().context("starting mDNS")?;
    let tag = group_tag(key);
    let properties = [("group", tag.as_str()), ("device", device)];
    let host = format!("copyblip-{device}.local.");
    let info = ServiceInfo::new(SERVICE_TYPE, device, &host, "", port, &properties[..])?
        .enable_addr_auto();
    daemon.register(info)?;
    let events = daemon.browse(SERVICE_TYPE)?;
    let device = device.to_owned();

    thread::spawn(move || {
        // Keeps the daemon's handle alive for as long as we browse.
        let _daemon = daemon;
        let mut targets: HashMap<String, Arc<Mutex<Vec<String>>>> = HashMap::new();
        while let Ok(event) = events.recv() {
            match event {
                ServiceEvent::ServiceResolved(service) => {
                    let same_group = service.get_property_val_str("group") == Some(tag.as_str());
                    let peer = service.get_property_val_str("device").map(str::to_owned);
                    let Some(peer) = peer.filter(|peer| same_group && *peer != device) else {
                        continue;
                    };
                    let addresses: Vec<String> = service
                        .get_addresses()
                        .iter()
                        .map(|ip| {
                            std::net::SocketAddr::new(ip.to_ip_addr(), service.get_port())
                                .to_string()
                        })
                        .collect();
                    match targets.get(&peer) {
                        Some(existing) => *existing.lock().unwrap() = addresses,
                        None => {
                            let shared = Arc::new(Mutex::new(addresses));
                            hub.dial(shared.clone());
                            targets.insert(peer, shared);
                        }
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    let peer = fullname.split('.').next().unwrap_or_default();
                    if let Some(existing) = targets.remove(peer) {
                        existing.lock().unwrap().clear();
                    }
                }
                _ => {}
            }
        }
    });
    Ok(())
}
