//! OS-specific attach/detach.
//!  * Windows: drives usbip.exe from the bundled usbip-win2 install (the MSI puts it in
//!    <app dir>\usbip-win2, i.e. C:\Program Files\USBIP-Client\usbip-win2).
//!  * Linux: talks to the kernel vhci_hcd sysfs interface directly (no usbip binary needed);
//!    when not root, the privileged step is re-run through `pkexec usbip-client --attach ...`.

use std::collections::HashMap;

pub type Key = (String, String); // (host, busid)

/// Attached devices, plus a display name for each where the OS reports one
/// (Windows: the product line of `usbip port`).
pub type Snapshot = (Vec<Key>, HashMap<Key, String>);

pub use imp::{attach, detach, run_helper_if_requested, snapshot};

#[cfg(windows)]
mod imp {
    use super::Key;
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn usbip_exe() -> PathBuf {
        // Installed layout: <app dir>\usbip-win2\usbip.exe
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            let p = dir.join("usbip-win2").join("usbip.exe");
            if p.exists() {
                return p;
            }
        }
        PathBuf::from("usbip.exe") // fall back to PATH
    }

    fn run(args: &[&str]) -> Result<String, String> {
        let out = Command::new(usbip_exe())
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("cannot run usbip.exe: {e}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if out.status.success() {
            Ok(text)
        } else {
            Err(text.trim().to_string())
        }
    }

    pub fn attach(host: &str, busid: &str) -> Result<(), String> {
        run(&["attach", "-r", host, "-b", busid]).map(|_| ())
    }

    /// Parse `usbip port` (usbip-win2). Each device is a block:
    ///   Port 01: device in use at Full Speed(12Mbps)
    ///            QinHeng Electronics : CH340 serial converter (1a86:7523)
    ///              -> usbip://192.168.2.237:3240/1-1
    /// The first plain line after "Port NN" is the product name.
    fn parse_ports(text: &str) -> Vec<(u32, Key, Option<String>)> {
        let mut out = vec![];
        let mut cur: Option<u32> = None;
        let mut desc: Option<String> = None;
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("Port ") {
                cur = rest.split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse().ok());
                desc = None;
            } else if cur.is_some() && desc.is_none() && !t.is_empty() && !t.starts_with("->") && !t.contains("usbip://") {
                desc = Some(t.to_string());
            } else if let (Some(p), Some(i)) = (cur, t.find("usbip://")) {
                let url = &t[i + 8..];
                if let Some((hostport, busid)) = url.split_once('/') {
                    let host = hostport.split(':').next().unwrap_or(hostport);
                    out.push((p, (host.to_string(), busid.trim().to_string()), desc.take()));
                    cur = None;
                }
            }
        }
        out
    }

    fn ports() -> Vec<(u32, Key, Option<String>)> {
        let Ok(text) = run(&["port"]) else { return vec![] };
        parse_ports(&text)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // Real output of usbip-win2 0.9.8.1 with the ESP32-S3 bridge attached.
        const REAL: &str = "Imported USB devices\r\n====================\r\n\
Port 01: device in use at Full Speed(12Mbps)\r\n\
         QinHeng Electronics : CH340 serial converter (1a86:7523)\r\n\
           -> usbip://192.168.2.237:3240/1-1\r\n\
           -> remote bus/dev: 001/001\r\n\
           -> serial: \r\n\
           -> mode: zero-copy\r\n";

        #[test]
        fn parses_key_and_product_name() {
            let v = parse_ports(REAL);
            assert_eq!(v.len(), 1);
            assert_eq!(v[0].0, 1);
            assert_eq!(v[0].1, ("192.168.2.237".to_string(), "1-1".to_string()));
            assert_eq!(v[0].2.as_deref(), Some("QinHeng Electronics : CH340 serial converter (1a86:7523)"));
        }

        #[test]
        fn nothing_attached_is_empty() {
            assert!(parse_ports("").is_empty());
            assert!(parse_ports("Imported USB devices\n====================\n").is_empty());
        }

        #[test]
        fn two_devices() {
            let two = format!("{REAL}Port 02: device in use at Low Speed(1.5Mbps)\r\n   Logitech (046d:c077)\r\n     -> usbip://10.0.0.5:3240/2-1\r\n");
            let v = parse_ports(&two);
            assert_eq!(v.len(), 2);
            assert_eq!(v[1].1, ("10.0.0.5".to_string(), "2-1".to_string()));
            assert_eq!(v[1].2.as_deref(), Some("Logitech (046d:c077)"));
        }
    }

    pub fn detach(host: &str, busid: &str) -> Result<(), String> {
        let port = ports()
            .into_iter()
            .find(|(_, k, _)| k.0 == host && k.1 == busid)
            .map(|(p, _, _)| p)
            .ok_or("device is not attached")?;
        run(&["detach", "-p", &port.to_string()]).map(|_| ())
    }

    pub fn snapshot() -> super::Snapshot {
        let p = ports();
        let keys = p.iter().map(|(_, k, _)| k.clone()).collect();
        let names = p.into_iter().filter_map(|(_, k, d)| d.map(|d| (k, d))).collect();
        (keys, names)
    }

    pub fn run_helper_if_requested() {}
}

#[cfg(target_os = "linux")]
mod imp {
    use crate::proto;
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    use std::process::Command;

    const VHCI: &str = "/sys/devices/platform/vhci_hcd.0";
    type Row = (String, u32, u32, u32); // (hub, port, status, devid)

    /// Rows from the vhci status file: "hub port sta spd dev sockfd local_busid".
    fn status_rows() -> Vec<Row> {
        let text = std::fs::read_to_string(format!("{VHCI}/status")).unwrap_or_default();
        text.lines()
            .skip(1)
            .filter_map(|l| {
                let f: Vec<&str> = l.split_whitespace().collect();
                let dev = u32::from_str_radix(f.get(4)?, 16).ok()?;
                Some((f.first()?.to_string(), f.get(1)?.parse().ok()?, f.get(2)?.parse().ok()?, dev))
            })
            .collect()
    }

    /// Attachments made by this app live in a user file ("port devid host busid" per line), so they
    /// survive an app restart. Entries whose vhci port is no longer in use for that device are dropped.
    fn state_path() -> std::path::PathBuf {
        crate::discover::config_dir().join("attached.txt")
    }

    fn load() -> Vec<(u32, u32, String, String)> {
        let used: Vec<(u32, u32)> = status_rows().into_iter().filter(|r| r.2 == 6).map(|r| (r.1, r.3)).collect();
        std::fs::read_to_string(state_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut f = l.split_whitespace();
                let (p, d) = (f.next()?.parse().ok()?, u32::from_str_radix(f.next()?, 16).ok()?);
                Some((p, d, f.next()?.to_string(), f.next()?.to_string()))
            })
            .filter(|(p, d, _, _)| used.contains(&(*p, *d)))
            .collect()
    }

    fn save(v: &[(u32, u32, String, String)]) {
        let text: String = v.iter().map(|(p, d, h, b)| format!("{p} {d:x} {h} {b}\n")).collect();
        let path = state_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, text);
    }

    /// Runs as root (directly or via pkexec). Returns the vhci port.
    fn do_attach(host: &str, busid: &str) -> Result<(u32, u32), String> {
        let _ = Command::new("modprobe").arg("vhci_hcd").status();
        let addr = discover_addr(host)?;
        let (stream, dev) = proto::import(addr, busid).map_err(|e| e.to_string())?;
        let want_hub = if dev.speed >= 5 { "ss" } else { "hs" };
        // sta 4 == VDEV_ST_NULL (free)
        let port = status_rows()
            .into_iter()
            .find(|(hub, _, sta, _)| hub == want_hub && *sta == 4)
            .map(|(_, p, _, _)| p)
            .ok_or("no free vhci port (is vhci_hcd loaded?)")?;
        let line = format!("{} {} {} {}", port, stream.as_raw_fd(), dev.devid(), dev.speed);
        std::fs::OpenOptions::new()
            .write(true)
            .open(format!("{VHCI}/attach"))
            .and_then(|mut f| f.write_all(line.as_bytes()))
            .map_err(|e| format!("vhci attach failed: {e}"))?;
        Ok((port, dev.devid())) // dropping `stream` is fine: the kernel holds its own reference
    }

    fn do_detach(port: u32) -> Result<(), String> {
        std::fs::OpenOptions::new()
            .write(true)
            .open(format!("{VHCI}/detach"))
            .and_then(|mut f| f.write_all(port.to_string().as_bytes()))
            .map_err(|e| format!("vhci detach failed: {e}"))
    }

    fn discover_addr(host: &str) -> Result<std::net::SocketAddr, String> {
        use std::net::ToSocketAddrs;
        (host, proto::PORT)
            .to_socket_addrs()
            .ok()
            .and_then(|mut a| a.next())
            .ok_or_else(|| format!("cannot resolve {host}"))
    }

    fn is_root() -> bool {
        unsafe { libc::geteuid() == 0 }
    }

    fn elevated(args: &[&str]) -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut cmd = if is_root() {
            Command::new(exe)
        } else {
            let mut c = Command::new("pkexec");
            c.arg(exe);
            c
        };
        let out = cmd.args(args).output().map_err(|e| format!("cannot run helper: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            Err(if err.is_empty() { "authorization cancelled or failed".into() } else { err })
        }
    }

    pub fn attach(host: &str, busid: &str) -> Result<(), String> {
        let out = elevated(&["--attach", host, busid])?;
        let (port, devid) = out
            .lines()
            .find_map(|l| {
                let mut f = l.strip_prefix("PORT ")?.split_whitespace();
                Some((f.next()?.parse().ok()?, u32::from_str_radix(f.next()?, 16).ok()?))
            })
            .ok_or("helper did not report a port")?;
        let mut v = load();
        v.push((port, devid, host.into(), busid.into()));
        save(&v);
        Ok(())
    }

    pub fn detach(host: &str, busid: &str) -> Result<(), String> {
        let v = load();
        let port = v
            .iter()
            .find(|(_, _, h, b)| h == host && b == busid)
            .map(|e| e.0)
            .ok_or("device was not attached by this app")?;
        elevated(&["--detach", &port.to_string()])?;
        save(&v.into_iter().filter(|e| e.0 != port).collect::<Vec<_>>());
        Ok(())
    }

    pub fn snapshot() -> super::Snapshot {
        // vhci does not report a product name; the menu falls back to the last scan or the busid.
        (load().into_iter().map(|(_, _, h, b)| (h, b)).collect(), Default::default())
    }

    /// `usbip-client --attach HOST BUSID` / `--detach PORT`: privileged helper entry point.
    pub fn run_helper_if_requested() {
        let a: Vec<String> = std::env::args().collect();
        match a.get(1).map(String::as_str) {
            Some("--attach") if a.len() == 4 => match do_attach(&a[2], &a[3]) {
                Ok((p, d)) => {
                    println!("PORT {p} {d:x}");
                    std::process::exit(0)
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1)
                }
            },
            Some("--detach") if a.len() == 3 => match a[2].parse().map_err(|_| "bad port".to_string()).and_then(do_detach) {
                Ok(()) => std::process::exit(0),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1)
                }
            },
            _ => {}
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn attach(_: &str, _: &str) -> Result<(), String> { Err("unsupported OS".into()) }
    pub fn detach(_: &str, _: &str) -> Result<(), String> { Err("unsupported OS".into()) }
    pub fn snapshot() -> super::Snapshot { (vec![], Default::default()) }
    pub fn run_helper_if_requested() {}
}
