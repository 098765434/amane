/*
 * Wireless debugging announces itself over mDNS: one service while it
 * offers pairing (a code or QR code is showing on the phone), one while it
 * takes connections; their ports change each time, so they're looked for
 * rather than remembered
 */

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::{Mutex, PoisonError, mpsc};
use std::thread;

use mdns_sd::{ServiceDaemon, ServiceEvent};

const PAIRING: &str = "_adb-tls-pairing._tcp.local.";
const CONNECT: &str = "_adb-tls-connect._tcp.local.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WirelessKind {
    Pairing,
    Connect,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WirelessService {
    pub kind: WirelessKind,

    // the instance's name; a QR code's pairing service carries the name the code gave
    pub name: String,
    pub address: IpAddr,
    pub port: u16,
}

static DAEMON: Mutex<Option<ServiceDaemon>> = Mutex::new(None);

pub fn start() {
    let mut daemon = DAEMON.lock().unwrap_or_else(PoisonError::into_inner);

    if daemon.is_some() {
        return;
    }

    let Ok(started) = ServiceDaemon::new() else {
        return;
    };

    let (Ok(pairing), Ok(connect)) = (started.browse(PAIRING), started.browse(CONNECT)) else {
        let _ = started.shutdown();

        return;
    };

    *daemon = Some(started);

    drop(daemon);

    // both kinds come in on one channel, so one loop keeps the list
    let (sender, events) = mpsc::channel();

    for browse in [pairing, connect] {
        let sender = sender.clone();

        thread::spawn(move || {
            for event in browse {
                if sender.send(event).is_err() {
                    break;
                }
            }
        });
    }

    drop(sender);

    thread::spawn(move || {
        let mut services = BTreeMap::new();

        // ends once both browses end, when the daemon shuts down
        for event in events {
            if apply(&mut services, event) {
                super::found(services.values().cloned().collect());
            }
        }

        super::found(Vec::new());
    });
}

pub fn stop() {
    if let Some(daemon) = DAEMON.lock().unwrap_or_else(PoisonError::into_inner).take() {
        let _ = daemon.shutdown();
    }
}

// whether the list changed
fn apply(services: &mut BTreeMap<String, WirelessService>, event: ServiceEvent) -> bool {
    match event {
        ServiceEvent::ServiceResolved(resolved) => {
            let kind = if resolved.ty_domain == PAIRING {
                WirelessKind::Pairing
            } else {
                WirelessKind::Connect
            };

            // a phone answers on IPv4 and IPv6 alike; the IPv4 address is the one KDE Connect knows
            let mut addresses: Vec<IpAddr> = resolved
                .addresses
                .iter()
                .map(|ip| ip.to_ip_addr())
                .collect();
            addresses.sort_by_key(IpAddr::is_ipv6);

            let Some(address) = addresses.first().copied() else {
                return false;
            };

            let service = WirelessService {
                kind,
                name: instance(&resolved.fullname, &resolved.ty_domain),
                address,
                port: resolved.port,
            };

            services.insert(resolved.fullname.clone(), service.clone()) != Some(service)
        }

        ServiceEvent::ServiceRemoved(_, fullname) => services.remove(&fullname).is_some(),

        _ => false,
    }
}

// "adb-R5CX-abc._adb-tls-connect._tcp.local." → "adb-R5CX-abc"
fn instance(fullname: &str, ty_domain: &str) -> String {
    fullname
        .strip_suffix(ty_domain)
        .unwrap_or(fullname)
        .trim_end_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_instance_without_its_type() {
        assert_eq!(
            instance("studio-amane42._adb-tls-pairing._tcp.local.", PAIRING),
            "studio-amane42"
        );
    }
}
