# Running mayhem_tcp

Reviewed: 2026-09-17. Windows interactive operation is hardware-tested.
Windows Service support is compiled but has not been installed/tested under
SCM. Linux/systemd and Docker files are provided but untested. There is no
Linux Rust toolchain or Docker executable available in the checked environment.

## Windows interactive

Build with `cargo build --release --locked`. Run `target\release\mayhem_tcp.exe`.
The MSVC build statically links the C runtime and requires no libhackrf/libusb
DLL. The existing WinUSB device driver is still required. No driver installation
is performed by this application. Select HackRF mode on the PortaPack and close
other applications using its USB interface. Stop with Ctrl+C.

Default listening address is `127.0.0.1:1234`. To accept LAN clients:

```powershell
.\target\release\mayhem_tcp.exe -a 0.0.0.0 -p 1234
```

Connect clients to the computer's LAN IP, not to `0.0.0.0`. Permit the chosen
TCP port through the host firewall when configuring LAN access. Nothing in this
project changes the firewall. The protocol has no authentication or encryption;
use a trusted LAN or an existing VPN/tunnel for remote access.

## Windows Service (optional, not installed)

Place the executable in a stable directory. In an elevated Windows PowerShell
session, adapt the path and binding before running:

```powershell
$serviceBinary = '"C:\Apps\mayhem_tcp\mayhem_tcp.exe" --service -a 127.0.0.1 -p 1234'
New-Service -Name mayhem_tcp -BinaryPathName $serviceBinary -StartupType Manual
Start-Service mayhem_tcp
Get-Service mayhem_tcp
Stop-Service mayhem_tcp
```

The `--service` mode must be launched by SCM and uses the service name
`mayhem_tcp`. It responds to stop/shutdown requests. Configure the account's USB
access as required on the target computer. This PoC writes diagnostics to
stderr, not the Windows Event Log; use interactive mode for troubleshooting.
Service installation, recovery policies and Event Log integration remain an
operational follow-up, not a verified deployment claim.

## Linux and systemd (untested)

Build with a Rust toolchain supporting edition 2024 (declared minimum 1.85).
Place the executable at `/usr/local/bin/mayhem_tcp`. Create a `hackrf` group and
a `mayhem-tcp` service account, and install
`deployment/99-mayhem-tcp.rules` into `/etc/udev/rules.d/`. Reload udev rules and
reconnect the device. The user/group names are deployment examples, not accounts
created by this repository.

For interactive operation, use a user with access to the USB device. Install
`deployment/mayhem_tcp.service` under `/etc/systemd/system/` for service mode,
then use `systemctl daemon-reload`, `systemctl start mayhem_tcp` and
`journalctl -u mayhem_tcp`. Adjust `ExecStart` to permit LAN clients if desired.
SIGTERM from systemd requests the same orderly shutdown as Ctrl+C.

## Docker on a Linux USB host (untested)

```text
docker build -t mayhem_tcp:poc .
docker run --rm --device=/dev/bus/usb/001/004 -p 1234:1234 mayhem_tcp:poc
```

Replace the example USB bus/device node with the attached HackRF's current
node. The container runs as root to access that explicitly passed USB device;
it does not need `--privileged`. Reconnection may change the device node and
require restarting with an updated mapping. Base images are version-tagged,
not digest-pinned; pin verified digests before a reproducible release.

Docker Desktop on Windows does not automatically expose a Windows WinUSB
device to a Linux container. Use the native Windows executable here; Linux
USB passthrough would require a separately configured environment.
