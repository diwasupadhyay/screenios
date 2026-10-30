$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$usbRoot = Join-Path $PSScriptRoot '.local/libusb'
New-Item -ItemType Directory -Force $usbRoot | Out-Null
$usbFiles = @(
    @('bin/x64/libusb-1.0.dll', '5072054CB3002AE071F382AD5C2C2B0092D9451C537D0C13444C2B6F968F7251'),
    @('lib/x64/libusb-1.0.lib', '821F07F42BE6E00E3377B86E40D9EAE1E68EDC126064016BE95BCFAA6F53C29F')
)
foreach ($entry in $usbFiles) {
    $destination = Join-Path $usbRoot (Split-Path $entry[0] -Leaf)
    if (-not (Test-Path $destination)) {
        Invoke-WebRequest ('https://raw.githubusercontent.com/phh235/iphone-mirror-windows/91e88e1423d2b7c253d4da1bcc951142e74ff035/vendor/iphone-mirror/third_party/libusb/' + $entry[0]) -OutFile $destination
    }
    if ((Get-FileHash $destination -Algorithm SHA256).Hash -ne $entry[1]) { throw 'USB runtime hash mismatch' }
}
$sdk = Join-Path $PSScriptRoot '.local/ffmpeg/ffmpeg-N-128142-g28fda48e0b-win64-lgpl-shared'
if (-not (Test-Path (Join-Path $sdk 'include/libavcodec/avcodec.h'))) {
    New-Item -ItemType Directory -Force .local | Out-Null
    $url = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-10-28-19-47/ffmpeg-N-128142-g28fda48e0b-win64-lgpl-shared.zip'
    Invoke-WebRequest $url -OutFile .local/ffmpeg.zip
    $actual = (Get-FileHash .local/ffmpeg.zip -Algorithm SHA256).Hash
    if ($actual -ne 'CDC8388862F20BB0393141F944FE18B54E7A6DE50FD42A24DB58C63FB48C6A8C') {
        throw 'Upstream decoder SDK changed. Review its ABI and update the pinned hash before building, or supply the documented SDK manually.'
    }
    Expand-Archive .local/ffmpeg.zip .local/ffmpeg -Force
}
$ErrorActionPreference = 'Continue'
cargo build --release
$buildExit = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($buildExit -ne 0) { throw 'Native build failed' }
$decoder = Join-Path $PSScriptRoot '.local/ffmpeg/ffmpeg-N-128142-g28fda48e0b-win64-lgpl-shared/bin'
foreach ($dll in @('avcodec-63.dll','avutil-61.dll','swscale-10.dll','swresample-7.dll')) {
    Copy-Item (Join-Path $decoder $dll) (Join-Path $PSScriptRoot 'target/release') -Force
}
New-Item -ItemType Directory -Force dist/ScreenIOS | Out-Null
$driverInstaller = Join-Path $PSScriptRoot '.local/UsbDk_1.0.22_x64.msi'
if (-not (Test-Path $driverInstaller)) {
    Invoke-WebRequest 'https://github.com/daynix/UsbDk/releases/download/v1.00-22/UsbDk_1.0.22_x64.msi' -OutFile $driverInstaller
}
if ((Get-FileHash $driverInstaller -Algorithm SHA256).Hash -ne '91F6F695E1E13C656024E6D3B55620BF08D8835EF05EE0496935BA6BB62466A5') { throw 'Driver installer hash mismatch' }
New-Item -ItemType Directory -Force dist/ScreenIOS/drivers | Out-Null
Copy-Item $driverInstaller dist/ScreenIOS/drivers -Force
Copy-Item vendor/quicktime/USBDK-LICENSE dist/ScreenIOS/drivers -Force
Copy-Item target/release/screenios.exe dist/ScreenIOS/ScreenIOS.exe -Force
Copy-Item target/release/ScreenIOS.UsbCapture.exe dist/ScreenIOS -Force
Copy-Item .local/libusb/libusb-1.0.dll target/release -Force
Copy-Item .local/libusb/libusb-1.0.dll dist/ScreenIOS -Force
Copy-Item vendor/quicktime/LICENSE dist/ScreenIOS/QuickTime-HELPER-LICENSE.txt -Force
Copy-Item vendor/quicktime/LIBUSB-LICENSE dist/ScreenIOS/libusb-LICENSE.txt -Force
foreach ($dll in @('avcodec-63.dll','avutil-61.dll','swscale-10.dll','swresample-7.dll')) {
    Copy-Item (Join-Path $decoder $dll) dist/ScreenIOS -Force
}
Copy-Item (Join-Path $sdk 'LICENSE.txt') dist/ScreenIOS/FFmpeg-LICENSE.txt -Force
Copy-Item THIRD_PARTY.md dist/ScreenIOS -Force
Write-Host 'Ready: dist/ScreenIOS/ScreenIOS.exe'
