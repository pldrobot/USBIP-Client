# Builds the Windows MSI: dist\usbip-client-<ver>-x64.msi
# Requires: Rust. WiX 3 binaries are downloaded to tools\wix3 on first run (no .NET SDK needed).
$ErrorActionPreference = 'Stop'
$root    = Resolve-Path "$PSScriptRoot\..\.."
$redist  = Join-Path $PSScriptRoot 'redist'
$version = (Select-String -Path "$root\Cargo.toml" -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches.Groups[1].Value
$tag     = 'v.0.9.8.1'                      # usbip-win2 release to bundle
$asset   = 'USBip-0.9.8.1-x64.exe'

New-Item -ItemType Directory -Force $redist | Out-Null
$setup = Join-Path $redist 'USBip-x64.exe'
if (-not (Test-Path $setup)) {
    Write-Host "Downloading $asset ..."
    Invoke-WebRequest "https://github.com/vadimgrn/usbip-win2/releases/download/$tag/$asset" -OutFile $setup
}
$lic = Join-Path $redist 'LICENSE-usbip-win2.txt'
if (-not (Test-Path $lic)) {
    Invoke-WebRequest 'https://raw.githubusercontent.com/vadimgrn/usbip-win2/master/LICENSE.txt' -OutFile $lic
}

$wix = Join-Path $root 'tools\wix3'
if (-not (Test-Path "$wix\candle.exe")) {
    Write-Host 'Downloading WiX 3.14 binaries ...'
    New-Item -ItemType Directory -Force $wix | Out-Null
    $zip = Join-Path $wix 'wix314-binaries.zip'
    Invoke-WebRequest 'https://github.com/wixtoolset/wix3/releases/download/wix3141rtm/wix314-binaries.zip' -OutFile $zip
    Expand-Archive $zip -DestinationPath $wix -Force
}

Push-Location $root
cargo build --release
if ($LASTEXITCODE) { throw 'cargo build failed' }
New-Item -ItemType Directory -Force dist | Out-Null

$obj = Join-Path $root 'dist\product.wixobj'
$msi = Join-Path $root "dist\usbip-client-$version-x64.msi"
& "$wix\candle.exe" -nologo -arch x64 "-dVersion=$version" "-dExePath=$root\target\release\usbip-client.exe" `
    "-dIconPath=$root\assets\logo.ico" "-dRedistPath=$setup" "-dLicensePath=$lic" -ext WixUtilExtension -out $obj "$PSScriptRoot\product.wxs"
if ($LASTEXITCODE) { throw 'candle failed' }
& "$wix\light.exe" -nologo -ext WixUIExtension -ext WixUtilExtension -sice:ICE38 -sice:ICE43 -sice:ICE57 -sice:ICE64 -o $msi $obj
if ($LASTEXITCODE) { throw 'light failed' }
Pop-Location
Write-Host "Built $msi"
