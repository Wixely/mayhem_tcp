Experimental Android 16 / ARM64 USB-host server for HackRF and PortaPack Mayhem.
The USB/TCP/DSP/AGC engine is Rust; the Android UI and foreground service use .NET.

Install the ARM64 APK, connect a HackRF in HackRF mode, tap Start server and
grant USB access. Both AGCs and all-interface binding default on; port is 12346.
Choose RTL-TCP in the client and use the phone's WireGuard IP while the tunnel
is enabled, or its LAN IP with WireGuard disabled.

Short physical-device streaming over WireGuard was verified at 2.048 MS/s.
Direct phone-LAN access also returned the RTL0 greeting and IQ data after
disabling WireGuard on the phone. LAN access with WireGuard enabled failed in
that setup; the specific VPN routing or policy cause remains unidentified.
Screen-off, unplug, notification Stop and sustained throughput need more testing.

This self-contained APK needs no separate .NET installation. It is signed with
a temporary CI debug key, so Android may require uninstalling a previous test
APK signed with a different key before installation. Uninstalling removes saved
settings. This is not a production-signed or Play Store release.

SHA-256 checksums and dependency/runtime license notices are attached.
