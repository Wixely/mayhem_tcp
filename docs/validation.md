# mayhem_tcp proof-of-concept validation

Date and review date: **2026-09-17**. Tested platform: Windows, Rust
1.98.1, MSVC target. Hardware reports HackRF One board ID 2, firmware v2.4.0,
USB API 0x0111, WinUSB driver. No firmware/driver changes were made.

## Offline checks

Android theme update (2026-09-17): ARM64/x64 APK builds passed with zero warnings
or errors, and the artifact scan passed. On the Android 16 x64 emulator, inspected
the light and dark main screens, dark Radio dialog and light About dialog.
Switching system night mode light → dark → light preserved an unsaved main-screen
buffer-count edit and AGC checkbox state. Native resource variants select the
theme; no AndroidX dependency or manual appearance setting was added. A live
HackRF stream during a theme switch still needs physical-phone verification.

- Release tests: **13 passed** after the analog AGC addition. Cover wire greeting, signed correction,
  invalid-setting rejection without mutation, rate/gain bounds, fragmented and
  coalesced commands including intervening read timeouts, command flooding and
  truncation, filter continuity, DC/IQ mapping and alias rejection.
- AGC coverage includes fast overload reduction, delayed recovery, USB-queue
  settling, level deadband, constant/DC input, sparse weak input below one ADC
  count RMS, gain limits, stage-write ordering, synthetic closed-loop
  convergence, independent digital/analog commands and manual-gain restoration.
- DSP synthetic tests use decimation factors 4, 8 and 32. A 100-count complex
  tone in the passband produces expected RMS between 69 and 72 counts.
  Tested out-of-band tones produce RMS below 0.5 output counts after startup.
  This checks software behavior, not calibrated RF response.
- `cargo clippy --all-targets --locked -- -D warnings`: passed.
- Debug/release builds and formatting checks: passed.

## Automated live TCP/USB test

Command:

```powershell
cargo test --release --locked --test hardware -- --ignored --nocapture
```

One test passed in approximately 28 seconds. A real HackRF fed the server and a
Rust TCP client read the legacy stream. Measurements used three-second windows
after discarding initial buffered data:

| Requested output | Observed output (S/s) | USB capture setting |
| --- | ---: | ---: |
| 2,000,000 | 2,000,396 | 8,000,000 |
| 2,048,000 | 2,048,016 | 8,192,000 |
| 2,400,000 | 2,399,980 | 9,600,000 |
| 250,000 | 250,039 | 8,000,000 |
| 2,048,000 after reconnect | 2,047,999 | 8,192,000 |
| 2,000,000 after failure recovery | 1,999,893 | 8,000,000 |

Small differences are host measurement/buffering effects, not oscillator
calibration results. Nonconstant sample bytes were observed. Tuning to
100/101 MHz and indexed manual gain changes were accepted by the hardware.

The test also verified a command fragmented across a 200 ms read timeout,
multiple commands in one write, a rate-zero command causing disconnect,
a client that stopped reading causing bounded-queue/timeout disconnect, and
successful reconnection afterward. After five sessions the server shut down
normally. A Windows accepted-socket nonblocking-mode inheritance issue was
found during testing, corrected, and this test rerun successfully.

## Unmodified application interoperability

Used the official **rtl_433 25.12 Windows MSVC x64 release**, executable
`rtl_433-rtlsdr.exe`; no client modifications or Soapy integration. Archive
provenance and SHA-256 are recorded in [dependencies.md](dependencies.md).

First run: `rtl_tcp:127.0.0.1:12347`, 100 MHz, 250 kS/s, manual gain 32 dB,
approximately eight seconds. Exit code 0. Server processed 4,202,496 output
bytes and ended/reopened cleanly.

Final statically linked Windows executable was then tested at 433.92 MHz,
2.048 MS/s, gain 40 dB with this command (server uses `--sessions 1`):

```powershell
.\.local\rtl_433\rtl_433-rtlsdr.exe -c NUL -d rtl_tcp:127.0.0.1:12347 -f 433920000 -s 2048000 -g 40 -R 0 -F log -T 8 -v
```

The client reported an R820T greeting, sample rate 2,048,000 S/s, async sample
reception and tuning to 433.920 MHz. It exited with code 0 at its time limit,
and the server closed the session and stopped. The exact time is determined by
rtl_433's timer and need not be eight precise wall-clock seconds.

All device decoders were disabled (`-R 0`); this proves protocol/stream
interoperability, **not successful decoding of an RF transmission**. No IQ
captures or decoded device identifiers were saved. Only short localhost
sessions were measured, not LAN throughput.

## Limits and follow-up

### Analog AGC addition

The complete hardware test suite passed: **2 tests**, approximately 38 seconds,
including the previous rate/reconnect/backpressure checks and a new live
AGC/manual-transition test. After refining the weak-input threshold, the
AGC-specific hardware test was rerun and passed in approximately 10 seconds.
It received 2,048,068 and 2,047,707 S/s in two nominal 2.048 MS/s measurement
windows, before/after retuning. Host timing accounts for small differences.

The test logs confirmed actual automatic LNA/VGA increases, restoration of a
remembered 46 dB manual request as LNA 40/VGA 6, a subsequent manual 24 dB
request as LNA 24/VGA 0, and return to auto. It asserts exactly two full
configurations: initial startup and retuning. Gain and mode updates did not
restart RX. The two opt-in tests serialize their exclusive use of the hardware.

Unmodified rtl_433 25.12 also ran with its default automatic tuner gain:

```powershell
.\target\release\mayhem_tcp.exe -p 12347 --sessions 1 --agc
# In another terminal:
.\.local\rtl_433\rtl_433-rtlsdr.exe -c NUL -d rtl_tcp:127.0.0.1:12347 -f 100000000 -s 2048000 -R 0 -F log -T 25 -v
```

The client reported `Tuner gain set to Auto` and exited with code 0 at its time
limit. The server streamed 98,631,680 output bytes over approximately 24.1 s,
with only the startup configuration. Ambient input prompted gradual gain
increases from LNA 16/VGA 16 to LNA 40/VGA 42; the last adjustment reported RMS
9.77 counts and peak 62. This demonstrates live automatic control and continued
streaming, not calibrated gain accuracy or controlled strong-signal recovery.
The RF amplifier and antenna power remained off. All device decoders were
disabled, and no IQ capture was saved.

### Digital AGC addition (2026-09-17)

All **17 offline tests** passed, along with Clippy (`-D warnings`), formatting,
and debug/release builds. Added coverage checks rate-independent gain recovery
at 250 kS/s, 2.048 MS/s and 3.2 MS/s, sudden overload at maximum amplification,
I/Q phase preservation, silence/DC behavior, chunk-boundary equivalence and
amplification before final quantization. Protocol tests check independent modes
and rejection of invalid digital-AGC values without mutating settings.

The live AGC test now enables digital AGC alongside analog AGC, switches to
manual gain, disables/re-enables digital AGC and retunes with both enabled.
It passed on firmware v2.4.0, measuring 2,047,956 and 2,047,546 S/s at the
requested 2.048 MS/s. Logs confirmed digital transitions and exactly two RX
configurations (startup and retune), with no stream errors. Controlled digital
level behavior is covered synthetically; ambient RF is not a calibrated source.
The full two-test hardware suite also passed in 38.55 seconds, including rate,
reconnect, invalid-command and stalled-reader regression checks.

### Remaining verification

The subsequent Osmocom parity update is covered separately in
[the parity matrix](osmocom-parity.md). Its 24 offline tests, four Android-native
emulator tests, desktop/Android Clippy and ARM64/x64 APK builds pass. New Radio
dialog rendering, save/reopen behavior and antenna-power opt-in validation were
checked on the Android 16 emulator. Offset recentering/DC rejection, test-counter
continuity, PPM clock arithmetic and USB-depth-aware AGC settling have synthetic
coverage. These new hardware paths are not qualified by the older streaming
measurements below. Remaining Android reliability tests are deferred at the
user's request.

On 2026-09-17 the updated APK passed a physical-phone LAN parity test. For
225001/240000/2048000/3200000 S/s requests, baseline measurements were
224841/240248/2047415/3199099 S/s, and offset-enabled measurements were
224963/239778/2047840/3198409 S/s (about three seconds each). PPM settings
+20/-20/+1000/-1000/0 all sustained reception at requested 2.048 MS/s; these
short host-timed measurements are not oscillator calibration. Test mode delivered
40,958,704 consecutive bytes over ten seconds without a counter discontinuity.
Invalid test/offset mode, PPM and rate settings left the counter running. Turning
test mode off restored ordinary IQ; reconnecting worked. Antenna power was
explicitly disabled. Calibrated clock accuracy remains unverified.

A subsequent LAN run on 2026-09-17 passed with the user confirming readiness
after instructions to select 8 USB buffers and retain 64 output queue blocks.
The USB count is a host startup setting and cannot be read back over rtl_tcp;
the phone configuration was not independently inspected through ADB.
Normal reception at 225001/240000/2048000/3200000 S/s measured
224796/239596/2049110/3198777 S/s; offset-enabled reception measured
224757/239850/2046813/3198050 S/s. All were within the test's 3% tolerance.
PPM +20/-20/+1000/-1000/0 transitions, invalid-command handling, return from
test mode to normal IQ and reconnecting passed. The software test counter
delivered 40,960,000 consecutive bytes over ten seconds without discontinuity;
this does not establish lossless hardware sampling before counter generation.
Six ten-second ordinary-IQ windows at 3.2 MS/s, with both AGCs enabled, measured
3,198,509–3,201,088 S/s without disconnecting. Antenna power was explicitly off,
and the test client disconnected afterward. This qualifies the reported
8-buffer configuration for this short run; other non-default USB depths and
long-duration behavior remain untested.

An ambient-spectrum test near the user-reported TETRA band used fixed 40 dB
manual gain, both AGCs off, zero PPM, 2.048 MS/s and antenna power off. Each
capture averaged 512 Hann-windowed 4096-point FFTs (500 Hz bins) after two seconds
of draining. At 391 MHz centre, normal/offset/normal centre-bin levels were
-32.0/-91.8/-32.1 dBFS; at 391.25 MHz, they were -32.0/-86.8/-31.8 dBFS.
A peak stayed near absolute 391.0305–391.031 MHz across both capture centres
and modes, supporting physical recentering and centre-spike suppression of
approximately 55–60 dB in this setup. No output rail clipping was observed.
Signal strengths varied between sequential captures. Carrier identity, exact
frequency and modulation were not established or decoded; no calibrated PPM
or RF-amplitude accuracy claim follows. Only averaged spectra were saved locally,
not raw IQ. The final request was 391.25 MHz, offset off, followed by disconnect.

Compatibility update reviewed on 2026-09-17: all 19 offline tests pass, including
64× decimation passband/alias rejection and fragmented-stream equivalence for
240 kS/s, queue-capacity bounds, and invalid-setting rejection without mutation.
Desktop and Android native Clippy and formatting checks passed. The ARM64 APK
built without warnings/errors and passed the artifact scan. CLI help and rejection
of a zero queue capacity were checked. The live regression suite now checks 240
kS/s, a 64-block queue, and continued streaming after invalid commands; it has
not been rerun against desktop hardware for this update.

The updated ARM64 APK subsequently passed a remote LAN test on the physical
Android phone with the user-confirmed default 32-block queue. Measured rates
were 239,955 / 249,930 / 2,002,325 / 2,047,363 / 2,400,857 / 3,198,829 samples/s
for requests of 240/250 kS/s and 2/2.048/2.4/3.2 MS/s, each measured for about
four seconds after draining startup data. Gain/AGC toggles, 100/101 MHz tuning,
fragmented/coalesced commands, three reconnects, invalid rate/gain-mode/digital
AGC/gain-index commands followed by valid 240 kS/s, stalled-reader cleanup,
and concurrent-client rejection all passed. Six consecutive ten-second windows
at 2.048 MS/s measured 2,047,559–2,048,819 samples/s without a disconnect.
This verifies the updated native-call path at 32 blocks.

A repeat on 2026-09-17 with the user reporting 64 buffer blocks and the app
in the background passed the same full remote suite. Rate-sweep measurements
were 240,039 / 250,009 / 2,002,028 / 2,047,647 / 2,399,295 / 3,199,254 samples/s.
Six consecutive ten-second windows at 2.048 MS/s measured 2,047,935–2,049,532
samples/s without a disconnect. Three reconnects, invalid-command recovery,
stalled-reader cleanup and concurrent-client rejection passed. The client was
disconnected afterward. Screen state and battery policy were not recorded;
that run alone does not establish screen-off reliability.

The following user-coordinated lock-screen run on 2026-09-17 passed eighteen
ten-second measurements at 2.048 MS/s (2,046,464–2,049,567 samples/s). The same
64-block server remained connected for over three minutes, including short
drain intervals between measurements. After disconnecting, a fresh client
received the RTL0 header and streamed for five seconds at 2,048,291 samples/s.
The test client then disconnected normally. Lock state was not independently
queried via ADB. This qualifies a short user-coordinated locked-screen session;
Doze, overnight reliability, battery policy and preference persistence remain
unverified.

- Across desktop and Android, 240/250 kS/s and 2/2.048/2.4/3.2 MS/s were hardware-measured. Other
  rates in the allowed range have rate-planning coverage but need hardware and
  client qualification.
- No SDR#/SDR++ GUI session, known-signal RF comparison, long-duration loss
  measurement, physical unplug/replug, or calibrated PPM test was performed.
- The build includes Windows Service support; SCM installation and stop/start
  remain untested. Nothing was installed as a service or exposed through the
  firewall.
- Linux/systemd, Docker and interactive VS Code debugging remain untested.
  WSL was checked: no Rust toolchain was available; no Linux tooling was installed.
- No RF transmit path exists. No firmware or flash-writing API was used.
- Controlled RF overload/burst recovery remains unverified on hardware;
  synthetic tests cover the controller's response. **Owner: Codex**, once a
  suitable controlled signal source/attenuator is available. **User:** try the
  client's tuner AGC option and report compatibility or reception issues.

**Recommended next action:** user chooses an rtl_tcp application and connects
it to `mayhem_tcp`; Codex handles compatibility findings and then extends
signal-quality/long-run testing. Production deployment verification follows.
