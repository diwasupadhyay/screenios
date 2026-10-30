ScreenIOS uses ios-core 0.1.15 from
https://github.com/oslo254804746/rust-ios-device at
e73c95156a7f7d7855adcd5739f0a317ea62f062 (MIT OR Apache-2.0).
Its licenses and local changes are retained in vendor/ios.

The local decoder SDK is BtbN FFmpeg N-128142-g28fda48e0b-20261028,
LGPLv3 shared build, libavcodec 63 / libavutil 61 / libswscale 10.
Source: https://github.com/FFmpeg/FFmpeg/tree/28fda48e0b
Build scripts and dependency sources: https://github.com/BtbN/FFmpeg-Builds
Downloaded archive SHA256:
CDC8388862F20BB0393141F944FE18B54E7A6DE50FD42A24DB58C63FB48C6A8C
The DLLs are dynamically loaded and may be replaced with ABI-compatible builds.
FFmpeg's license is copied to the local distribution by build.ps1.
No binary distributions or device data are committed to this repository.

Other Rust dependencies and exact versions are recorded in Cargo.lock.

The separate ScreenIOS.UsbCapture.exe helper uses QuickTime protocol sources
from phh235/iphone-mirror-windows commit
91e88e1423d2b7c253d4da1bcc951142e74ff035 (which vendors RayrenSX/iPhoneMirror).
The helper is GPL-3.0-only; its complete corresponding source is in
src/quicktime_helper.cpp and vendor/quicktime in this repository:
https://github.com/diwasupadhyay/screenios . Build instructions: README.md.
Changes remove the upstream frame-diagnostics dependency and add a USB-only
capture process with bounded H.264 output. License: vendor/quicktime/LICENSE.
libusb is dynamically linked by that helper under LGPL-2.1-or-later;
its header and license are in vendor/quicktime. Runtime hashes are pinned in
build.ps1. Upstream source: https://github.com/libusb/libusb .
UsbDk 1.0.22 x64 is bundled in the local package under Apache-2.0, with its
license retained in vendor/quicktime/USBDK-LICENSE. Source:
https://github.com/daynix/UsbDk/tree/v1.00-22 . Installation is user-initiated
and uses the original signed MSI with Windows elevation. Its SHA-256 is
91F6F695E1E13C656024E6D3B55620BF08D8835EF05EE0496935BA6BB62466A5.
