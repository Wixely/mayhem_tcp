Windows x64 proof-of-concept prerelease for HackRF / PortaPack in HackRF mode.

- Standard rtl_tcp receive streaming with filtered sample-rate conversion.
- Independent analog and digital IQ AGC, live gain control, and reconnect support.
- Native Windows executable with a statically linked C runtime; no libhackrf/libusb DLL required.
- Includes documentation, third-party license notices and a SHA-256 archive checksum.

Extract the ZIP and run `mayhem_tcp.exe --agc --digital-agc -p 12346`, then
connect an rtl_tcp client to `127.0.0.1:12346`. The existing WinUSB driver and
exclusive access to a HackRF in HackRF mode are required.

Tested with Mayhem v2.4.0 on Windows. This is receive-only experimental software,
not a production or calibrated RF release. Windows Service operation, Linux,
Docker, long-duration reception and broad GUI-client compatibility remain
unverified. See the included validation record for details. The executable is
unsigned. Hardware integration tests require a physical device and are not run
on the GitHub-hosted runner.
