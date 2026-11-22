//! phonesvc core library.
//!
//! Shared by the GUI app (`phonesvc.exe`) and the CLI (`phonesvc-cli.exe`).
//! Pure Rust: Qualcomm Sahara / Firehose, GPT / sparse / PIT / rawprogram
//! parsers, USB enumeration over a runtime-loaded libusb, and device profiles.

pub mod cli;
pub mod firehose;
pub mod gpt;
pub mod pit;
pub mod profiles;
pub mod rawprogram;
pub mod sahara;
pub mod service;
pub mod sparse;
pub mod transport;
pub mod usb;