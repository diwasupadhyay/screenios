$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
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
Copy-Item target/release/screenios.exe dist/ScreenIOS/ScreenIOS.exe -Force
foreach ($dll in @('avcodec-63.dll','avutil-61.dll','swscale-10.dll','swresample-7.dll')) {
    Copy-Item (Join-Path $decoder $dll) dist/ScreenIOS -Force
}
Copy-Item (Join-Path $sdk 'LICENSE.txt') dist/ScreenIOS/FFmpeg-LICENSE.txt -Force
Copy-Item THIRD_PARTY.md dist/ScreenIOS -Force
Write-Host 'Ready: dist/ScreenIOS/ScreenIOS.exe'
