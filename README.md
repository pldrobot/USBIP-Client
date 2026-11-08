# USBIP-Client (usbip-client)

Cross-platform system-tray USB/IP client for the ESP32-S3 USB/IP server. Right-click the tray
icon to see the servers found on the LAN (checked = attached; click to attach/detach), with
**Refresh**, a divider and **Exit** at the bottom.

- Discovery: scans the local /24 for port 3240, plus extra hosts listed one per line in
  `servers.txt` (`%APPDATA%\usbip-client\` on Windows, `~/.config/usbip-client/` on Linux).
- Windows: calls `usbip.exe` from the bundled usbip-win2 install (BSD-2 licensed).
- Linux: uses the kernel `vhci_hcd` sysfs interface directly; privileged step runs via `pkexec`.

## Build

    cargo build --release

Linux build deps: `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`.

## Packaging

- Windows: `powershell packaging\windows\build.ps1` → `dist\usbip-client-<ver>-x64.msi`
  (WiX 3 is auto-downloaded to `tools\wix3`). One UAC prompt installs the app, the usbip-win2 driver and autostart.
- Linux: `cargo install cargo-deb cargo-generate-rpm`, then `cargo deb` or
  `cargo build --release && cargo generate-rpm`. Packages add XDG autostart and load `vhci-hcd` at boot.

## Status / not yet verified

Windows build and MSI build succeed. Not yet tested: installing the MSI, `usbip.exe` output parsing
(`usbip port`), attach as a non-admin user, and the whole Linux path.
