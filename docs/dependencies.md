# Dependency and reference provenance

Reviewed: 2026-09-17. Direct Cargo dependencies are pinned to exact versions;
Cargo.lock pins their resolution. No Python or Node.js tooling is used.

| Dependency | Version | Upstream | Declared license |
| --- | --- | --- | --- |
| nusb | 0.2.7 | https://github.com/kevinmehall/nusb | MIT OR Apache-2.0 |
| ctrlc | 3.5.2 | https://github.com/Detegr/rust-ctrlc | MIT OR Apache-2.0 |
| windows-service (Windows only) | 0.8.1 | https://github.com/mullvad/windows-service-rs | MIT OR Apache-2.0 |

Updates: review release notes and API changes, update the exact manifest
versions, regenerate Cargo.lock, and repeat offline tests plus the opt-in USB
integration test. The Windows release packaging script includes license/notice
files for resolved Windows dependencies and Rust runtime notices; review these
when updating dependencies. Source references informed the USB/wire protocol; the old
GPL hackrf_tcp implementation was not copied into this project.

An unmodified native rtl_433 test client was downloaded into ignored `.local/`:

- Project/release: https://github.com/merbanan/rtl_433/releases/tag/25.12
- Archive: `rtl_433-win-msvc-x64-25.12.zip`
- URL: https://github.com/merbanan/rtl_433/releases/download/25.12/rtl_433-win-msvc-x64-25.12.zip
- Observed SHA-256: `088A00AA5446C8F859346A93320FB2AE353B7689A1C53B4A5BB20DF0FF1C1151`
- Tested executable: `rtl_433-rtlsdr.exe`, reports version 25.12, inputs file/rtl_tcp/RTL-SDR.
- The hash records this download; it is not a signature-verification claim.
- It is a test-only external application, not linked, installed globally, or
  included in the mayhem_tcp executable. The archive also contains other
  variants/DLLs, which were not used to add integrations.
