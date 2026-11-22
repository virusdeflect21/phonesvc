# phonesvc

Independent phone-servicing toolkit for Windows — **Qualcomm EDL (9008)** and
related partition operations, built entirely on open, documented protocols.

No vendor SDKs. No proprietary DLLs. Pure Rust: a shared library, a native
windowed GUI app, and a scriptable CLI.

> **Status:** protocol logic (Sahara / Firehose / GPT / sparse / PIT /
> rawprogram) is implemented and verified with mock transports in the test
> suite. Real-hardware flashing against a physical 9008 device has **not** been
> proven yet. Treat sector sizes, partition layouts, and device profiles as
> starting points, not authoritative data.

## What it does

- **USB enumeration** over a runtime-loaded `libusb-1.0.dll` (no link-time C
  dependency).
- **Qualcomm EDL (9008)** detection and **Sahara** identity readback.
- **Firehose service layer**: upload a Firehose programmer (`prog_firehose*.elf`)
  over Sahara, then `configure`, `read`, `write`, `erase`, `peek`, `poke`,
  `get-storage-info`, and `reset`.
- **GPT / MBR** partition parsing from a dumped disk image.
- **Android sparse** image parsing and raw expansion.
- **Samsung PIT** parsing.
- **Qualcomm `rawprogram*.xml`** firmware parsing.
- **40+ device profiles** across Qualcomm chipset families, Google Pixel,
  Samsung, Xiaomi/Redmi/POCO, OnePlus/OPPO, vivo/iQOO, Motorola, Lenovo, ASUS,
  Sony, Fairphone, plus generic eMMC/UFS fallbacks.

## Layout

| Path                  | Purpose                                                   |
|-----------------------|-----------------------------------------------------------|
| `src/lib.rs`          | Library root; re-exports the modules below.               |
| `src/transport.rs`    | `Transport` trait (mockable byte stream).                 |
| `src/usb.rs`          | libusb FFI via `libloading`; enumeration + bulk transport.|
| `src/sahara.rs`       | Sahara protocol (loader upload).                          |
| `src/firehose.rs`     | Firehose protocol (partition ops over the programmer).    |
| `src/service.rs`      | High-level Sahara+Firehose service helpers.               |
| `src/gpt.rs`          | GPT / MBR parser.                                         |
| `src/sparse.rs`       | Android sparse image parser + raw expansion.              |
| `src/pit.rs`          | Samsung PIT parser.                                       |
| `src/rawprogram.rs`   | Qualcomm `rawprogram*.xml` parser.                        |
| `src/profiles.rs`     | Built-in device profiles.                                 |
| `src/cli.rs`          | clap command definitions.                                 |
| `src/main.rs`         | `phonesvc-cli` binary.                                    |
| `src/gui_main.rs`     | `phonesvc` GUI binary (egui / eframe).                    |

The library is shared by two binaries: `phonesvc` (GUI) and `phonesvc-cli`.

## Building

Requires only the Rust toolchain (the build does **not** need a C compiler,
vcpkg, or MSVC on `PATH` — libusb is loaded dynamically at runtime).

```sh
cargo build --release
cargo test --release
```

Output binaries:

- `target/release/phonesvc.exe` — the windowed app
- `target/release/phonesvc-cli.exe` — the CLI

To run, keep an x64 `libusb-1.0.dll` next to the executables.

## GUI

Double-click `phonesvc.exe`. Tabs:

- **Devices** — list connected USB devices (bus, address, VID:PID, EDL badge).
- **Qualcomm EDL** — detect 9008 devices, read Sahara identity.
- **Service** — pick a Firehose programmer, then read / write / erase / peek /
  poke / reset / storage-info. Each action runs the full connect cycle.
- **Images** — inspect a GPT image, PIT dump, or `rawprogram*.xml` firmware
  folder.
- **Profiles** — browse built-in device profiles.

Long operations run on worker threads and report back over a channel, so the
window never blocks.

## CLI

```
phonesvc-cli devices
phonesvc-cli qc detect | info
phonesvc-cli service --programmer <prog_firehose.elf> connect|read|write|erase|peek|reset|storage-info ...
phonesvc-cli partitions <image>
phonesvc-cli pit <dump>
phonesvc-cli flash-raw <directory>
phonesvc-cli profiles list | list-vendor <vendor> | show <name>
```

Sector numbers and addresses accept decimal or `0x`-prefixed hex.

## Architecture notes

- **Transport trait** — Sahara and Firehose operate over any `Transport`, so the
  protocol logic is unit-tested against an in-memory mock (no hardware needed).
- **Runtime libusb** — `usb.rs` binds only the handful of libusb functions used,
  loaded via `libloading`. The EDL interface is assumed to be interface 0 with
  bulk endpoints OUT `0x02` / IN `0x81` (overridable in `open_transport`).
- **Two-phase device session** — Sahara uploads the Firehose programmer, then
  the same link speaks Firehose XML. `service.rs` ties these together so the GUI
  and CLI share identical wire behaviour.

## Requirements

- Windows x64.
- For raw-USB operations, a libusb-compatible driver bound to the device
  (e.g. Zadig / WinUSB). `devices` / `qc detect` only need the DLL present.

## Scope & safety

This is an independent tool for legitimate servicing, repair, and development
on devices you own or are authorized to work on. It is built only on publicly
documented protocols and does not include or depend on any vendor's proprietary
binaries. It does **not** implement anti-theft-circumvention features
(FRP / account / lockscreen removal).

Flashing the wrong image or wrong sectors can brick a device. Always verify the
firmware matches your exact model and partition layout.

## License

MIT — see [LICENSE](LICENSE).