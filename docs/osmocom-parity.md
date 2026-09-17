# Osmocom rtl_tcp parity

Reviewed: 2026-09-17. Baseline: upstream Osmocom rtl-sdr, not extended forks.
This is protocol compatibility on HackRF, not RTL2832/tuner hardware emulation.

| Command | mayhem_tcp behavior |
| --- | --- |
| 0x01 tune | Implemented; 1 MHz through the protocol's u32 frequency limit. |
| 0x02 rate | Implemented; 225001–3200000 S/s. Also allows the gap between 300 and 900 kS/s that RTL rejects. |
| 0x03 gain mode | Analog AGC or manual. Host AGC differs from RTL tuner AGC. |
| 0x04 gain | Maps total dB to HackRF LNA/VGA. |
| 0x05 PPM | Corrects tuning and sample clock, assuming a common reference error. |
| 0x06 IF gain | Logged no-op: RTL tuner stages have no faithful HackRF mapping. |
| 0x07 test mode | Hardware-paced software byte counter, wrapping 255 to 0. Checks TCP, not hardware sample loss. |
| 0x08 digital AGC | Software IQ AGC, not the RTL2832 algorithm. |
| 0x09 direct sampling | Logged no-op: HackRF lacks RTL ADC bypass modes. |
| 0x0a offset tuning | Tune Fs/4 above the requested centre, shift digitally by +Fs/4 before filtering. |
| 0x0b RTL crystal | Logged no-op; use PPM correction. |
| 0x0c tuner crystal | Logged no-op; use PPM correction. |
| 0x0d gain index | R820T compatibility table mapped to HackRF gains. |
| 0x0e bias tee | Implemented after host opt-in, including Android Radio settings. |

Desktop accepts `-a`, `-p`, `-f`, `-g`, `-s`, `-b`, `-n`, `-d`, `-P`, `-T`.
Frequency/rate accept k/M/G suffixes. `-b 0` selects the default 16 USB transfers;
other USB counts must be 1–64. `-d` selects an enumeration index or an exact serial;
use `--serial` for a digits-only serial. No selector still requires exactly one
HackRF. `-T` explicitly enables startup antenna power and subsequent client control.
`-D` returns an explanatory error instead of pretending to enable direct sampling.
`--offset-tuning` and `--test-mode` select those initial modes.

Android's Radio dialog provides initial frequency/rate/manual gain, PPM, USB
buffers, offset/test modes and antenna-power opt-in. Main-screen checkboxes
still select analog/digital AGC. Device selection remains one attached HackRF,
and listening uses IPv4 localhost or all interfaces. Desktop accepts numeric
IPv4/IPv6 bind addresses, not hostnames. IPv6 runtime behavior is unverified.

Deliberate differences: desktop defaults to manual 32 dB, and `-g 0` means
manual zero gain rather than automatic. Use `--agc` for automatic. Output queues
remain bounded (1–1024 blocks, default 32); `-n 0` is rejected. Full queues
disconnect rather than discarding IQ. Sessions reset to host startup settings
rather than inheriting the previous client's changes. Hardware errors can still
disconnect even though invalid protocol settings are ignored.

Validation: 24 offline tests and four native tests inside an Android 16 emulator
pass. Offset recentering/DC rejection, counter continuity, clock arithmetic,
CLI validation and USB-depth-dependent AGC settling have synthetic coverage.
The Radio dialog was visually inspected; antenna-power validation and saving/
reopening offset settings passed in the emulator. ARM64/x64 APK builds passed.
The updated APK subsequently passed a physical-phone LAN regression on
2026-09-17: normal and offset-enabled reception at 225001/240000/2048000/3200000
S/s, streaming at PPM +20/-20/+1000/-1000/0, and 40,958,704 consecutive test-counter
bytes without a discontinuity. Invalid mode/PPM/rate commands preserved the test
stream, disabling test mode restored ordinary IQ, and a fresh connection worked.
This checks streaming and mode transitions, not calibrated clock correction.
An additional ambient-RF comparison near the user-reported TETRA band at 391 MHz
used 2.048 MS/s, fixed 40 dB manual gain and both AGCs off. With 500 Hz FFT bins,
a peak near 391.0305–391.031 MHz stayed at the same absolute frequency when the
capture centre moved from 391 to 391.25 MHz and offset tuning was toggled.
The centre-bin power decreased by approximately 55–60 dB with offset enabled.
This supports physical recentering/DC suppression; the carrier's exact frequency
and modulation were not established, so it is not a calibrated reference.
A subsequent run with the user-reported 8 USB buffers and 64 output queue blocks
passed the same rate/mode checks, 40,960,000 continuous test-counter bytes,
reconnecting and six ten-second ordinary-IQ windows at 3.2 MS/s
(3,198,509–3,201,088 S/s). USB depth cannot be read back over rtl_tcp;
configuration depended on the user's setup confirmation. Other non-default
depths remain untested, and counter continuity does not prove lossless USB IQ.
Antenna power was explicitly disabled by the test and was not enabled on hardware.

Deferred at user request: remaining Android reliability tests (notification Stop,
USB unplug/replug, longer locked-screen/Doze sessions and physical-device settings
persistence). Earlier successful LAN/background/short locked-screen runs remain
recorded in the Android README.

Calibrated frequency/sample-clock verification is also deferred at the user's
request (2026-09-17). The short non-default USB-depth check is complete.

**Next action — Codex:** extend repeatable protocol regression coverage to all
14 command IDs, including documented no-ops and gain-index boundaries, then
exercise an unmodified rtl_tcp client against the Android server. **User:**
choose the intended GUI client for subsequent interactive qualification.
Optional closer behavioral parity includes startup AGC/CLI gain-zero semantics,
session-setting persistence and hostname binding; these are separate choices,
not missing radio commands. Retain bounded queues and document overflow policy.

References: [Osmocom command handler and CLI](https://github.com/osmocom/rtl-sdr/blob/master/src/rtl_tcp.c),
[Osmocom clock/rate implementation](https://github.com/osmocom/rtl-sdr/blob/master/src/librtlsdr.c),
[Mayhem HackRF USB controls](https://github.com/portapack-mayhem/hackrf/blob/38e082b9399fae30241a6ca5a1e1be0e157115de/firmware/hackrf_usb/usb_api_transceiver.c).
