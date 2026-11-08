//! Procedural logo on a transparent background:
//!   * USB trident, laid horizontal, in USBip yellow (#ffde59), arrow pointing left
//!   * from its big round end: two flat-blue network-signal arcs
//!   * dark outline so it stays readable on light and dark taskbars
//! `render(size, true)` turns the signal arcs green (tray icon while a device is attached).
//! `render_tray` is a zoomed-in, bolder variant that stays legible at 16-32 px.
//! Shared by the tray icon (runtime) and examples/gen_icons.rs (ICO/PNG for the installer).

type P = (f32, f32);
type Rgb = (f32, f32, f32);

const YELLOW: Rgb = (255.0, 222.0, 89.0);
const OUTLINE: Rgb = (30.0, 27.0, 46.0);
const BLUE: Rgb = (37.0, 99.0, 235.0); // #2563eb, flat
const GREEN: Rgb = (34.0, 197.0, 94.0); // bars when connected
const ARCS: [f32; 2] = [0.14, 0.26];

#[derive(Clone, Copy)]
struct Style {
    stroke: f32,  // half stroke width of the trident lines (upright frame)
    grow: f32,    // extra radius for circles / arrow head / square
    arc_hw: f32,  // half width of the signal arcs
    outline: f32, // outline thickness
    zoom: f32,    // >1 scales the artwork up about its centre
}

const NORMAL: Style = Style { stroke: 0.03, grow: 0.0, arc_hw: 0.03, outline: 0.02, zoom: 1.0 };
const TRAY: Style = Style { stroke: 0.055, grow: 0.03, arc_hw: 0.055, outline: 0.035, zoom: 1.12 };

fn len(a: P, b: P) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn seg_dist(p: P, a: P, b: P) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    len(p, (a.0 + t * dx, a.1 + t * dy))
}

fn in_tri(p: P, a: P, b: P, c: P) -> bool {
    let s = |p1: P, p2: P, p3: P| (p1.0 - p3.0) * (p2.1 - p3.1) - (p2.0 - p3.0) * (p1.1 - p3.1);
    let (d1, d2, d3) = (s(p, a, b), s(p, b, c), s(p, c, a));
    !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
}

/// USB trident in its own upright frame (stem at x=0.38, arrow up, circle at the bottom).
fn trident(q: P, s: Style) -> bool {
    let (w, g) = (s.stroke, s.grow);
    let sx = 0.38;
    seg_dist(q, (sx, 0.26), (sx, 0.80)) <= w
        || len(q, (sx, 0.80)) <= 0.095 + g
        || in_tri(q, (sx, 0.10 - g), (sx - 0.09 - g, 0.28), (sx + 0.09 + g, 0.28))
        // left branch ending in a circle
        || seg_dist(q, (sx, 0.64), (0.20, 0.50)) <= w
        || seg_dist(q, (0.20, 0.50), (0.20, 0.40)) <= w
        || len(q, (0.20, 0.35)) <= 0.07 + g
        // right branch ending in a square
        || seg_dist(q, (sx, 0.52), (0.56, 0.40)) <= w
        || seg_dist(q, (0.56, 0.40), (0.56, 0.36)) <= w
        || ((q.0 - 0.56).abs() <= 0.065 + g && (q.1 - 0.30).abs() <= 0.065 + g)
}

/// Rotate the trident to horizontal: arrow tip pointing LEFT, big round end on the right.
const K: f32 = 0.72; // scale
const CX0: f32 = 0.33; // x of the trident's vertical centre line
fn usb(p: P, s: Style) -> bool {
    let q = (0.38 + (p.1 - 0.5) / K, 0.5 + (p.0 - CX0) / K);
    trident(q, s)
}

/// Centre of the big round end (orig (0.38, 0.80)): the signal starts here.
const ORIGIN: P = (CX0 + K * 0.30, 0.5);

/// Signal arcs radiating right from the big round end. Returns the colour if hit.
fn signal(p: P, connected: bool, s: Style) -> Option<Rgb> {
    let (dx, dy) = (p.0 - ORIGIN.0, p.1 - ORIGIN.1);
    if dx > 0.0 && dy.abs() <= dx * 1.2 {
        let d = len(p, ORIGIN);
        for r in ARCS {
            if (d - r).abs() <= s.arc_hw {
                return Some(if connected { GREEN } else { BLUE });
            }
        }
    }
    None
}

/// Colour of the logo at p, or None when transparent.
fn shade(p: P, connected: bool, s: Style) -> Option<Rgb> {
    // zoom about the artwork's visual centre
    let p = (0.45 + (p.0 - 0.5) / s.zoom, 0.5 + (p.1 - 0.5) / s.zoom);
    if usb(p, s) {
        return Some(YELLOW);
    }
    if let Some(c) = signal(p, connected, s) {
        return Some(c);
    }
    let r = s.outline;
    let d = r * 0.71;
    for (ox, oy) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r), (d, d), (-d, d), (d, -d), (-d, -d)] {
        let n = (p.0 + ox, p.1 + oy);
        if usb(n, s) || signal(n, connected, s).is_some() {
            return Some(OUTLINE);
        }
    }
    None
}

fn render_with(size: u32, connected: bool, s: Style) -> Vec<u8> {
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let (mut r, mut g, mut b, mut hit) = (0.0f32, 0.0f32, 0.0f32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = (
                        (x as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32,
                        (y as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32,
                    );
                    if let Some(c) = shade(p, connected, s) {
                        r += c.0;
                        g += c.1;
                        b += c.2;
                        hit += 1;
                    }
                }
            }
            if hit == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let n = hit as f32;
                out.extend_from_slice(&[(r / n) as u8, (g / n) as u8, (b / n) as u8, (hit * 255 / (SS * SS)) as u8]);
            }
        }
    }
    out
}

/// Full logo, straight-alpha RGBA, `size`×`size` (installer / shortcuts / exe icon).
pub fn render(size: u32, connected: bool) -> Vec<u8> {
    render_with(size, connected, NORMAL)
}

/// Bolder, zoomed variant for the system tray.
#[allow(dead_code)]
pub fn render_tray(size: u32, connected: bool) -> Vec<u8> {
    render_with(size, connected, TRAY)
}
