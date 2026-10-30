# QuickTime helper

Protocol and Media sources originate from phh235/iphone-mirror-windows commit
91e88e1423d2b7c253d4da1bcc951142e74ff035, under vendor/iphone-mirror/src/Core/src
(which vendors RayrenSX/iPhoneMirror). License: GPL-3.0-only, see LICENSE.
ScreenIOS removes SessionProtocol's source-frame diagnostics dependency.
src/quicktime_helper.cpp is also GPL-3.0-only and built as a separate executable.

libusb.h is from the same pinned tree's third_party/libusb under LGPL-2.1-or-later;
see LIBUSB-LICENSE. build.ps1 pins and verifies matching Windows runtime hashes.

The helper emits bounded H.264 Annex-B packets on stdout and stops on stdin
closure. It contains no wireless, input-control or driver-installation code.
The app's explicit installer button installs the bundled UsbDk driver. QuickTime uses a temporary USB
capture configuration; shutdown requests normal configuration. Replug the cable
if startup is interrupted. On the attached iOS 27 device, UsbDk enumeration and
capture-interface opening succeed, but both bulk read and handshake-recovery
write return LIBUSB_ERROR_IO. No QuickTime frames have been verified. The
Developer Mode path remains the tested backend.
