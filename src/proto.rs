//! Minimal USB/IP wire protocol (client side): device list + import.
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub const PORT: u16 = 3240;
const VERSION: u16 = 0x0111;
const OP_REQ_DEVLIST: u16 = 0x8005;
#[cfg(target_os = "linux")]
const OP_REQ_IMPORT: u16 = 0x8003;
const DEV_LEN: usize = 312;

#[derive(Clone, Debug)]
pub struct Device {
    pub busid: String,
    #[allow(dead_code)]
    pub busnum: u32,
    #[allow(dead_code)]
    pub devnum: u32,
    #[allow(dead_code)]
    pub speed: u32,
    pub vid: u16,
    pub pid: u16,
}

impl Device {
    #[cfg(target_os = "linux")]
    pub fn devid(&self) -> u32 {
        (self.busnum << 16) | self.devnum
    }
    pub fn label(&self) -> String {
        let vendor = match self.vid {
            0x04A9 => "Canon",
            0x03F0 => "HP",
            0x04B8 => "Epson",
            0x04F9 => "Brother",
            _ => "USB",
        };
        format!("{} {:04x}:{:04x}", vendor, self.vid, self.pid)
    }
}

fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}
fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn parse_device(b: &[u8]) -> Device {
    let raw = &b[256..288];
    let end = raw.iter().position(|&c| c == 0).unwrap_or(32);
    Device {
        busid: String::from_utf8_lossy(&raw[..end]).into_owned(),
        busnum: be32(&b[288..]),
        devnum: be32(&b[292..]),
        speed: be32(&b[296..]),
        vid: be16(&b[300..]),
        pid: be16(&b[302..]),
    }
}

pub fn connect(addr: SocketAddr, timeout: Duration) -> std::io::Result<TcpStream> {
    let s = TcpStream::connect_timeout(&addr, timeout)?;
    s.set_read_timeout(Some(Duration::from_secs(5)))?;
    s.set_write_timeout(Some(Duration::from_secs(5)))?;
    s.set_nodelay(true).ok();
    Ok(s)
}

fn header(code: u16) -> [u8; 8] {
    let mut h = [0u8; 8];
    h[0..2].copy_from_slice(&VERSION.to_be_bytes());
    h[2..4].copy_from_slice(&code.to_be_bytes());
    h
}

/// Ask a server which devices it exports. Doubles as the "is this a USB/IP server" probe.
pub fn devlist(addr: SocketAddr, timeout: Duration) -> std::io::Result<Vec<Device>> {
    let mut s = connect(addr, timeout)?;
    s.write_all(&header(OP_REQ_DEVLIST))?;
    let mut rep = [0u8; 12];
    s.read_exact(&mut rep)?;
    if be16(&rep[2..]) != 0x0005 || be32(&rep[4..]) != 0 {
        return Err(std::io::Error::other("bad devlist reply"));
    }
    let count = be32(&rep[8..]).min(16);
    let mut out = Vec::new();
    for _ in 0..count {
        let mut d = [0u8; DEV_LEN];
        s.read_exact(&mut d)?;
        let mut skip = vec![0u8; d[311] as usize * 4];
        s.read_exact(&mut skip)?;
        out.push(parse_device(&d));
    }
    Ok(out)
}

/// Import a device. Returns the still-open socket (handed to vhci_hcd) and the device info.
#[cfg(target_os = "linux")]
pub fn import(addr: SocketAddr, busid: &str) -> std::io::Result<(TcpStream, Device)> {
    let mut s = connect(addr, Duration::from_secs(5))?;
    let mut req = [0u8; 40];
    req[..8].copy_from_slice(&header(OP_REQ_IMPORT));
    let id = busid.as_bytes();
    let n = id.len().min(31);
    req[8..8 + n].copy_from_slice(&id[..n]);
    s.write_all(&req)?;
    let mut rep = [0u8; 8];
    s.read_exact(&mut rep)?;
    if be32(&rep[4..]) != 0 {
        return Err(std::io::Error::other(
            "server refused import (device busy or not present)",
        ));
    }
    let mut d = [0u8; DEV_LEN];
    s.read_exact(&mut d)?;
    s.set_read_timeout(None)?;
    s.set_write_timeout(None)?;
    Ok((s, parse_device(&d)))
}
