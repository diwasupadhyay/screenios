$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$sdk = Join-Path $PSScriptRoot '.local/ffmpeg/ffmpeg-master-latest-win64-lgpl-shared'
if (-not (Test-Path (Join-Path $sdk 'include/libavcodec/avcodec.h'))) {
    New-Item -ItemType Directory -Force .local | Out-Null
    $url = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-lgpl-shared.zip'
    Invoke-WebRequest $url -OutFile .local/ffmpeg.zip
    $actual = (Get-FileHash .local/ffmpeg.zip -Algorithm SHA256).Hash
    if ($actual -ne 'C76D748F241055AA34010741F098886BBB82506DBC76D52C1B62232B1C428593') {
        throw 'Upstream decoder SDK changed. Review its ABI and update the pinned hash before building, or supply the documented SDK manually.'
    }
    Expand-Archive .local/ffmpeg.zip .local/ffmpeg -Force
}
cargo build --release
if ($LASTEXITCODE -ne 0) { throw 'Native build failed' }
$decoder = Join-Path $PSScriptRoot '.local/ffmpeg/ffmpeg-master-latest-win64-lgpl-shared/bin'
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
