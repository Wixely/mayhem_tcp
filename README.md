# mayhem_tcp

A Rust proof of concept that makes a HackRF One available to **existing rtl_tcp
clients**. Tested on Windows with PortaPack Mayhem **v2.4.0**, USB API **0x0111**,
and the existing WinUSB driver. Reviewed: **2026-09-17**.

The executable is `mayhem_tcp.exe`. It receives IQ, filters/downsamples to the
client's requested rate, and sends the ordinary unsigned 8-bit rtl_tcp stream.
An unmodified **rtl_433 25.12** client successfully connected and received at
250 kS/s and 2.048 MS/s. No custom client plugin, firmware change, or native
libhackrf/libusb DLL is required.

## Try it on this computer

Windows x64 packages are published on the
[GitHub releases page](https://github.com/Wixely/mayhem_tcp/releases).
Extract the ZIP, select HackRF mode on the PortaPack, then run
`mayhem_tcp.exe --agc --digital-agc -p 12346` and connect your client to
`127.0.0.1:12346`. Prereleases are experimental and unsigned. Each package
includes documentation, dependency licenses and a separate SHA-256 checksum.

Select **HackRF mode** on the PortaPack and close any other application using
the device. From this directory:

```powershell
.\target\release\mayhem_tcp.exe
```

In your application, select **RTL-TCP / rtl_tcp**, host **127.0.0.1**, port
**1234**. Enable **tuner AGC / automatic tuner gain**, or use manual gain
initially around **32-40 dB**, and a sample rate
such as **2.048 MS/s**. The initial tuning is **100 MHz**; the client can change
it. The server presents an **R820T compatibility profile**, although the actual
hardware is HackRF. Disconnecting releases the radio; reconnecting starts a
fresh session. Stop the server with Ctrl+C.

For another computer on your LAN:

```powershell
.\target\release\mayhem_tcp.exe -a 0.0.0.0 -p 1234
```

Connect to this computer's LAN address and allow TCP port 1234 through its
firewall as appropriate. The default localhost binding does not accept LAN
connections. No firewall or service settings were changed during development.

Build from source or view options:

```powershell
cargo build --release --locked
.\target\release\mayhem_tcp.exe --help
```

## Scope and limitations

- One receive client at a time. No transmission or firmware-writing features.
- Output rates **250 kS/s through 3.2 MS/s**; USB capture uses a power-of-two
  multiple at least 8 MS/s, followed by a real low-pass decimator.
- Tuning, analog AGC, manual gain/gain index and tuning-only PPM correction are implemented.
- Analog AGC adjusts LNA/VGA from raw IQ levels. Enable it in the client or
  start with `--agc` (clients can override).
- Digital IQ AGC is independently controlled by the client's **Digital/RTL AGC**
  option or `--digital-agc`. It scales filtered IQ before 8-bit conversion;
  both AGCs can run together. Both default off until enabled by the client or CLI.
  This is software level control, not an exact RTL2832 AGC emulation, and cannot
  recover ADC clipping or improve the received signal-to-noise ratio.
- RF amplifier stays off. Antenna power requests are blocked unless enabled
  with `--allow-bias-tee`, and power is disabled on session cleanup.
- Tuning/rate changes restart capture; brief gaps/stale TCP-buffered samples are
  possible. There are no timestamps or loss markers in the legacy protocol.
- Gain and auto/manual changes run live without restarting RX or the filter.
  Gain transitions still have hardware settling effects. Switching back to
  manual restores the last requested manual gain.
- Stalled readers/invalid implemented settings terminate the session; a later
  client can reconnect. Unknown RTL-specific commands are ignored and logged.
- Only the hardware/rates/clients in the [validation record](docs/validation.md)
  have been tested. No GUI client, calibrated RF signal, or long-term lossless
  capture claim is made. No authentication/encryption is added to rtl_tcp.

See [protocol behavior](docs/protocol.md), [deployment options](docs/deployment.md),
[dependency provenance](docs/dependencies.md), and the earlier
[feasibility investigation](docs/feasibility.md).

Windows interactive operation is tested. Windows Service support is compiled;
SCM operation, Linux/systemd and Docker deployment remain unverified. Optional
deployment files are included. The Windows MSVC executable statically links the
C runtime. It still requires the OS's USB driver.

## Tests and debugging

```powershell
cargo test --release --locked
cargo clippy --all-targets --locked -- -D warnings
cargo fmt -- --check
```

The hardware test is **opt-in** and takes exclusive use of the attached HackRF:

```powershell
cargo test --release --locked --test hardware -- --ignored --nocapture
```

It launches its own localhost server, checks IQ rates, tuning, fragmented
commands, reconnects, invalid settings, a stalled reader, and live AGC/manual
transitions. Hardware tests serialize their use of the radio. Samples are
discarded. Tests use 100/101 MHz and leave RX/antenna power off when finished.

Open this folder in VS Code with the recommended **rust-analyzer** and Microsoft
**C/C++** extensions. Put the PortaPack in HackRF mode, then select
**mayhem_tcp (localhost, both AGCs)** in Run and Debug and press **F5**.
The launch builds automatically and shows server logs in the integrated terminal.
Connect your rtl_tcp client to **127.0.0.1:12346**. The debug profiles use this port to avoid a local service occupying 1234. Both AGCs start enabled;
client commands can override them. Use **mayhem_tcp (localhost, manual gain)**
to start at 32 dB with both AGCs off. The information-only USB probe remains
available in the launch selector.

Use **Terminal > Run Task > Test mayhem_tcp** for offline tests. For the opt-in
hardware suite, stop the debug server and any other HackRF application, then run
**Test mayhem_tcp (live HackRF; stop debug server first)**. Stop interactive
reception with Ctrl+C in the server terminal for normal cleanup.

These Windows launch configurations use the Microsoft C/C++ debugger (`cppvsdbg`).
Build tasks were verified; an interactive debugger session was not run. Development builds
retain debug information and enable optimization for real-time DSP. Pausing
the debugger while streaming can overflow buffers and disconnect the client.

The original information-only probe remains available:

```powershell
cargo run --release --locked --manifest-path tools/usb-probe/Cargo.toml
```

Adding `-- --rx` to that command tests raw USB receive at 8, 10 and 20 MS/s.
Both tools change runtime tuning/gain during RX tests and do not restore the
previous configuration. Neither performs RF transmission or flash writes.

## Next action

The proof of concept is built and hardware-tested. **User:** choose and try your
intended rtl_tcp application. **Codex:** address its specific compatibility
issues next, then validate signal quality, longer captures and deployment modes
before considering a release.
