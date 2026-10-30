Source: https://github.com/oslo254804746/rust-ios-device (ios-core 0.1.15).
Upstream MIT / Apache-2.0 licenses are retained in this directory.

ScreenIOS builds only the display and userspace tunnel features. No HID,
clipboard, Bonjour, remote pairing, or wireless discovery features are enabled.

Local changes:
- USB-only usbmux device selection, with no network fallback.
- Bounded IPv6 UDP receive tap and transmit channel in the userspace tunnel.
- Compile wireless PSK/OpenSSL code only when wireless discovery is enabled.
- Correct top-level display request integers and the stop request for iOS 27.
- Allow the device's default video resolution adaptation.

This is protocol code, not a bundled application or executable.
