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

No radio is simulated: without a HackRF the app reports that no device was
found. Only one HackRF and one active TCP client are supported. USB denial,
unplug and bind failures are shown in the log. Frequency, amplifier, antenna
power and supported sample-rate limits are the same as the desktop server.
There is no transmit or firmware-writing feature.

## WireGuard / mobile data

Wi-Fi is not required. The app uses ordinary sockets and does not bind to a
specific Android network or bypass the VPN. Start WireGuard and include this
app if the tunnel uses per-app filtering. The home client connects to the
phone's tunnel IP. WireGuard AllowedIPs, home routing/firewall and peer-to-peer
forwarding must permit that connection. Changing between Wi-Fi and cellular
can break an existing TCP session; reconnect the client afterward.

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

The phone's LAN address responded to ping but TCP port 12346 timed out; only
the WireGuard address worked in that setup. The cause is not established, so
do not claim direct LAN connectivity has been qualified. No private addresses
are included in this record. Screen-off behavior, notification Stop, unplug,
long-duration throughput and network handover remain unverified on hardware.

Android owns `UsbDeviceConnection`; Rust duplicates its descriptor before
returning from `mt_create`. The service keeps the Android connection open until
`mt_run` returns, then frees the native handle and closes the connection.
Stop only sets a cancellation flag; the worker shuts down USB and network
operations before freeing resources. USB detach requests the same shutdown.
The non-sticky foreground service does not restart automatically without a
fresh user action and USB permission. CPU wake lock ownership ends with the
server worker; the screen need not stay on.

**User next action:** test retuning, AGC toggles, screen-off streaming, client reconnect, USB
unplug and notification Stop. Share the in-app log and phone model if a step
fails. **Codex next action:** fix findings and measure throughput/thermal
behavior before a stable Android release; investigate direct LAN access if
needed. Short ARM64 hardware/VPN streaming is verified as described above.

## GitHub Android builds

Pushing an `android-*` tag triggers the Android APK workflow; it can also be
rerun manually with that tag selected. It installs the pinned Rust/.NET/Android
toolchains, checks the shared code, builds ARM64, verifies the APK signature and
publishes a prerelease with APK, checksums and dependency/runtime license notices.

GitHub test APKs use a temporary debug signing key on each runner. They cannot
update an APK signed by a different local/CI key: uninstall that test app first
if Android reports a signature conflict. Uninstalling removes the app's saved
settings. Production signing and automatic updates are not configured; no
private signing key is uploaded or committed.
