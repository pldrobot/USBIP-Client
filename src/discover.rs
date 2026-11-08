//! Server discovery: manual list (servers.txt) + /24 scan of the local subnet on port 3240.
use crate::proto::{self, Device};
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Server {
    pub host: String,
    pub devices: Vec<Device>,
}

fn local_ipv4() -> Option<Ipv4Addr> {
    // Connecting a UDP socket sends nothing; it just selects the outbound interface.
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("192.0.2.1:9").ok()?;
    match s.local_addr().ok()?.ip() {
        IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
        _ => None,
    }
}

/// %APPDATA%\usbip-client on Windows, ~/.config/usbip-client on Linux.
pub fn config_dir() -> std::path::PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| ".".into());
    base.join("usbip-client")
}

/// Extra servers (one host/IP per line) in <config_dir>/servers.txt, for other subnets.
fn manual_hosts() -> Vec<String> {
    std::fs::read_to_string(config_dir().join("servers.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

fn probe(host: &str, timeout: Duration) -> Option<Server> {
    let addr = (host, proto::PORT).to_socket_addrs().ok()?.next()?;
    let devices = proto::devlist(addr, timeout).ok()?;
    Some(Server { host: host.to_string(), devices })
}

pub fn scan() -> Vec<Server> {
    let found: Arc<Mutex<Vec<Server>>> = Arc::default();
    let mut hosts: Vec<(String, Duration)> = manual_hosts()
        .into_iter()
        .map(|h| (h, Duration::from_secs(2)))
        .collect();
    if let Some(ip) = local_ipv4() {
        let o = ip.octets();
        for i in 1..255u8 {
            if i != o[3] {
                hosts.push((
                    format!("{}.{}.{}.{}", o[0], o[1], o[2], i),
                    Duration::from_millis(400),
                ));
            }
        }
    }
    let queue = Arc::new(Mutex::new(hosts));
    let workers: Vec<_> = (0..48)
        .map(|_| {
            let (q, f) = (queue.clone(), found.clone());
            std::thread::spawn(move || loop {
                let next = q.lock().unwrap().pop();
                let Some((h, t)) = next else { break };
                if let Some(s) = probe(&h, t) {
                    let mut f = f.lock().unwrap();
                    if !f.iter().any(|x| x.host == s.host) {
                        f.push(s);
                    }
                }
            })
        })
        .collect();
    for w in workers {
        let _ = w.join();
    }
    let mut v = found.lock().unwrap().clone();
    v.sort_by(|a, b| a.host.cmp(&b.host));
    v
}
