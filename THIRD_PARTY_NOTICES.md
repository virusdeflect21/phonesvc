# Third-Party Notices

phonesvc is MIT-licensed (see [LICENSE](LICENSE)). It links to, or bundles,
the third-party components below. Each remains under its own license.

---

## libusb (libusb-1.0)

- **Version:** 1.0.30 (file version 1.0.30.12037)
- **Project:** https://libusb.info — https://github.com/libusb/libusb
- **Copyright:** Copyright © libusb contributors. See individual source files.
- **License:** GNU Lesser General Public License v2.1 or later (LGPL-2.1-or-later).
  Full text: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.txt

libusb is **not** statically linked into the phonesvc executables. Instead, the
x64 build of `libusb-1.0.dll` is shipped alongside the executables and loaded
dynamically at runtime via `libloading` (see `src/usb.rs`). Because the library
is used unmodified, is distributed as a separate dynamically-linkable file, and
its source is readily available from the upstream project, this use complies with
the LGPL.

- **Bundled binary:** `vendor/libusb-1.0.dll` (x64), redistributed unmodified.
- **Upstream source:** https://github.com/libusb/libusb (tag v1.0.30)
- **Prebuilt Windows binaries:** https://github.com/libusb/libusb/releases/tag/v1.0.30

To comply with the LGPL you may obtain the complete corresponding source for the
bundled libusb from the upstream links above. If you redistribute phonesvc, keep
this notice and the `vendor/libusb-1.0.dll` file (or make the library available
separately) so recipients can replace it with a compatible build.

---

## Rust crates

phonesvc's Rust dependencies are pulled from crates.io by Cargo and retain their
respective licenses. The direct dependencies are:

| Crate        | License                  |
|--------------|--------------------------|
| `clap`       | MIT OR Apache-2.0        |
| `libloading` | ISC                      |
| `anyhow`     | MIT OR Apache-2.0        |
| `hex`        | MIT OR Apache-2.0        |
| `eframe`/`egui` (egui ecosystem) | MIT OR Apache-2.0 |
| `rfd`        | MIT                      |

Run `cargo license` (via `cargo install cargo-license`) or inspect each crate's
`Cargo.toml` for the authoritative, current license of every transitive
dependency.