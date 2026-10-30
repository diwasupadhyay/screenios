# ScreenIOS

<img src="screenios.png" width="96" alt="ScreenIOS logo">

Native Windows 11 USB-only iPhone viewer. Rust handles USB and the window; a
small C++ adapter uses FFmpeg with D3D11 hardware decoding and a software fallback.
No Python, browser runtime, phone input, or wireless fallback.

Connect an unlocked, trusted iPhone running iOS 27. Click **Connect**.
The connection card becomes a borderless floating screen with one slim control
strip above the video. Drag the empty part of the strip to move it. Resize from
any edge or corner with normal resize cursors; the floating window retains the
phone's aspect ratio without side bars. The strip contains Disconnect,
Fullscreen, Minimize and Close. Double-click the video or press F11 for
fullscreen; Escape exits fullscreen. Controls never cover the phone screen.

The startup screen offers two USB modes:
- **Developer Mode**: the tested CoreDevice path. Requires Apple's USB driver
  (Apple Devices/iTunes), Trust, Developer Mode, and an available display service.
- **Without Developer Mode**: experimental QuickTime USB capture through a
  separate native helper. Use **Install USB driver** to install the bundled,
  hash-verified UsbDk driver with Windows administrator approval. The installer
  works offline. Developer Mode-off streaming has not yet been
  verified on the attached iOS 27 device. There is no silent fallback between modes.

Current non-developer test result: UsbDk installs and opens the capture interface,
but this PC/iPhone combination returns `LIBUSB_ERROR_IO` on bulk transfers,
including the handshake retry. This mode is experimental, not a verified
alternative to the working Developer Mode path.

Follow the mode-specific setup instructions in the app. iOS displays its own
Trust, passcode, restart and Developer Mode confirmations on the phone.
Driver installation only happens when explicitly requested. ScreenIOS does not
install phone apps or developer images. Apple's USB driver is still required
for Developer Mode; install Apple Devices if it is missing.
Protected content may not be captured by iOS.

## Build

Install Rust for Windows MSVC and Visual Studio C++ Build Tools. Run
`powershell -ExecutionPolicy Bypass -File build.ps1`.
The script downloads the native decoder SDK on first build. Internet is needed
for build dependencies only; mirroring uses USB exclusively.

Run `dist/ScreenIOS/ScreenIOS.exe`. Keep its DLLs and USB helper beside it.
Executables, DLLs, downloaded SDKs, logs and device data are excluded from Git.

`cargo test --bin screenios` runs protocol tests. `ScreenIOS.exe --smoke` runs
two live connection cycles and a fullscreen transition, requiring a real phone.
Smoke diagnostics are local only. Cable-unplug handling requires a manual test.

The USB backend is an adapted, licensed copy of ios-core; see
`vendor/ios/SCREENIOS-CHANGES.md`. FFmpeg is dynamically linked under LGPLv3;
its license and source references accompany the local build.
