//! Command-line interface (clap derive).

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "phonesvc",
    version,
    about = "Independent phone-servicing toolkit (Qualcomm EDL / ADB / Fastboot) — Rust"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List connected USB devices.
    Devices {
        /// Filter by vendor id (hex, e.g. 05c6).
        #[arg(long)]
        vid: Option<String>,
        /// Filter by product id (hex).
        #[arg(long)]
        pid: Option<String>,
    },
    /// Qualcomm EDL (9008) operations.
    Qc {
        #[command(subcommand)]
        cmd: QcCommand,
    },
    /// Firehose service operations (upload a programmer, then read/write/erase).
    Service {
        /// Path to a Firehose programmer (prog_firehose*.elf).
        #[arg(long)]
        programmer: String,
        #[command(subcommand)]
        cmd: ServiceCommand,
    },
    /// List GPT partitions from a dumped disk image (starting at LBA 0).
    Partitions {
        /// Path to a disk image.
        image: String,
    },
    /// List partitions from a Samsung PIT dump.
    Pit {
        /// Path to a PIT dump file.
        image: String,
    },
    /// List program entries from a Qualcomm rawprogram*.xml firmware directory.
    FlashRaw {
        /// Directory containing rawprogram*.xml and images.
        directory: String,
    },
    /// Device profiles.
    Profiles {
        #[command(subcommand)]
        cmd: ProfilesCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum QcCommand {
    /// Detect Qualcomm EDL (9008) devices.
    Detect,
    /// Read Sahara identity (hello) from a 9008 device.
    Info,
}

#[derive(Subcommand, Debug)]
pub enum ServiceCommand {
    /// Upload the programmer and run a Firehose nop (connectivity test).
    Ping,
    /// Read the Sahara hello, upload the programmer, and configure Firehose.
    Connect,
    /// Read sectors from the device to a local file.
    Read {
        /// Starting sector.
        start_sector: u64,
        /// Number of sectors to read.
        num_sectors: u64,
        /// Output file path.
        out: String,
    },
    /// Write a local file to the device.
    Write {
        /// Local file to flash.
        filename: String,
        /// Starting sector.
        start_sector: u64,
        /// Number of sectors to write.
        num_sectors: u64,
    },
    /// Erase sectors on the device.
    Erase {
        /// Starting sector.
        start_sector: u64,
        /// Number of sectors to erase.
        num_sectors: u64,
    },
    /// Peek memory at an address.
    Peek {
        /// Address (hex or decimal).
        address: String,
        /// Size in bytes.
        size: u32,
    },
    /// Reset the device.
    Reset,
    /// Query storage info for a physical partition.
    StorageInfo {
        /// Physical partition (drive) number.
        drive: u32,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProfilesCommand {
    /// List available device profiles.
    List,
    /// Filter the list by vendor (e.g. qualcomm, samsung, google, xiaomi).
    ListVendor {
        /// Vendor name.
        vendor: String,
    },
    /// Show a profile's details.
    Show {
        /// Profile name.
        name: String,
    },
}