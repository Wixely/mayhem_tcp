# HackRF / Mayhem rtl_tcp server feasibility

Investigation and source review date: **2026-09-17**. Recheck dependency and firmware status before implementation/release.

**Subsequent implementation update (2026-09-17):** the `mayhem_tcp` proof of
concept now exists. The report below preserves the investigation's original
recommendations; current implementation status and remaining work are in the
[README](../README.md) and [validation record](validation.md).

## Recommendation

Build a receive-only Rust host application exposing the standard `rtl_tcp` wire protocol, using HackRF mode on the existing Mayhem firmware. The user explicitly selected existing `rtl_tcp` clients as the target. Start with one controlling client, Windows first, and a USB backend isolated from the network and DSP code.

There is direct evidence that Rust can configure and receive from this particular device. The remaining uncertainty is client compatibility, sample-rate conversion quality, and production robustness, rather than basic firmware access.

Strictly speaking, `rtl_sdr` is the local sample-capture utility; `rtl_tcp` is the network server being emulated. The new program would run on the USB-connected computer. It would not run inside the PortaPack or give it standalone networking.

## What was verified on this computer

The workspace initially contained no files and was not a Git repository. Rust was already installed; `hackrf_info` and `hackrf_transfer` were not on PATH. No drivers, integrations, or firmware were installed or changed.

Windows PnP reported **HackRF One**, status **OK**, driver service **WINUSB**. The diagnostic in `tools/usb-probe` uses `nusb` 0.2.7 directly and returned:

```text
USB API: 0x0111
Board ID: [2]
Firmware: v2.4.0
```

Receive tests used 100 MHz, LNA/VGA 16 dB, RF amplifier off, antenna power off, and a persistent USB read queue of 16 x 256 KiB transfers. Samples were counted and discarded, with a byte histogram to establish that the returned buffers varied.

| Requested complex sample rate | Bytes received | Approximate duration | Measured complex sample rate |
| --- | ---: | ---: | ---: |
| 8 MS/s | 80,216,064 | 5.014 s | 8.000 MS/s |
| 10 MS/s | 100,139,008 | 5.007 s | 10.000 MS/s |
| 20 MS/s | 200,015,872 | 5.000 s | 20.000 MS/s |

All three receive runs and stop operations completed without a reported error. A subsequent device-information query succeeded. Debug/release builds, `cargo fmt --check`, and Clippy with warnings denied passed on Windows.

These are short USB throughput checks, **not** evidence of lossless long-term capture, calibrated RF performance, correct demodulation, network performance, or compatibility with any particular client. No sequence-number loss measurement or known-signal comparison was performed. The program sets a frequency but does not independently measure LO accuracy. Linux, service deployment, Docker, and interactive VS Code debugging were not exercised. No RF transmission or flash write was performed.

## Why Mayhem 2.4 works

Mayhem has a HackRF mode that exposes the usual HackRF USB controls and bulk IQ stream. Its ordinary PortaPack UI uses a different control path; remote screen/buttons/files and high-rate HackRF IQ streaming are separate features. Do not design the first version around simultaneously running a standalone Mayhem RF application and owning the radio from the computer.

The [Mayhem v2.4.0 tree](https://github.com/portapack-mayhem/mayhem-firmware/tree/v2.4.0) pins its HackRF submodule to `38e082b9399fae30241a6ca5a1e1be0e157115de`. That submodule's [USB descriptor](https://github.com/portapack-mayhem/hackrf/blob/38e082b9399fae30241a6ca5a1e1be0e157115de/firmware/hackrf_usb/usb_descriptor.c) declares API `0x0111`, matching the connected device. The [transceiver implementation](https://github.com/portapack-mayhem/hackrf/blob/38e082b9399fae30241a6ca5a1e1be0e157115de/firmware/hackrf_usb/usb_api_transceiver.c) implements the relevant controls.

The [Mayhem serial shell](https://github.com/portapack-mayhem/mayhem-firmware/blob/v2.4.0/firmware/application/usb_serial_shell.cpp) also includes a `hackrf` command. Automatic mode switching could be a later convenience, but was not tested: this device was already accessible through the HackRF interface. No downgrade is indicated. Firmware display strings should be reported for diagnostics; support should be based on USB API capabilities and tested hardware behavior.

## The old hackrf_tcp project

The likely project is [jpenalbae/hackrf_tcp](https://github.com/jpenalbae/hackrf_tcp), a 2014-era C proof of concept. Its README says it was to be abandoned, was tested only on Linux/macOS, needed a restart after disconnect, and required a custom gr-osmosdr source.

Its [protocol/source](https://github.com/jpenalbae/hackrf_tcp/blob/master/hackrf_tcp.c) uses its own greeting, framed data, commands and responses, including a 64-bit parameter. This is incompatible with standard `rtl_tcp`. It calls libhackrf rather than requiring a specifically named firmware version in its README. Therefore the claim that old firmware was the sole problem is **unconfirmed**. Old host libraries, drivers, platform assumptions, or the different protocol could explain a failure, but the original failing binary/logs were not available to diagnose.

Reviving it would not satisfy unmodified `rtl_tcp` clients without replacing the protocol. Its declared GPL-3.0 license also differs from the user's default MIT preference for new open-source work. A fresh implementation is the better starting point.

## Practical options

| Option | Fit for this task | Assessment |
| --- | --- | --- |
| New Rust rtl_tcp server over native USB (`nusb`) | Best fit for Rust, Windows, compact deployment | Direct USB feasibility verified here; requires a small, carefully tested HackRF backend plus DSP/protocol work. |
| Rust rtl_tcp server over current official libhackrf | Strong alternative | Reuses the vendor's device handling; adds C library/build/distribution dependencies. Use as a reference backend or fallback if native USB maintenance becomes burdensome. |
| Existing experimental Rust HackRF crate | Potential shortcut | Audit stream lifecycle, raw sample access, Windows coverage, and API support before adopting. |
| SoapyRemote + SoapyHackRF | Existing general remote-SDR solution | Useful if clients support Soapy; does not expose the rtl_tcp protocol. More components than a dedicated server. |
| SDR++ server + HackRF source | Existing SDR++ remote solution | Appropriate for SDR++ clients, but its server protocol is different from rtl_tcp. |
| Revive old hackrf_tcp | Poor fit | Custom client dependency, old implementation limitations, and protocol replacement still needed. |

The [official libhackrf header](https://github.com/greatscottgadgets/hackrf/blob/main/host/libhackrf/src/hackrf.h) provides the broad reference API. The [nusb project](https://github.com/kevinmehall/nusb) supplies cross-platform USB without libusb; only its Windows path was verified here.

Two relevant Rust libraries are [hackrfone](https://github.com/newAM/hackrfone) and [hackrf-nusb](https://github.com/bastibl/hackrf-nusb). Both identify themselves as experimental. `hackrfone` states that it is incomplete and tested only on Linux; its v0.4.0 receive method creates a reader per call, so it is not the preferred continuous-stream foundation without further work. `hackrf-nusb` documents a persistent queue and explicit stream lifecycle, but exposes Complex32 samples without an alternate raw stream format. Converting byte IQ to floats and back for a simple TCP relay is avoidable overhead. Neither library was hardware-tested in this investigation; the diagnostic tested nusb directly.

[SoapyRemote](https://github.com/pothosware/SoapyRemote) and [SoapyHackRF](https://github.com/pothosware/SoapyHackRF) are complementary components. [SoapyRTLTCP](https://github.com/pothosware/SoapyRTLTCP) works in the opposite direction: it lets Soapy clients consume an existing rtl_tcp server, rather than making a HackRF server compatible. [SDR++ server source](https://github.com/AlexandreRouma/SDRPlusPlus/blob/master/core/src/server.cpp) implements its own framed control/data protocol.

There is also [docker-soapy2tcp-channels](https://github.com/rpatel3001/docker-soapy2tcp-channels), which exposes downsampled Soapy channels as rtl_tcp streams. Its author reports testing only an RSP1 clone and significant CPU use. It depends on Python/Numba, outside the user's current tooling permission, and was not run. It is evidence of an existing bridge approach, not a validated Windows/HackRF substitute.

## Compatibility work that actually matters

The [upstream rtl_tcp implementation](https://github.com/osmocom/rtl-sdr/blob/master/src/rtl_tcp.c) establishes the greeting and command formats. A compatibility server must send the 12-byte `RTL0` greeting, followed by uninterrupted unsigned 8-bit interleaved IQ; parse 5-byte commands with a network-order 32-bit parameter; and tolerate commands split or combined across TCP reads. It cannot insert its own error messages or metadata into the IQ stream.

HackRF supplies signed 8-bit IQ. For unprocessed byte samples, converting to offset-binary unsigned samples is an XOR with `0x80` per byte. DSP output needs appropriate scaling, clipping, and rebiasing instead.

**Sample rates are the main DSP requirement.** [HackRF's sampling guidance](https://github.com/greatscottgadgets/hackrf/blob/main/docs/source/sampling_rate.rst) recommends at least 8 MS/s, and filtering plus decimation for lower output rates. An rtl_tcp client may request 2.0, 2.048, or 2.4 MS/s, or substantially less. The server must emit the requested rate accurately. Start with 8 -> 2 MS/s using a proper low-pass decimator. Qualify additional hardware-rate/divisor combinations or use a rational resampler for other rates. Never silently substitute 8 MS/s for a 2 MS/s request, and do not downsample merely by discarding samples without filtering. Higher-rate capture remains on USB; only the reduced stream crosses the network.

**Gain mapping needs an explicit policy.** HackRF's independent LNA, VGA and switchable RF amplifier do not correspond directly to an RTL tuner's gain table. Clients may rely on known tuner IDs and locally stored gain tables. Test the advertised tuner type and gain-count behavior against actual applications; an invented HackRF tuner ID will not automatically work. Map manual total gain to a documented LNA/VGA schedule, keep RF amplifier control explicit, and decide whether automatic gain is software-implemented or unsupported. Do not claim to reproduce RTL hardware AGC.

**The protocol limits exposed capabilities.** The ordinary frequency command tops out at 4,294,967,295 Hz, below HackRF One's [specified 6 GHz range](https://greatscottgadgets.com/hackrf/one/). Clients can impose still narrower RTL-specific limits. Access above that needs an extension, client changes, or a separately configured translation scheme. Standard rtl_tcp has no transmit path, capability negotiation, sample timestamps, or general acknowledgements. Host configuration should control HackRF-specific options. Unsupported RTL-only commands need a documented behavior and server-side logging.

**Streaming needs bounded resources.** Keep USB capture independent of socket writes, reuse buffers, bound the network queue, and handle slow/disconnected clients. For the first version, stop/disconnect on overrun rather than silently introducing an unmarked gap. Rate changes should restart the appropriate pipeline and discard stale buffers; frequency changes need documented settling behavior. Stop RX when a client leaves, then allow another connection without restarting the process. Bind to loopback by default and use an existing secure tunnel/VPN for remote access: plain rtl_tcp supplies no authentication or encryption.

Raw IQ payload bandwidth is two bytes per complex sample (before network overhead):

| Output rate | Payload bandwidth |
| --- | ---: |
| 2 MS/s | 32 Mbit/s |
| 2.048 MS/s | 32.768 Mbit/s |
| 2.4 MS/s | 38.4 Mbit/s |
| 8 MS/s | 128 Mbit/s |
| 20 MS/s | 320 Mbit/s |

These are arithmetic requirements, not LAN measurements. Full-rate output calls for suitable USB and network headroom; retaining the two-byte format avoids the fourfold expansion of complex float32 IQ.

## Suggested implementation sequence and remaining work

The investigation is complete. The production server has not been implemented. Effort is moderate for a reliable compatibility server; the basic USB question is resolved, but DSP and real-client behavior should drive estimates after an initial prototype.

| Stage | Concrete completion criterion | Owner |
| --- | --- | --- |
| 1. Compatibility prototype (recommended next action) | Rust server accepts one real rtl_tcp client, tunes the HackRF, converts signed IQ, delivers filtered 2 MS/s, and supports disconnect/reconnect. Pin exact client versions used for validation. | Codex, in the implementation task |
| 2. Rate and gain coverage | Qualify 2.048/2.4 MS/s and required lower rates, gain mapping, startup command sequences and rejected settings; verify DSP against synthetic tones and a known RF source. | Codex; user supplies preferred application names |
| 3. Robustness | Longer captures, sustained TCP throughput, bounded memory under a stalled reader, hot-unplug recovery, repeated retuning and reconnects, clean shutdown. | Codex |
| 4. Deployment | Windows executable plus interactive/Windows Service operation; test Linux interactive/systemd operation and provide Docker with documented USB passthrough. | Codex |
| 5. Distribution decision | Choose private GitLab or intentional public release, license, package/signing policy and release ownership. | User |

The most useful next milestone is a real-client demonstration of **HackRF at 8 MS/s -> filtered 2 MS/s -> standard rtl_tcp**, rather than extending firmware or designing a new network protocol. Exact client applications/versions remain unspecified; compatibility with every rtl_tcp implementation should not be assumed.
