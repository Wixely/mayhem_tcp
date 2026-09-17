# rtl_tcp compatibility profile

Reviewed: 2026-09-17. This is a receive-only proof of concept.

The server advertises the R820T tuner ID (5) and 29 gain entries so clients can
use their existing tuner UI. This is **emulation**, not a claim that the device
is an RTL-SDR. The hardware is a HackRF One running in HackRF mode. Clients do
not require a custom plugin or a new greeting.

After the 12-byte `RTL0` greeting, the server sends unsigned 8-bit interleaved
I,Q samples, biased by 128. Commands are one opcode byte and a big-endian u32.
No acknowledgements, errors, metadata or other text are inserted into IQ.
TCP fragmentation and coalescing are supported, including timeouts between
command bytes. Commands are processed between USB transfers.

| Command | Behavior |
| --- | --- |
| 0x01 frequency | 1 MHz through 4,294,967,295 Hz; a client's UI may impose narrower limits. |
| 0x02 sample rate | Output rates 250,000 through 3,200,000 S/s. Hardware is set to a power-of-two multiple at least 8 MS/s and below 16 MS/s. Rates are nominal; clock accuracy is not calibrated. |
| 0x03 tuner gain mode | Accepted but manual gain remains active. No hardware/software AGC emulation. |
| 0x04 tuner gain | Signed tenths of a dB, 0 through 1020; mapped to LNA/VGA below. |
| 0x05 PPM | Signed -1000..1000. Tuning request = nominal frequency / (1 + ppm / 1e6). Does not correct ADC timing. |
| 0x08 digital AGC | Accepted but ignored; manual gain remains active. |
| 0x0d gain index | Index 0..28 into the R820T gain table. |
| 0x0e antenna power | 0 disables; 1 enables only with server option `--allow-bias-tee`. Disabled again when the session closes. |
| Other commands | Ignored and logged, including IF-stage gain, test mode, direct sampling, offset tuning and crystal settings. |

Invalid values for implemented commands terminate the session and are logged
on the host. Unknown commands do not modify the hardware. AGC/unsupported
command messages go to host stderr only. The RF amplifier is always disabled.
There is no transmit, flash, reboot, register-write or Mayhem UI API exposed.

Gain requests round to the nearest 2 dB, with halfway values rounding upward.
LNA takes the largest multiple of 8 dB no greater than the total, capped at
40 dB. VGA supplies the remainder, up to 62 dB. For example, 32 dB maps to
LNA 32/VGA 0; 49.6 dB maps to LNA 40/VGA 10. The default is 32 dB. The emulated
R820T table limits indexed/client-UI gain to 49.6 dB; direct gain requests or
`-g` can access the full 102 dB combined range. This mapping does not reproduce
an R820T's RF characteristics. Select manual gain in your client.

The decimator uses a unity-DC-gain Blackman-windowed sinc filter with
`64 * divisor + 1` taps and cutoff at 0.4 times output sample rate. Its transition
occupies the spectrum near output Nyquist; the whole displayed bandwidth is
not a flat passband. It evaluates symmetric taps only at retained samples.
State and decimation phase persist between USB blocks. Filtered values are
rounded/clipped into signed 8-bit range, then biased to unsigned bytes.

Every effective setting change stops RX, retires its USB queue, reconfigures,
and resets the DSP. The first new USB transfer is discarded for settling.
Queued output from previous configurations is skipped, but bytes already
written into TCP cannot be withdrawn. The client may briefly see old data or
a gap around tuning/rate/gain changes. There is no sample-accurate transition
marker in rtl_tcp.

The USB queue uses 16 x 256 KiB transfers; the output queue holds at most 32
blocks (approximately 2 MiB at decimation 4), plus a writer's current block
and OS socket buffers. Socket writes have a 2-second timeout. A full queue or
write failure terminates the stream instead of silently discarding samples.
USB transfer errors also terminate the session. This does not detect every
possible loss inside the hardware: raw HackRF samples have no sequence IDs.

One client owns the device. Concurrent connections are closed immediately.
Every later accepted session reopens the device with startup defaults. Ctrl+C
and service stop signals request shutdown; an in-progress USB operation may
take up to its timeout to finish. Ordinary error cleanup stops RX and antenna
power; it does not restore the previous tuning, gain or sample rate.

Protocol references: [rtl_tcp](https://github.com/osmocom/rtl-sdr/blob/master/src/rtl_tcp.c),
[RTL gain table/clock correction](https://github.com/osmocom/rtl-sdr/blob/master/src/librtlsdr.c),
[Mayhem 2.4 HackRF control implementation](https://github.com/portapack-mayhem/hackrf/blob/38e082b9399fae30241a6ca5a1e1be0e157115de/firmware/hackrf_usb/usb_api_transceiver.c).
