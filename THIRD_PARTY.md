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
