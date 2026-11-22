# phonesvc — phone-servicing toolkit (Rust)

Independent phone-servicing app. Native Windows, no vendor SDKs, no
proprietary DLLs. Built on open, documented protocols only:

- **Qualcomm EDL (9008)** — Sahara loader upload + Firehose partition ops
- **GPT / MBR** partition parsing
- **Android sparse** image conversion
- **Samsung PIT** parsing
- **Qualcomm rawprogram\*.xml** parsing

## Files

| File               | Purpose                                                   |
|--------------------|-----------------------------------------------------------|
| `phonesvc.exe`     | **The app** — double-click to open the window (x64 GUI).  |
| `phonesvc-cli.exe` | Scriptable command-line version of the same toolkit.      |
| `libusb-1.0.dll`   | x64 libusb, loaded at runtime. Keep it next to the exes.  |

No installation. Keep the three files together in one folder.

## Using the app

Double-click `phonesvc.exe`. A window opens with five tabs:

- **Devices** — Refresh to list connected USB devices (bus, address, VID:PID,
  and an EDL badge for Qualcomm 9008).
- **Qualcomm EDL** — Detect EDL devices, and read the device's Sahara identity
  (put the phone into 9008 / EDL mode first).
- **Service** — Upload a Firehose programmer (`prog_firehose*.elf`) over Sahara,
  then run read / write / erase / peek / poke / reset and query storage info.
  Pick the programmer once, then each action re-runs the connect cycle.
- **Images** — Open a GPT disk image, a Samsung PIT dump, or a Qualcomm
  firmware folder (rawprogram\*.xml) to inspect partitions and entries.
- **Profiles** — Browse built-in device profiles (40+ across Qualcomm, Google,
  Samsung, Xiaomi, OnePlus, OPPO, vivo, Motorola, Lenovo, ASUS, Sony,
  Fairphone, and generic eMMC/UFS fallbacks).

A collapsible log at the bottom shows progress and any errors. Long operations
run in the background so the window stays responsive.

## Using the CLI

```
phonesvc-cli devices                 List connected USB devices
phonesvc-cli devices --vid 05c6      Filter by vendor id (hex)
phonesvc-cli qc detect               Detect Qualcomm EDL (9008) devices
phonesvc-cli qc info                 Read Sahara identity from a 9008 device
phonesvc-cli service --programmer <prog_firehose.elf> connect   Upload + configure
phonesvc-cli service --programmer <p.elf> read <start> <sectors> <out>
phonesvc-cli service --programmer <p.elf> write <file> <start> <sectors>
phonesvc-cli service --programmer <p.elf> erase <start> <sectors>
phonesvc-cli service --programmer <p.elf> peek <addr> <size>
phonesvc-cli service --programmer <p.elf> reset
phonesvc-cli service --programmer <p.elf> storage-info <drive>
phonesvc-cli partitions <image>      List GPT partitions from a disk image
phonesvc-cli pit <dump>              List partitions from a Samsung PIT dump
phonesvc-cli flash-raw <directory>   List entries from rawprogram*.xml firmware
phonesvc-cli profiles list           List built-in device profiles
phonesvc-cli profiles list-vendor <vendor>   Filter profiles by vendor
phonesvc-cli profiles show <name>    Show a profile's details
```

Sector numbers and addresses accept decimal or `0x`-prefixed hex.

## Requirements

- Windows x64.
- For raw-USB operations (EDL and flashing), a libusb-compatible driver bound
  to the device (e.g. Zadig / WinUSB). `Devices` and `qc detect` only need the
  DLL present.

## Notes

- Real-hardware flashing (Sahara/Firehose) is implemented but unproven against a
  physical device; protocol logic is verified with mock transports in the test
  suite.
- Sector sizes and partition layouts vary by model and firmware. The built-in
  profiles are starting points, not authoritative.