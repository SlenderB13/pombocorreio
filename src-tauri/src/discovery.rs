use crate::{
    models::Peer,
    state::{emit_change, CoreState},
};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::thread;
use tauri::AppHandle;

const SERVICE_TYPE: &str = "_pombocorreio._tcp.local.";

pub(crate) fn start_discovery(
    port: u16,
    state: CoreState,
    app: AppHandle,
) -> Result<ServiceDaemon, String> {
    let mdns = ServiceDaemon::new().map_err(|error| error.to_string())?;
    let (id, name) = {
        let inner = state.0.lock().expect("state lock");
        (
            inner.settings.device_id.clone(),
            inner.settings.device_name.clone(),
        )
    };

    let properties = [("id", id.as_str()), ("name", name.as_str())];
    let hostname = format!("{id}.local.");
    let service = ServiceInfo::new(SERVICE_TYPE, &id, &hostname, (), port, &properties[..])
        .map_err(|error| error.to_string())?
        .enable_addr_auto();
    mdns.register(service).map_err(|error| error.to_string())?;

    let receiver = mdns
        .browse(SERVICE_TYPE)
        .map_err(|error| error.to_string())?;
    thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let Some(peer_id) = info.get_property_val_str("id") else {
                        continue;
                    };
                    if peer_id == id {
                        continue;
                    }

                    let name = info
                        .get_property_val_str("name")
                        .unwrap_or("Nearby device")
                        .to_string();
                    let address = info
                        .get_addresses()
                        .iter()
                        .find(|ip| ip.is_ipv4())
                        .or_else(|| info.get_addresses().iter().next())
                        .map(|ip| ip.to_ip_addr().to_string());

                    if let Some(address) = address {
                        let mut inner = state.0.lock().expect("state lock");
                        let trusted = inner.settings.trusted_devices.contains(peer_id);
                        inner.peers.insert(
                            peer_id.to_string(),
                            Peer {
                                id: peer_id.to_string(),
                                name,
                                address,
                                port: info.get_port(),
                                trusted,
                            },
                        );
                        drop(inner);
                        emit_change(&app);
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    let mut inner = state.0.lock().expect("state lock");
                    inner.peers.retain(|_, peer| !fullname.contains(&peer.id));
                    drop(inner);
                    emit_change(&app);
                }
                _ => {}
            }
        }
    });

    Ok(mdns)
}
