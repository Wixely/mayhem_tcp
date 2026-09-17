# Android USB server test build

Reviewed: 2026-09-17. Target: Android 16 (API 36), ARM64 phone/tablet,
HackRF One or PortaPack in HackRF mode, and ordinary rtl_tcp clients.
This is a separate experimental target; the Windows release is unchanged.

The USB receive engine, filtering, AGC and TCP protocol stay in Rust. A small
.NET 10 Android application provides native Android controls, USB permission
handling, a foreground connected-device service, notification Stop action and
a partial CPU wake lock while running. No Java source, Gradle, Python or Node
build pipeline is used. The Android SDK's Java compiler is still part of .NET's
normal APK build. NativeAOT is not enabled because .NET Android NativeAOT/Java
interop is experimental; the computational work is already native Rust.
Android entry points are framework Activity/Service classes rather than a
top-level console program.

## Install and try

The app follows the device's light/dark appearance automatically, including
native controls, dialogs and system-bar icon contrast. Main-screen edits are
restored when a theme change recreates the activity. Radio dialog changes
should be saved before switching themes.

1. Copy `.local/android/mayhem_tcp-android-arm64-test.apk` to the phone and
   install it, allowing installation from your chosen file/browser app if asked.
   This is a self-contained test APK signed with the local Android debug key;
   no .NET installation is needed on the phone. This is not a Play Store build.
2. Connect the HackRF using USB host/OTG and put the PortaPack in **HackRF mode**.
   Close other apps that own the USB device. Use a powered hub if required.
3. Open **mayhem_tcp**, allow notifications, leave port **12346** and
   **Listen on all interfaces (LAN / WireGuard)** selected, then tap **Start server**.
   Grant the Android USB permission when prompted.
4. Wait for the firmware/USB API and listening messages. Configure the remote
   software's **RTL-TCP** source with the phone's reachable IP and port **12346**.
   Start with **2.048 MS/s**. Both AGCs start enabled; clients can override them.
5. Stop from the app or its persistent notification before disconnecting USB.

Tap **Radio** while stopped to set initial frequency, output rate, manual gain,
PPM, USB buffer count, offset tuning, test mode and antenna-power opt-in. These
settings are saved for the next server start; clients can override the applicable
radio settings. PPM now corrects tuning and sample-clock requests. Test mode sends
a byte counter instead of IQ; offset tuning moves capture away from the hardware
centre spike and digitally recentres the wanted band. The minimum supported
output rate is 225001 S/s. See the [Osmocom parity matrix](../docs/osmocom-parity.md)
for RTL-specific controls and behavioral differences that remain.

**Buffer blocks** sets the output queue capacity (1–1024, default 32) for the
next server start and is saved with the other settings. Try 64 for brief network
stalls; larger queues can use more memory and add latency during congestion.
They cannot compensate for a connection that is consistently too slow.
Clients can now request 240 kS/s; invalid settings are logged and ignored while
the previous configuration continues streaming. Both were verified on the
physical phone over LAN with 32- and 64-block queues. Saving/restoring that
setting across an app restart remains to be tested on the phone.

No radio is simulated: without a HackRF the app reports that no device was
found. Only one HackRF and one active TCP client are supported. USB denial,
unplug and bind failures are shown in the log. Frequency, amplifier, antenna
power and supported sample-rate limits are the same as the desktop server.
Android antenna power remains off/blocked unless explicitly allowed in Radio settings.
There is no transmit or firmware-writing feature.

## WireGuard / mobile data

For background streaming, tap **Battery** for instructions and a shortcut
to Android's battery optimization list. Select mayhem_tcp (choose All apps if
needed), then Don't optimize or Unrestricted; wording varies by phone. If that
settings page is unavailable, the shortcut opens app details instead. Settings
are changed only by the user. This can increase battery use and is not a verified
fix for background interruptions. **About** shows the project repository and an
**Open GitHub** button for source, documentation and issue reports.

Wi-Fi is not required. The app uses ordinary sockets and does not bind to a
specific Android network or bypass the VPN. Start WireGuard and include this
app if the tunnel uses per-app filtering. The home client connects to the
phone's tunnel IP. WireGuard AllowedIPs, home routing/firewall and peer-to-peer
forwarding must permit that connection. Changing between Wi-Fi and cellular
can break an existing TCP session; reconnect the client afterward.

On the tested Android 16 phone, use the phone's **WireGuard IP while WireGuard
is enabled**, or its **LAN IP with WireGuard disabled**. Direct LAN access failed
with the tunnel enabled and worked after the user turned WireGuard off, with
no app change. Keep **Listen on all interfaces (LAN / WireGuard)** selected and
use port **12346**. This points to VPN routing or policy; the exact setting
responsible was not identified. Simultaneous direct LAN and WireGuard access
has not been verified.

At 2.048 MS/s the IQ payload is 32.768 Mbit/s, about 14.75 GB/hour, before
TCP/VPN overhead. USB receives 16.384 MB/s at that setting. CPU/thermal limits,
cellular bandwidth, screen-off behavior, USB power and firmware compatibility
need real-device qualification. No throughput is guaranteed by the APK build.

The manifest targets API 36 and requests INTERNET, USB permission through the
system dialog, foreground-service permissions, notifications and WAKE_LOCK.
It does not request Wi-Fi-only connectivity or network discovery. If targeting
API 37 later, implement the newer local-network runtime permission.

## Build on Windows

Requirements: Rust with `aarch64-linux-android`, .NET SDK 10 with the Android
workload, Android SDK platform/build tools 36, NDK r28c or later, and JDK 21.
The currently tested host has .NET 10.0.300, Android workload 36.1.53,
Rust 1.98.1 and NDK 28.2.13676358. No SDK/license installation is performed by
the script.

```powershell
rustup target add aarch64-linux-android
powershell -NoProfile -File scripts/build-android.ps1
```

The script accepts `-AndroidSdk` and `-JavaSdk`; otherwise it uses ANDROID_HOME,
JAVA_HOME or the usual Visual Studio installation directories. Run **Build
Android ARM64 test APK** from VS Code's tasks menu for the same build. The
existing desktop VS Code debug profiles can debug the shared Rust server.
Device debugging of the .NET shell uses a .NET Android-capable debugger;
the APK and `adb logcat -s mayhem_tcp AndroidRuntime` are sufficient for this
initial physical-device test. Android native breakpoint debugging is not yet
configured or verified.

For an x86_64 emulator:

```powershell
rustup target add x86_64-linux-android
powershell -NoProfile -File scripts/build-android.ps1 -Architecture x64
adb install -r .local/android/mayhem_tcp-android-x64-test.apk
```

Native libraries use 16 KiB maximum page alignment and remapped source paths.
Build products, native libraries, APKs, logs and signing material are ignored.
Keep the debug signing key for installing subsequent test builds over this
one; use a separately managed release key before public distribution.

## Ownership and verification

Verified on 2026-09-17:

- ARM64 and x86_64 self-contained debug APK builds, zero compiler warnings/errors.
- ARM64 APK signature, API 36 target, ARM64-only payload and Rust ELF 16 KiB alignment.
- Android 16 x86_64 emulator installation, app launch, native library loading,
  visible controls, invalid-port validation and missing-HackRF error handling.
- Three native tests executed inside Android: invalid descriptor rejection,
  duplicated-descriptor ownership with stop-before-run, and clean rejection of
  a non-USB descriptor without unwinding across the FFI boundary.
- All 17 existing desktop offline tests, formatting and Clippy passed; Android
  native Clippy also passed. The existing Windows hardware server was running,
  so the exclusive hardware suite was not rerun or allowed to interrupt it.

A subsequent physical Android-device test successfully served rtl_tcp over
the phone's WireGuard address. The greeting was RTL0 / tuner 5 / 29 gain
entries; with both AGCs requested, 20,512,768 bytes arrived in 5.008 seconds,
measuring 2,047,965 complex samples/s at the requested 2.048 MS/s. The user
also confirmed their client worked. This verifies short USB-to-VPN streaming,
not calibrated RF performance or sustained operation.

Direct LAN access initially failed while WireGuard was enabled. After the user
disabled WireGuard on the phone, a TCP connection to the LAN address on port
12346 succeeded, returned the RTL0 greeting and delivered 7,300 bytes of IQ.
This verifies a short LAN connection and data reception, not sustained LAN
throughput. No private addresses are included in this record.
An updated-APK LAN regression on 2026-09-17 subsequently passed with the user
confirming 32 buffer blocks: 240/250 kS/s and 2/2.048/2.4/3.2 MS/s rate requests,
fragmented/coalesced commands, gain/AGC toggles, retuning, three reconnects,
invalid-command recovery, stalled-client disconnection, and rejection of a
second client without disrupting the first. At 240 kS/s, 239,955 samples/s
were measured over four seconds. Six consecutive ten-second measurements at
2.048 MS/s ranged from 2,047,559 to 2,048,819 samples/s without disconnecting.
Samples were counted and discarded; RF accuracy and losslessness were not measured.
A repeat on 2026-09-17 with the user reporting **64 buffer blocks and the app
in the background** passed the same full suite. Six ten-second windows at
2.048 MS/s measured 2,047,935–2,049,532 samples/s without a disconnect; the
240 kS/s sweep measurement was 240,039 samples/s. This verifies short background
operation in that setup. Screen state, Doze and battery settings were not recorded,
so it does not establish screen-off reliability or the cause of earlier failures.
A subsequent user-coordinated lock-screen test on 2026-09-17 streamed over LAN
at 2.048 MS/s for over three minutes with the same 64-block setup. All eighteen
ten-second measurement windows passed (2,046,464–2,049,567 samples/s), followed
by a successful fresh connection and five seconds of IQ. The phone's lock state
was coordinated with the user, not independently queried through ADB. This is
a short lock-screen test, not proof of Doze or overnight reliability.
Notification Stop, unplug, long-duration throughput and network handover remain
unverified on hardware.

Android owns `UsbDeviceConnection`; Rust duplicates its descriptor before
returning from `mt_create`. The service keeps the Android connection open until
`mt_run` returns, then frees the native handle and closes the connection.
Stop only sets a cancellation flag; the worker shuts down USB and network
operations before freeing resources. USB detach requests the same shutdown.
The non-sticky foreground service does not restart automatically without a
fresh user action and USB permission. CPU wake lock ownership ends with the
server worker; the screen need not stay on.

Remaining reliability tests (longer screen-off sessions, physical-device setting
persistence, USB unplug and notification Stop) are **deferred at the user's
request**. The parity-update APK subsequently passed physical LAN checks for
225001 S/s, offset-mode streaming, PPM changes, test-counter continuity, invalid
commands, return to ordinary IQ and reconnects; see the parity matrix for results.
The subsequent user-configured 8-USB-buffer test (64 output queue blocks)
passed rate/mode changes, counter continuity, reconnecting and a minute at
3.2 MS/s; see [validation details](../docs/validation.md).
Calibrated frequency/sample-clock testing is deferred at the user's request.
**Codex next action:** extend protocol regression coverage and test an unmodified
rtl_tcp client against Android. Other non-default USB counts remain untested.
Earlier short ARM64 hardware/VPN, LAN and locked-screen streaming results apply
to the previously tested build; the new parity run verifies streaming behavior,
not RF calibration or a repeat of all Android reliability tests.

## GitHub Android builds

Pushing a `v*` tag builds Windows and Android together in one GitHub prerelease.
Pushing an `android-*` tag triggers the standalone Android APK workflow; it can also be
rerun manually with that tag selected. It installs the pinned Rust/.NET/Android
toolchains, checks the shared code, builds ARM64, verifies the APK signature and
publishes a prerelease with APK, checksums and dependency/runtime license notices.

GitHub test APKs use a temporary debug signing key on each runner. They cannot
update an APK signed by a different local/CI key: uninstall that test app first
if Android reports a signature conflict. Uninstalling removes the app's saved
settings. Production signing and automatic updates are not configured; no
private signing key is uploaded or committed.
