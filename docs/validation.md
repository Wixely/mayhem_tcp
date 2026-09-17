# mayhem_tcp proof-of-concept validation

Date and review date: **2026-09-17**. Tested platform: Windows, Rust
1.98.1, MSVC target. Hardware reports HackRF One board ID 2, firmware v2.4.0,
USB API 0x0111, WinUSB driver. No firmware/driver changes were made.

## Offline checks

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

Compatibility update reviewed on 2026-09-17: all 19 offline tests pass, including
64× decimation passband/alias rejection and fragmented-stream equivalence for
240 kS/s, queue-capacity bounds, and invalid-setting rejection without mutation.
Desktop and Android native Clippy and formatting checks passed. The ARM64 APK
built without warnings/errors and passed the artifact scan. CLI help and rejection
of a zero queue capacity were checked. The live regression suite now checks 240
kS/s, a 64-block queue, and continued streaming after invalid commands; it has
not been rerun against hardware for this update. Android buffer persistence and
the updated native-call arguments need a physical-device run with this APK.

- Only 250 kS/s, 2 MS/s, 2.048 MS/s and 2.4 MS/s were hardware-measured. Other
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
