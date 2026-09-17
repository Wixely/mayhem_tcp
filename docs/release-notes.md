mayhem_tcp v0.2.0 brings the Android USB-host app and expanded Osmocom rtl_tcp compatibility to main. Downloads include Windows x64 and Android ARM64, SHA-256 checksums and dependency licenses.

- Android 16 USB hosting with a Rust engine and .NET foreground service, configurable buffers, battery-settings shortcut and Radio settings.
- Automatic device light/dark appearance and the red circuit-crab launcher icon.
- Output rates from 225001 S/s to 3.2 MS/s, offset tuning, tuning/sample-clock PPM correction and a hardware-paced software test counter.
- Independent analog/digital AGC, gain-index support, USB buffer/device selection options and explicit antenna-power opt-in.
- Invalid settings preserve streaming; bounded queues and session cleanup support reconnection.

Windows: extract the ZIP and run `mayhem_tcp.exe --agc --digital-agc -p 12346`. Select RTL-TCP in your client. Use `-a 0.0.0.0` for remote connections.

Android: install the ARM64 APK, connect the HackRF in HackRF mode and start the server. With WireGuard enabled, use the phone's tunnel IP; direct LAN worked in our setup with WireGuard off. The APK is self-contained and needs no .NET installation.

The GitHub APK uses a temporary CI debug signing key. Updating a locally signed or older CI test APK may require uninstalling it first, which removes saved settings. Production signing is not configured.

Validation includes 24 offline Rust tests, four Android-native emulator tests, physical phone rate/mode/reconnection tests, a short locked-screen run, an 8-USB-buffer maximum-rate run and relative RF offset-tuning checks. Light/dark switching and field restoration were checked in the Android 16 emulator. CI runs offline checks and builds; it has no HackRF.

Receive-only experimental prerelease. RTL-specific IF gain, direct sampling and crystal commands are documented no-ops on HackRF. Calibrated clock accuracy, extended Android reliability, broad GUI-client compatibility and service/deployment modes remain unverified or deferred. Windows executable is unsigned. See docs/validation.md and docs/osmocom-parity.md for the precise scope.
