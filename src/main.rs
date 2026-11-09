#![cfg_attr(windows, windows_subsystem = "windows")]

mod attach;
mod discover;
mod logo;
mod proto;

use discover::Server;
use std::time::Duration;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

enum Ev {
    Scanned(Vec<Server>),
    Attached(Vec<attach::Key>),
    Error(String),
    Menu(MenuEvent),
}

/// Tray icon: the logo, with a small green dot while something is attached.
fn icon(active: bool) -> Icon {
    Icon::from_rgba(logo::render_tray(64, active), 64, 64).expect("icon")
}

fn build_menu(servers: &[Server], attached: &[attach::Key], error: &Option<String>) -> Menu {
    let menu = Menu::new();
    if !attached.is_empty() {
        for (host, busid) in attached {
            let dev = servers
                .iter()
                .filter(|s| s.host == *host)
                .flat_map(|s| s.devices.iter())
                .find(|d| d.busid == *busid)
                .map(|d| d.label())
                .unwrap_or_else(|| busid.to_string());
            let _ = menu.append(&MenuItem::with_id(
                format!("conn|{host}|{busid}"),
                format!("● Connected: {host} — {dev}"),
                false,
                None,
            ));
        }
        let _ = menu.append(&PredefinedMenuItem::separator());
    }
    let mut any = false;
    for s in servers {
        if s.devices.is_empty() {
            let _ = menu.append(&MenuItem::with_id(format!("none|{}", s.host), format!("{} (no device)", s.host), false, None));
            any = true;
        }
        for d in &s.devices {
            let on = attached.iter().any(|(h, b)| *h == s.host && *b == d.busid);
            let id = format!("dev|{}|{}", s.host, d.busid);
            let _ = menu.append(&CheckMenuItem::with_id(id, format!("{} — {}", s.host, d.label()), true, on, None));
            any = true;
        }
    }
    // A server we're attached to usually stops answering scans (it's busy serving us), so it
    // may be missing from `servers`. Keep it in the menu so it can still be detached.
    for (host, busid) in attached {
        let listed = servers.iter().any(|s| s.host == *host && s.devices.iter().any(|d| d.busid == *busid));
        if !listed {
            let id = format!("dev|{host}|{busid}");
            let _ = menu.append(&CheckMenuItem::with_id(id, format!("{host} — {busid}"), true, true, None));
            any = true;
        }
    }
    if !any {
        let _ = menu.append(&MenuItem::with_id("empty", "No servers found", false, None));
    }
    if let Some(e) = error {
        let short: String = e.chars().take(80).collect();
        let _ = menu.append(&MenuItem::with_id("err", format!("⚠ {short}"), false, None));
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
                let _ = proxy.send_event(Ev::Scanned(discover::scan()));
            }
            let _ = proxy.send_event(Ev::Attached(attach::attached()));
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
    let mut error: Option<String> = None;

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let mut refresh_ui = false;
        match event {
            Event::NewEvents(StartCause::Init) => {
                tray = TrayIconBuilder::new()
                    .with_tooltip("USB/IP Client")
                    .with_icon(icon(false))
                    .with_menu(Box::new(build_menu(&servers, &attached, &error)))
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
            Event::UserEvent(Ev::Attached(a)) => {
                if a != attached {
                    attached = a;
                    refresh_ui = true;
                }
            }
            Event::UserEvent(Ev::Error(e)) => {
                error = Some(e);
                refresh_ui = true;
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
                        let (host, busid) = (host.to_string(), busid.to_string());
                        let on = attached.iter().any(|(h, b)| *h == host && *b == busid);
                        error = None;
                        let p = proxy.clone();
                        std::thread::spawn(move || {
                            let r = if on { attach::detach(&host, &busid) } else { attach::attach(&host, &busid) };
                            if let Err(e) = r {
                                let _ = p.send_event(Ev::Error(e));
                            }
                            let _ = p.send_event(Ev::Attached(attach::attached()));
                        });
                    }
                }
            }
            _ => {}
        }
        if refresh_ui {
            if let Some(t) = &tray {
                t.set_menu(Some(Box::new(build_menu(&servers, &attached, &error))));
                t.set_icon(Some(icon(!attached.is_empty()))).ok();
                let tip = if attached.is_empty() {
                    "USBIP Client".to_string()
                } else {
                    format!("USBIP Client - {} device(s) attached", attached.len())
                };
                t.set_tooltip(Some(tip)).ok();
            }
        }
    });
}
