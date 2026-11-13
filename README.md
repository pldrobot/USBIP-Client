# USBIP-Client (usbip-client)

Cross-platform system-tray USB/IP client for the ESP32-S3 USB/IP server. Right-click the tray
icon to see the servers found on the LAN (checked = attached; click to attach/detach), with
**Refresh**, a divider and **Exit** at the bottom.

- Discovery: scans the local /24 for port 3240, plus extra hosts listed one per line in
  `servers.txt` (`%APPDATA%\usbip-client\` on Windows, `~/.config/usbip-client/` on Linux).
- Windows: calls `usbip.exe` from the bundled usbip-win2 install (BSD-2 licensed).
- Linux: uses the kernel `vhci_hcd` sysfs interface directly; privileged step runs via `pkexec`.

## usbip-win2 reference

The Windows attach backend uses [usbip-win2](https://github.com/vadimgrn/usbip-win2) (BSD-2-Clause).
Reference fork: https://github.com/pldrobot/usbip-win2 (synced with upstream). `build.ps1` downloads the
release installer from the fork first, then upstream, and embeds it in the MSI.

## Build

    cargo build --release

Linux build deps: `libgtk-3-dev libayatana-appindicator3-dev libxdo-dev`.

## Packaging

Two self-contained build scripts, one per OS — each builds the app and its installer in one go:

- Windows: `powershell -ExecutionPolicy Bypass -File .\build.ps1` → `dist\usbip-client.exe` and
  `dist\usbip-client-<ver>-x64.msi` (WiX 3 is auto-downloaded to `tools\wix3`). One UAC prompt
  installs the app, the usbip-win2 driver and autostart.
- Linux: `./build.sh` → `dist/usbip-client` and `dist/usbip-client_<ver>_amd64.deb`
  (auto-installs `cargo-deb` if missing; errors out with the exact `apt install` line if the
  GTK/appindicator dev headers aren't present). Packages add XDG autostart and load `vhci-hcd` at
  boot. For an `.rpm` instead: `cargo install cargo-generate-rpm && cargo generate-rpm`.

## Status / not yet verified

Windows build and MSI build succeed. Not yet tested: installing the MSI, `usbip.exe` output parsing
(`usbip port`), attach as a non-admin user, and the whole Linux path.
