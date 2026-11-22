//! phonesvc-cli — command-line front-end for the `phonesvc` library.
//!
//! The windowed app is the `phonesvc` binary; this is the scriptable CLI.

use anyhow::Result;
use clap::Parser;
use phonesvc::cli::{Cli, Command, ProfilesCommand, QcCommand, ServiceCommand};
use phonesvc::transport::Transport;
use phonesvc::{gpt, pit, profiles, rawprogram, sahara, service, usb};

fn cmd_devices(vid: Option<String>, pid: Option<String>) -> Result<()> {
    let usb = usb::Libusb::load()?;
    let devices = usb.enumerate()?;
    let vid_f = vid.as_deref().and_then(|s| u16::from_str_radix(s, 16).ok());
    let pid_f = pid.as_deref().and_then(|s| u16::from_str_radix(s, 16).ok());

    println!("BUS  ADDR  VID:PID");
    let mut any = false;
    for d in &devices {
        if let Some(v) = vid_f {
            if d.vid != v {
                continue;
            }
        }
        if let Some(p) = pid_f {
            if d.pid != p {
                continue;
            }
        }
        println!("{:>4} {:>5}  {:04x}:{:04x}", d.bus, d.address, d.vid, d.pid);
        any = true;
    }
    if !any {
        println!("(no matching devices)");
    }
    Ok(())
}

fn cmd_qc_detect() -> Result<()> {
    let usb = usb::Libusb::load()?;
    let edl = usb.detect_edl()?;
    if edl.is_empty() {
        println!("No Qualcomm EDL (9008) devices detected.");
        return Ok(());
    }
    println!("Qualcomm EDL (9008) devices:");
    for d in &edl {
        println!("  bus {} addr {}  {:04x}:{:04x}", d.bus, d.address, d.vid, d.pid);
    }
    Ok(())
}

fn cmd_qc_info() -> Result<()> {
    let usb = usb::Libusb::load()?;
    let edl = usb.detect_edl()?;
    if edl.is_empty() {
        anyhow::bail!("no Qualcomm EDL (9008) device found");
    }
    let mut t = usb.open_transport(usb::QUALCOMM_VID, usb::EDL_PID, 0x02, 0x81)?;
    t.set_read_timeout_ms(5000);
    let mut s = sahara::SaharaSession::new(&mut t);
    let hello = s.wait_hello()?;
    println!("Sahara hello:");
    println!("  version           : {}", hello.version);
    println!("  compatible version: {}", hello.compatible_version);
    println!("  mode              : {}", hello.mode);
    println!("  image id          : {}", hello.image_id);
    println!("  image length      : {} bytes", hello.image_len);
    Ok(())
}

fn cmd_partitions(image: &str) -> Result<()> {
    let data = std::fs::read(image)?;
    let (hdr, parts) = gpt::parse_gpt(&data)?;
    println!("Disk GUID: {}", hdr.disk_guid);
    println!(
        "First usable LBA: {}  Last usable LBA: {}",
        hdr.first_usable_lba, hdr.last_usable_lba
    );
    println!("Partitions: {}", parts.len());
    for p in &parts {
        println!(
            "  [{:>2}] {:<32} type={} sectors={} size={}B",
            p.index,
            if p.name.is_empty() { "(unnamed)" } else { &p.name },
            p.type_guid,
            p.num_sectors(),
            p.size_bytes()
        );
    }
    Ok(())
}

fn cmd_pit(image: &str) -> Result<()> {
    let data = std::fs::read(image)?;
    let entries = pit::parse_pit(&data)?;
    println!("PIT entries: {}", entries.len());
    for e in &entries {
        println!(
            "  [{:>2}] {:<20} dev={} blocks={} block_size={} fs={}",
            e.index, e.name, e.device_type, e.block_count, e.block_size, e.fs_type
        );
    }
    Ok(())
}

fn cmd_flash_raw(directory: &str) -> Result<()> {
    let entries = rawprogram::parse_rawprogram_dir(directory)?;
    let progs = rawprogram::programs_only(&entries);
    println!(
        "Parsed {} entries ({} program) from {}",
        entries.len(),
        progs.len(),
        directory
    );
    for e in &entries {
        let kind = match e.kind {
            rawprogram::EntryKind::Program => "program",
            rawprogram::EntryKind::Erase => "erase  ",
        };
        println!(
            "  {} {:<16} part={} start={} sectors={} size={}B file={}",
            kind,
            e.label,
            e.physical_partition,
            e.start_sector,
            e.num_sectors,
            e.size_bytes(),
            e.filename
        );
    }
    Ok(())
}

fn cmd_profiles_list() {
    println!("Built-in device profiles ({}):", profiles::BUILTIN.len());
    for p in profiles::BUILTIN {
        println!(
            "  {:<28} [{:?}/{:?}] {} (sector_size={})",
            p.name, p.vendor, p.storage, p.description, p.sector_size
        );
    }
}

fn cmd_profiles_list_vendor(vendor: &str) -> Result<()> {
    let v = parse_vendor(vendor)?;
    let list = profiles::by_vendor(v);
    if list.is_empty() {
        println!("No profiles for vendor {:?}.", v);
        return Ok(());
    }
    println!("{:?} profiles ({}):", v, list.len());
    for p in list {
        println!(
            "  {:<28} [{:?}] {} (sector_size={})",
            p.name, p.storage, p.description, p.sector_size
        );
    }
    Ok(())
}

fn parse_vendor(s: &str) -> Result<profiles::Vendor> {
    use profiles::Vendor;
    Ok(match s.to_lowercase().as_str() {
        "qualcomm" | "qcom" => Vendor::Qualcomm,
        "samsung" => Vendor::Samsung,
        "google" => Vendor::Google,
        "xiaomi" => Vendor::Xiaomi,
        "oneplus" => Vendor::OnePlus,
        "oppo" => Vendor::Oppo,
        "vivo" => Vendor::Vivo,
        "motorola" => Vendor::Motorola,
        "lenovo" => Vendor::Lenovo,
        "asus" => Vendor::Asus,
        "sony" => Vendor::Sony,
        "lg" => Vendor::Lg,
        "htc" => Vendor::Htc,
        "microsoft" => Vendor::Microsoft,
        "fairphone" => Vendor::Fairphone,
        "generic" => Vendor::Generic,
        other => anyhow::bail!("unknown vendor {:?} (try: qualcomm, samsung, google, xiaomi, ...)", other),
    })
}

fn cmd_profiles_show(name: &str) -> Result<()> {
    let p = profiles::find(name).ok_or_else(|| anyhow::anyhow!("no builtin profile named {:?}", name))?;
    println!("Profile: {}", p.name);
    println!("  {} ", p.description);
    println!("  vendor      : {:?}", p.vendor);
    println!("  storage     : {:?}", p.storage);
    println!("  sector_size : {}", p.sector_size);
    println!("  partitions  : {}", p.partitions.len());
    for part in p.partitions {
        println!("    {:<16} sector_size={}", part.label, part.sector_size);
    }
    Ok(())
}

/// Open the first EDL device, upload the programmer, and hand back a transport
/// that is now speaking Firehose. Caller runs Firehose ops on the closure.
fn with_firehose<F, R>(programmer_path: &str, f: F) -> Result<R>
where
    F: FnOnce(&mut usb::LibusbTransport) -> Result<R>,
{
    let prog = std::fs::read(programmer_path)?;
    service::validate_programmer(&prog)?;
    let usbctx = usb::Libusb::load()?;
    let edl = usbctx.detect_edl()?;
    if edl.is_empty() {
        anyhow::bail!("no Qualcomm EDL (9008) device found");
    }
    let mut t = usbctx.open_transport(usb::QUALCOMM_VID, usb::EDL_PID, service::DEFAULT_OUT_EP, service::DEFAULT_IN_EP)?;
    t.set_read_timeout_ms(10000);
    let summary = service::upload_programmer(&mut t, &prog)?;
    println!(
        "programmer uploaded (image_id={}, image_len={} bytes)",
        summary.image_id, summary.image_len
    );
    let r = service::configure(&mut t, 1 << 20)?;
    println!("firehose configured: {}", if r.ok() { "ok" } else { "FAILED" });
    f(&mut t)
}

fn cmd_service(programmer: &str, cmd: ServiceCommand) -> Result<()> {
    match cmd {
        ServiceCommand::Connect => {
            with_firehose(programmer, |_t| {
                println!("connected: programmer uploaded and Firehose configured.");
                Ok(())
            })
        }
        ServiceCommand::Ping => with_firehose(programmer, |t| {
            let r = service::peek(t, 0, 0)?;
            let _ = r;
            println!("ping ok (firehose reachable).");
            Ok(())
        }),
        ServiceCommand::Read { start_sector, num_sectors, out } => with_firehose(programmer, |t| {
            let n = service::read_sectors(t, start_sector, num_sectors, &out)?;
            println!("read {} bytes -> {}", n, out);
            Ok(())
        }),
        ServiceCommand::Write { filename, start_sector, num_sectors } => with_firehose(programmer, |t| {
            let meta = std::fs::metadata(&filename)?;
            let size = meta.len();
            let mut last_decile = 0u64;
            let r = service::write_file(
                t,
                &filename,
                start_sector,
                num_sectors,
                size,
                Some(|sent, total| {
                    let pct = if total > 0 { sent * 100 / total } else { 0 };
                    let decile = pct / 10;
                    if decile != last_decile {
                        println!("  writing: {}%", pct);
                        last_decile = decile;
                    }
                }),
            )?;
            println!("write done: {}", if r.ok() { "ok" } else { "FAILED" });
            Ok(())
        }),
        ServiceCommand::Erase { start_sector, num_sectors } => with_firehose(programmer, |t| {
            let r = service::erase(t, start_sector, num_sectors)?;
            println!("erase done: {}", if r.ok() { "ok" } else { "FAILED" });
            Ok(())
        }),
        ServiceCommand::Peek { address, size } => with_firehose(programmer, |t| {
            let addr = parse_num(&address)?;
            let data = service::peek(t, addr, size)?;
            println!("peek {} bytes at {:#x}:", data.len(), addr);
            for (i, chunk) in data.chunks(16).enumerate() {
                let hex: Vec<String> = chunk.iter().map(|b| format!("{:02x}", b)).collect();
                println!("  {:04x}: {}", i * 16, hex.join(" "));
            }
            Ok(())
        }),
        ServiceCommand::Reset => with_firehose(programmer, |t| {
            let r = service::reset(t)?;
            println!("reset sent: {}", if r.ok() { "ok" } else { "FAILED" });
            Ok(())
        }),
        ServiceCommand::StorageInfo { drive } => with_firehose(programmer, |t| {
            let info = service::storage_info(t, drive)?;
            println!("storage info (drive {}): {}", drive, info);
            Ok(())
        }),
    }
}

fn parse_num(s: &str) -> Result<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| e.into())
    } else {
        s.parse::<u64>().map_err(|e| e.into())
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Devices { vid, pid } => cmd_devices(vid, pid),
        Command::Qc { cmd } => match cmd {
            QcCommand::Detect => cmd_qc_detect(),
            QcCommand::Info => cmd_qc_info(),
        },
        Command::Service { programmer, cmd } => cmd_service(&programmer, cmd),
        Command::Partitions { image } => cmd_partitions(&image),
        Command::Pit { image } => cmd_pit(&image),
        Command::FlashRaw { directory } => cmd_flash_raw(&directory),
        Command::Profiles { cmd } => match cmd {
            ProfilesCommand::List => {
                cmd_profiles_list();
                Ok(())
            }
            ProfilesCommand::ListVendor { vendor } => cmd_profiles_list_vendor(&vendor),
            ProfilesCommand::Show { name } => cmd_profiles_show(&name),
        },
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {:#}", e);
        std::process::exit(1);
    }
}