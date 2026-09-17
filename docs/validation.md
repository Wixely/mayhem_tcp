# mayhem_tcp proof-of-concept validation

Date and review date: **2026-09-17**. Tested platform: Windows, Rust
1.98.1, MSVC target. Hardware reports HackRF One board ID 2, firmware v2.4.0,
USB API 0x0111, WinUSB driver. No firmware/driver changes were made.

## Offline checks

- Release tests: **7 passed**. Cover wire greeting, signed correction,
  invalid-setting rejection without mutation, rate/gain bounds, fragmented and
  coalesced commands including intervening read timeouts, command flooding and
  truncation, filter continuity, DC/IQ mapping and alias rejection.
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

**Recommended next action:** user chooses an rtl_tcp application and connects
it to `mayhem_tcp`; Codex handles compatibility findings and then extends
signal-quality/long-run testing. Production deployment verification follows.
