#![cfg_attr(windows, windows_subsystem = "windows")]

mod attach;
mod discover;
mod logo;
mod proto;

use discover::Server;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

enum Ev {
    Scanned(Vec<Server>),
    Attached(Vec<attach::Key>, HashMap<attach::Key, String>),
    Error(String),
    Menu(MenuEvent),
    /// An attach/detach thread for this device has finished.
    Done(attach::Key),
}

/// Tray icon: the logo, with a small green dot while something is attached.
fn icon(active: bool) -> Icon {
    Icon::from_rgba(logo::render_tray(64, active), 64, 64).expect("icon")
}

/// Name for an attached device: what the OS reports (vendor and product), else what the last scan
/// knew about it, else the bus id.
fn device_label(servers: &[Server], names: &HashMap<attach::Key, String>, key: &attach::Key) -> String {
    names
        .get(key)
        .cloned()
        .or_else(|| {
            servers
                .iter()
                .filter(|s| s.host == key.0)
                .flat_map(|s| s.devices.iter())
                .find(|d| d.busid == key.1)
                .map(|d| d.label())
        })
        .unwrap_or_else(|| key.1.clone())
}

fn build_menu(
    servers: &[Server],
    attached: &[attach::Key],
    names: &HashMap<attach::Key, String>,
    error: &Option<String>,
) -> Menu {
    let menu = Menu::new();
    // Connected devices first: one checked entry each, click to detach. An attached server does not
    // answer scans with its device (it is in use), so these entries do not depend on the scan.
    for key in attached {
        let (host, busid) = key;
        let _ = menu.append(&CheckMenuItem::with_id(
            format!("dev|{host}|{busid}"),
            format!("\u{25CF} {host} \u{2014} {}", device_label(servers, names, key)),
            true,
            true,
            None,
        ));
    }
    if !attached.is_empty() {
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    let mut any = !attached.is_empty();
    // Then the other servers and devices found on the LAN, without repeating attached ones.
    for s in servers {
        let host_attached = attached.iter().any(|(h, _)| *h == s.host);
        if s.devices.is_empty() {
            if !host_attached {
                let _ = menu.append(&MenuItem::with_id(format!("none|{}", s.host), format!("{} (no device)", s.host), false, None));
                any = true;
            }
            continue;
        }
        for d in &s.devices {
            if attached.iter().any(|(h, b)| *h == s.host && *b == d.busid) {
                continue;
            }
            let id = format!("dev|{}|{}", s.host, d.busid);
            let _ = menu.append(&CheckMenuItem::with_id(id, format!("{} \u{2014} {}", s.host, d.label()), true, false, None));
            any = true;
        }
    }
    if !any {
        let _ = menu.append(&MenuItem::with_id("empty", "No servers found", false, None));
    }
    if let Some(e) = error {
        let short: String = e.chars().take(80).collect();
        let _ = menu.append(&MenuItem::with_id("err", format!("\u{26A0} {short}"), false, None));
    }
    let _ = menu.append(&MenuItem::with_id("refresh", "Refresh", true, None));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id("exit", "Exit", true, None));
    menu
}

fn spawn_poller(proxy: EventLoopProxy<Ev>, rescan: std::sync::mpsc::Receiver<()>) {
    std::thread::spawn(move || {
        let mut ticks = 0u32;
        loop {
            let manual = rescan.recv_timeout(Duration::from_secs(10)).is_ok();
            if manual || ticks % 3 == 0 {
                // Do not probe servers we are attached to: it only disturbs the session.
                let skip: HashSet<String> = attach::snapshot().0.into_iter().map(|(h, _)| h).collect();
                let _ = proxy.send_event(Ev::Scanned(discover::scan(&skip)));
            }
            let (a, n) = attach::snapshot();
            let _ = proxy.send_event(Ev::Attached(a, n));
            ticks += 1;
        }
    });
}

fn main() {
    attach::run_helper_if_requested();

    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    {
        let p = proxy.clone();
        MenuEvent::set_event_handler(Some(move |e| {
            let _ = p.send_event(Ev::Menu(e));
        }));
    }
    let (rescan_tx, rescan_rx) = std::sync::mpsc::channel::<()>();
    spawn_poller(proxy.clone(), rescan_rx);

    let mut tray: Option<TrayIcon> = None;
    let mut servers: Vec<Server> = vec![];
    let mut attached: Vec<attach::Key> = vec![];
    let mut names: HashMap<attach::Key, String> = HashMap::new();
    let mut error: Option<String> = None;
    // Devices with an attach/detach in progress: a second click must not start another one.
    let mut busy: HashSet<attach::Key> = HashSet::new();

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let mut refresh_ui = false;
        match event {
            Event::NewEvents(StartCause::Init) => {
                tray = TrayIconBuilder::new()
                    .with_tooltip("USB/IP Client")
                    .with_icon(icon(false))
                    .with_menu(Box::new(build_menu(&servers, &attached, &names, &error)))
                    .build()
                    .map_err(|e| eprintln!("tray: {e}"))
                    .ok();
            }
            Event::UserEvent(Ev::Scanned(mut s)) => {
                // Keep last-known entries (with device labels) for servers we're attached to.
                for old in &servers {
                    if attached.iter().any(|(h, _)| *h == old.host) && !s.iter().any(|x| x.host == old.host) {
                        s.push(old.clone());
                    }
                }
                s.sort_by(|a, b| a.host.cmp(&b.host));
                servers = s;
                refresh_ui = true;
            }
            Event::UserEvent(Ev::Attached(a, n)) => {
                if a != attached || n != names {
                    attached = a;
                    names = n;
                    refresh_ui = true;
                }
            }
            Event::UserEvent(Ev::Error(e)) => {
                error = Some(e);
                refresh_ui = true;
            }
            Event::UserEvent(Ev::Done(k)) => {
                busy.remove(&k);
            }
            Event::UserEvent(Ev::Menu(m)) => {
                let id = m.id.0.as_str();
                if id == "exit" {
                    tray.take();
                    *flow = ControlFlow::Exit;
                } else if id == "refresh" {
                    let _ = rescan_tx.send(());
                } else if let Some(rest) = id.strip_prefix("dev|") {
                    if let Some((host, busid)) = rest.split_once('|') {
                        let key: attach::Key = (host.to_string(), busid.to_string());
                        if busy.contains(&key) {
                            refresh_ui = true; // already working on it: ignore, and undo the checkbox flip
                        } else {
                            busy.insert(key.clone());
                            let on = attached.contains(&key);
                            error = None;
                            let p = proxy.clone();
                            std::thread::spawn(move || {
                                let r = if on { attach::detach(&key.0, &key.1) } else { attach::attach(&key.0, &key.1) };
                                if let Err(e) = r {
                                    let _ = p.send_event(Ev::Error(e));
                                }
                                let (a, n) = attach::snapshot();
                                let _ = p.send_event(Ev::Attached(a, n));
                                let _ = p.send_event(Ev::Done(key));
                            });
                        }
                    }
                }
            }
            _ => {}
        }
        if refresh_ui {
            if let Some(t) = &tray {
                t.set_menu(Some(Box::new(build_menu(&servers, &attached, &names, &error))));
                t.set_icon(Some(icon(!attached.is_empty()))).ok();
                let tip = match attached.as_slice() {
                    [] => "USBIP Client".to_string(),
                    [one] => format!("USBIP Client - attached: {}", device_label(&servers, &names, one)),
                    many => format!("USBIP Client - {} devices attached", many.len()),
                };
                t.set_tooltip(Some(tip)).ok();
            }
        }
    });
}
