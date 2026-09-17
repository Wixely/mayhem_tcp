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
| 0x02 sample rate | Output rates 240,000 through 3,200,000 S/s. Hardware is set to a power-of-two multiple at least 8 MS/s and below 16 MS/s. At 240 kS/s, capture is 15.36 MS/s with 64× decimation. Rates are nominal; clock accuracy is not calibrated. |
| 0x03 tuner gain mode | 0 enables host-controlled analog AGC; 1 selects manual gain. Other values are ignored. |
| 0x04 tuner gain | Signed tenths of a dB, 0 through 1020; mapped to LNA/VGA below. |
| 0x05 PPM | Signed -1000..1000. Tuning request = nominal frequency / (1 + ppm / 1e6). Does not correct ADC timing. |
| 0x08 digital AGC | 0 disables, 1 enables software digital IQ AGC independently of analog gain. Other values are ignored. |
| 0x0d gain index | Index 0..28 into the R820T gain table. |
| 0x0e antenna power | 0 disables; 1 enables only with server option `--allow-bias-tee`. Disabled again when the session closes. |
| Other commands | Ignored and logged, including IF-stage gain, test mode, direct sampling, offset tuning and crystal settings. |

Invalid values for implemented commands are logged and ignored without changing
the settings or disconnecting. Later valid commands are still processed. USB
errors, truncated packets and command-queue flooding can still end the session.
Unknown commands do not modify the hardware. AGC/unsupported
command messages go to host stderr only. The RF amplifier is always disabled.
There is no transmit, flash, reboot, register-write or Mayhem UI API exposed.

Gain requests round to the nearest 2 dB, with halfway values rounding upward.
LNA takes the largest multiple of 8 dB no greater than the total, capped at
40 dB. VGA supplies the remainder, up to 62 dB. For example, 32 dB maps to
LNA 32/VGA 0; 49.6 dB maps to LNA 40/VGA 10. The default is 32 dB. The emulated
R820T table limits indexed/client-UI gain to 49.6 dB; direct gain requests or
`-g` can access the full 102 dB combined range. This mapping does not reproduce
an R820T's RF characteristics. While automatic gain is active, gain/index
commands remember a manual setting but do not override the controller. Returning
to manual applies that remembered setting. Startup remains manual unless
`--agc` is provided; the client can change either mode with command 0x03.

## Digital AGC

Digital AGC defaults off; `--digital-agc` enables it initially. Client command
0x08 overrides this. It runs after decimation on floating-point IQ, before
unsigned 8-bit quantization, and does not write hardware gain or restart RX.
One positive multiplier is applied to both I and Q, preserving phase.

The complex-magnitude peak envelope attacks immediately and decays exponentially
with a 200 ms time constant. Its target is 64 signed sample counts (about 6 dB
below component full scale), with amplification capped at 64x (about 36 dB).
It starts at unity gain and holds its envelope during exact-zero samples.
DC contributes to the magnitude, preventing a DC offset from overflowing output.
Gain recovery depends on sample rate, not USB block boundaries. Enabling it or
reconfiguring the receiver resets its envelope; disabling bypasses it completely.

This is a software approximation, not the RTL2832 chip's algorithm or audio AGC.
It scales noise along with signals, can change amplitude modulation, and cannot
repair ADC clipping. Analog AGC continues to meter raw input independently.

## Analog AGC

This is a host control loop, not RTL hardware AGC or demodulated-audio AGC.
Raw signed I/Q is measured before filtering and decimation. Over 20 ms windows
it measures per-component RMS with the mean/DC power subtracted, absolute raw
peak and the fraction of components at magnitude 120 or above. DC subtraction
is only for metering; it does not modify the transmitted IQ.

Initial automatic gain uses the last manual total, split approximately equally
between LNA/VGA (32 dB becomes 16/16). Automatic totals are bounded to 0..102 dB,
with hardware-valid LNA steps of 8 dB and VGA steps of 2 dB. The manual mapping
above is preserved. When redistributing gain, decreases are sent before increases
to avoid an unnecessary transient increase in total gain. The RF amplifier
remains off; AGC does not change antenna power.

The initial controller policy is:

- Peak >=126, more than 0.01% of components at magnitude >=120, or RMS >32:
  lower total gain by 8 dB.
- RMS >24: lower total gain by 2 dB.
- RMS 12..24: hold gain. Peak >=80 also inhibits gain increases.
- RMS 0.01..<12 sustained for 500 ms: raise gain by 2 dB, provided the hang period
  has expired. Constant/DC-only input below 0.01 RMS does not turn gain up; weak
  quantized signals below one ADC count can still increase gain.
- Healthy/high levels or peaks >=80 delay increases for two seconds. Reductions
  remain allowed during this hang period.
- After enabling AGC or changing gain, ignore enough samples to drain the
  16 x 256 KiB USB queue, plus 20 ms settling. At 8 MS/s this is approximately
  282 ms. Samples continue streaming throughout; only AGC metering is paused.

These thresholds are engineering defaults tested with synthetic inputs and
short live sessions, not a calibrated guarantee for every modulation or RF
environment. Noise can still raise gain; long inter-burst gaps can cause it to
rise. Strong signals anywhere in the captured band influence analog gain, even
if removed from the output by decimation. ADC statistics cannot detect every
kind of distortion occurring earlier in the analog stages. Use manual gain
for measurements requiring fixed amplitude or when automatic behavior is unsuitable.

## Streaming and transitions

The decimator uses a unity-DC-gain Blackman-windowed sinc filter with
`64 * divisor + 1` taps and cutoff at 0.4 times output sample rate. Its transition
occupies the spectrum near output Nyquist; the whole displayed bandwidth is
not a flat passband. It evaluates symmetric taps only at retained samples.
State and decimation phase persist between USB blocks. Filtered values are
rounded/clipped into signed 8-bit range, then biased to unsigned bytes.

Frequency, sample rate, PPM or antenna-power changes stop RX, retire its USB
queue, reconfigure, and reset DSP/AGC history. Automatic mode reseeds from the
remembered manual total on restart. The first new USB transfer is discarded for settling.
Queued output from previous configurations is skipped, but bytes already
written into TCP cannot be withdrawn. The client may briefly see old data or
a gap around these changes. Gain and auto/manual updates use live USB controls
and keep RX, the USB queue and DSP history running. Previously captured IQ
and brief analog settling effects can still appear around a live gain change.
There is no sample-accurate transition
marker in rtl_tcp.

The USB queue uses 16 x 256 KiB transfers; the output queue defaults to 32
blocks (approximately 2 MiB at decimation 4), plus a writer's current block
and OS socket buffers. Socket writes have a 2-second timeout. A full queue or
write failure terminates the stream instead of silently discarding samples.
USB transfer errors also terminate the session. This does not detect every
possible loss inside the hardware: raw HackRF samples have no sequence IDs.

The output queue is configurable with `-n BLOCKS` / `--queue-blocks BLOCKS`
or Android's **Buffer blocks** field, from 1 through 1024. This is a capacity,
not a target: the server sends each block as soon as possible. Each block is
approximately 256 KiB divided by the decimation factor. At 2.048 MS/s, 32
blocks hold about 0.512 seconds of IQ; 64 hold about 1.024 seconds. Larger
queues increase possible backlog and memory use; they do not fix insufficient
sustained throughput or override the 2-second socket-write timeout. The USB
queue remains fixed, and no samples are deliberately dropped to make room.

One client owns the device. Concurrent connections are closed immediately.
Every later accepted session reopens the device with startup defaults. Ctrl+C
and service stop signals request shutdown; an in-progress USB operation may
take up to its timeout to finish. Ordinary error cleanup stops RX and antenna
power; it does not restore the previous tuning, gain or sample rate.

Protocol references: [rtl_tcp](https://github.com/osmocom/rtl-sdr/blob/master/src/rtl_tcp.c),
[RTL gain table/clock correction](https://github.com/osmocom/rtl-sdr/blob/master/src/librtlsdr.c),
[Mayhem 2.4 HackRF control implementation](https://github.com/portapack-mayhem/hackrf/blob/38e082b9399fae30241a6ca5a1e1be0e157115de/firmware/hackrf_usb/usb_api_transceiver.c).

Gain-stage reference: [HackRF RX gain guidance](https://hackrf.readthedocs.io/en/latest/setting_gain.html).
