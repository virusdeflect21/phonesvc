//! phonesvc — windowed phone-servicing app (egui/eframe).
//!
//! A native GUI over the `phonesvc` library: USB enumeration, Qualcomm EDL
//! detection + Sahara identity, GPT / PIT / rawprogram image inspection, and
//! device profiles. Long-running work (USB I/O, large file parsing) runs on
//! worker threads and reports back over a channel so the UI never blocks.
//!
//! Builds with `#![windows_subsystem = "windows"]` in release so double-clicking
//! the exe opens a window with no console flash.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;
use phonesvc::sahara::SaharaHello;
use phonesvc::transport::Transport;
use phonesvc::usb::UsbDeviceInfo;
use phonesvc::{gpt, pit, profiles, rawprogram, sahara, service, usb};

/// Messages sent from worker threads back to the UI.
enum Msg {
    Devices(Vec<UsbDeviceInfo>),
    Edl(Vec<UsbDeviceInfo>),
    Sahara(SaharaHello),
    Gpt(gpt::GptHeader, Vec<gpt::GptPartition>),
    Pit(Vec<pit::PitEntry>),
    Raw(Vec<rawprogram::RawEntry>),
    /// Result of a Firehose peek: the address and the bytes read back.
    Peek(u64, Vec<u8>),
    /// Result of a Firehose get-storage-info: the drive and the info string.
    StorageInfo(u32, String),
    Log(String),
    Error(String),
}

#[derive(PartialEq)]
enum Tab {
    Devices,
    Qualcomm,
    Service,
    Images,
    Profiles,
}

struct App {
    tab: Tab,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    busy: bool,

    devices: Vec<UsbDeviceInfo>,
    edl: Vec<UsbDeviceInfo>,
    sahara: Option<SaharaHello>,

    gpt: Option<(gpt::GptHeader, Vec<gpt::GptPartition>)>,
    pit: Option<Vec<pit::PitEntry>>,
    raw: Option<Vec<rawprogram::RawEntry>>,
    raw_dir: String,

    selected_profile: Option<&'static profiles::DeviceProfile>,
    log: Vec<String>,

    // ---- Service (Firehose) tab state ----
    programmer_path: String,
    svc_connected: bool,
    // read
    rd_start: String,
    rd_sectors: String,
    rd_out: String,
    // write
    wr_file: String,
    wr_start: String,
    wr_sectors: String,
    // erase
    er_start: String,
    er_sectors: String,
    // peek / poke
    pk_addr: String,
    pk_size: String,
    po_addr: String,
    po_data: String,
    // storage info
    si_drive: String,
    // last peek result
    last_peek: Option<(u64, Vec<u8>)>,
    last_storage: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Sane dark defaults.
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals.dark_mode = true;
        cc.egui_ctx.set_style(style);

        let (tx, rx) = channel();
        let mut app = App {
            tab: Tab::Devices,
            tx,
            rx,
            busy: false,
            devices: Vec::new(),
            edl: Vec::new(),
            sahara: None,
            gpt: None,
            pit: None,
            raw: None,
            raw_dir: String::new(),
            selected_profile: None,
            log: Vec::new(),
            programmer_path: String::new(),
            svc_connected: false,
            rd_start: "0".to_string(),
            rd_sectors: "1".to_string(),
            rd_out: String::new(),
            wr_file: String::new(),
            wr_start: "0".to_string(),
            wr_sectors: "1".to_string(),
            er_start: "0".to_string(),
            er_sectors: "1".to_string(),
            pk_addr: "0".to_string(),
            pk_size: "16".to_string(),
            po_addr: "0".to_string(),
            po_data: String::new(),
            si_drive: "0".to_string(),
            last_peek: None,
            last_storage: None,
        };
        app.logln("phonesvc ready.");
        app
    }

    fn logln(&mut self, s: impl Into<String>) {
        let s = s.into();
        eprintln!("{}", s);
        self.log.push(s);
        if self.log.len() > 500 {
            let drain = self.log.len() - 500;
            self.log.drain(0..drain);
        }
    }

    /// Spawn a worker that produces a `Msg`, marking the UI busy while it runs.
    fn spawn<F>(&mut self, f: F)
    where
        F: FnOnce() -> anyhow::Result<Msg> + Send + 'static,
    {
        let tx = self.tx.clone();
        self.busy = true;
        std::thread::spawn(move || {
            let msg = f().unwrap_or_else(|e| Msg::Error(format!("{e:#}")));
            let _ = tx.send(msg);
        });
    }

    // ---- actions ----

    fn action_refresh_devices(&mut self) {
        self.spawn(|| {
            let u = usb::Libusb::load()?;
            let devices = u.enumerate()?;
            Ok(Msg::Devices(devices))
        });
    }

    fn action_detect_edl(&mut self) {
        self.spawn(|| {
            let u = usb::Libusb::load()?;
            let edl = u.detect_edl()?;
            Ok(Msg::Edl(edl))
        });
    }

    fn action_sahara_info(&mut self) {
        self.spawn(|| {
            let u = usb::Libusb::load()?;
            let edl = u.detect_edl()?;
            if edl.is_empty() {
                anyhow::bail!("no Qualcomm EDL (9008) device found");
            }
            let mut t = u.open_transport(usb::QUALCOMM_VID, usb::EDL_PID, 0x02, 0x81)?;
            t.set_read_timeout_ms(5000);
            let mut s = sahara::SaharaSession::new(&mut t);
            let hello = s.wait_hello()?;
            Ok(Msg::Sahara(hello))
        });
    }

    fn action_open_gpt(&mut self, path: String) {
        self.spawn(move || {
            let data = std::fs::read(&path)?;
            let (hdr, parts) = gpt::parse_gpt(&data)?;
            Ok(Msg::Gpt(hdr, parts))
        });
    }

    fn action_open_pit(&mut self, path: String) {
        self.spawn(move || {
            let data = std::fs::read(&path)?;
            let entries = pit::parse_pit(&data)?;
            Ok(Msg::Pit(entries))
        });
    }

    fn action_open_raw(&mut self, dir: String) {
        self.spawn(move || {
            let entries = rawprogram::parse_rawprogram_dir(&dir)?;
            Ok(Msg::Raw(entries))
        });
    }

    // ---- Service (Firehose) actions ----
    //
    // Each action runs the full connect cycle (load libusb → open the 9008
    // transport → Sahara-upload the programmer → Firehose configure → op) on a
    // worker thread, because the transport borrows the libusb context and can't
    // be held across UI frames. That mirrors how the CLI's `with_firehose`
    // works, so GUI and CLI share identical wire behaviour.

    fn action_service_connect(&mut self) {
        let prog_path = self.programmer_path.clone();
        self.spawn(move || {
            with_service(&prog_path, |t| {
                let r = service::configure(t, 1 << 20)?;
                if !r.ok() {
                    anyhow::bail!("firehose configure failed");
                }
                Ok(Msg::Log(
                    "service: programmer uploaded, firehose configured".to_string(),
                ))
            })
        });
        self.svc_connected = true;
    }

    fn action_service_read(&mut self) {
        let prog_path = self.programmer_path.clone();
        let start = self.rd_start.clone();
        let sectors = self.rd_sectors.clone();
        let out = self.rd_out.clone();
        self.spawn(move || {
            if out.is_empty() {
                anyhow::bail!("choose an output file first");
            }
            let start = parse_num(&start)?;
            let sectors = parse_num(&sectors)?;
            with_service(&prog_path, |t| {
                let n = service::read_sectors(t, start, sectors, &out)?;
                Ok(Msg::Log(format!("service: read {} byte(s) -> {}", n, out)))
            })
        });
    }

    fn action_service_write(&mut self) {
        let prog_path = self.programmer_path.clone();
        let file = self.wr_file.clone();
        let start = self.wr_start.clone();
        let sectors = self.wr_sectors.clone();
        self.spawn(move || {
            if file.is_empty() {
                anyhow::bail!("choose a file to write first");
            }
            let start = parse_num(&start)?;
            let sectors = parse_num(&sectors)?;
            let size = std::fs::metadata(&file)?.len();
            with_service(&prog_path, |t| {
                let r = service::write_file(t, &file, start, sectors, size, None::<fn(u64, u64)>)?;
                Ok(Msg::Log(format!(
                    "service: wrote {} byte(s): {}",
                    size,
                    if r.ok() { "ok" } else { "FAILED" }
                )))
            })
        });
    }

    fn action_service_erase(&mut self) {
        let prog_path = self.programmer_path.clone();
        let start = self.er_start.clone();
        let sectors = self.er_sectors.clone();
        self.spawn(move || {
            let start = parse_num(&start)?;
            let sectors = parse_num(&sectors)?;
            with_service(&prog_path, |t| {
                let r = service::erase(t, start, sectors)?;
                Ok(Msg::Log(format!(
                    "service: erase {} sector(s) from {}: {}",
                    sectors,
                    start,
                    if r.ok() { "ok" } else { "FAILED" }
                )))
            })
        });
    }

    fn action_service_peek(&mut self) {
        let prog_path = self.programmer_path.clone();
        let addr = self.pk_addr.clone();
        let size = self.pk_size.clone();
        self.spawn(move || {
            let addr = parse_num(&addr)?;
            let size: u32 = parse_num(&size)? as u32;
            with_service(&prog_path, |t| {
                let data = service::peek(t, addr, size)?;
                Ok(Msg::Peek(addr, data))
            })
        });
    }

    fn action_service_poke(&mut self) {
        let prog_path = self.programmer_path.clone();
        let addr = self.po_addr.clone();
        let data = self.po_data.clone();
        self.spawn(move || {
            let addr = parse_num(&addr)?;
            let bytes = parse_hex_bytes(&data)?;
            with_service(&prog_path, |t| {
                let r = service::poke(t, addr, &bytes)?;
                Ok(Msg::Log(format!(
                    "service: poked {} byte(s) at {:#x}: {}",
                    bytes.len(),
                    addr,
                    if r.ok() { "ok" } else { "FAILED" }
                )))
            })
        });
    }

    fn action_service_reset(&mut self) {
        let prog_path = self.programmer_path.clone();
        self.spawn(move || {
            with_service(&prog_path, |t| {
                let r = service::reset(t)?;
                Ok(Msg::Log(format!(
                    "service: reset sent: {}",
                    if r.ok() { "ok" } else { "FAILED" }
                )))
            })
        });
    }

    fn action_service_storage_info(&mut self) {
        let prog_path = self.programmer_path.clone();
        let drive = self.si_drive.clone();
        self.spawn(move || {
            let drive: u32 = parse_num(&drive)? as u32;
            with_service(&prog_path, |t| {
                let info = service::storage_info(t, drive)?;
                Ok(Msg::StorageInfo(drive, info))
            })
        });
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Devices(d) => {
                    self.busy = false;
                    self.logln(format!("enumerated {} USB device(s)", d.len()));
                    self.devices = d;
                }
                Msg::Edl(d) => {
                    self.busy = false;
                    if d.is_empty() {
                        self.logln("no Qualcomm EDL (9008) devices detected");
                    } else {
                        self.logln(format!("found {} EDL device(s)", d.len()));
                    }
                    self.edl = d;
                }
                Msg::Sahara(h) => {
                    self.busy = false;
                    self.logln(format!(
                        "Sahara hello: version={} compatible={} mode={} image_id={} image_len={}",
                        h.version, h.compatible_version, h.mode, h.image_id, h.image_len
                    ));
                    self.sahara = Some(h);
                }
                Msg::Gpt(hdr, parts) => {
                    self.busy = false;
                    self.logln(format!(
                        "GPT: {} partition(s), disk GUID {}",
                        parts.len(),
                        hdr.disk_guid
                    ));
                    self.gpt = Some((hdr, parts));
                }
                Msg::Pit(entries) => {
                    self.busy = false;
                    self.logln(format!("PIT: {} entrie(s)", entries.len()));
                    self.pit = Some(entries);
                }
                Msg::Raw(entries) => {
                    self.busy = false;
                    let progs = entries
                        .iter()
                        .filter(|e| e.kind == rawprogram::EntryKind::Program)
                        .count();
                    self.logln(format!(
                        "rawprogram: {} entries ({} program)",
                        entries.len(),
                        progs
                    ));
                    self.raw = Some(entries);
                }
                Msg::Peek(addr, data) => {
                    self.busy = false;
                    self.logln(format!("peek: read {} byte(s) at {:#x}", data.len(), addr));
                    self.last_peek = Some((addr, data));
                }
                Msg::StorageInfo(drive, info) => {
                    self.busy = false;
                    self.logln(format!("storage info (drive {}): {}", drive, info));
                    self.last_storage = Some(info);
                }
                Msg::Log(s) => self.logln(s),
                Msg::Error(e) => {
                    self.busy = false;
                    self.logln(format!("ERROR: {e}"));
                }
            }
        }
    }
}

fn fmt_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", n, UNITS[0])
    } else {
        format!("{:.2} {}", v, UNITS[u])
    }
}

/// Read and sanity-check a Firehose programmer image from disk.
fn read_programmer(path: &str) -> anyhow::Result<Vec<u8>> {
    if path.is_empty() {
        anyhow::bail!("select a Firehose programmer file first");
    }
    let prog = std::fs::read(path)?;
    service::validate_programmer(&prog)?;
    Ok(prog)
}

/// Run the full service connect cycle — load libusb, open the first 9008
/// device, Sahara-upload the programmer, then hand the live Firehose transport
/// to `f`. The libusb context and transport both live in this stack frame, so
/// the borrow is valid and everything is torn down cleanly on return.
fn with_service<F>(programmer_path: &str, f: F) -> anyhow::Result<Msg>
where
    F: FnOnce(&mut usb::LibusbTransport<'_>) -> anyhow::Result<Msg>,
{
    let prog = read_programmer(programmer_path)?;
    let ctx = usb::Libusb::load()?;
    let edl = ctx.detect_edl()?;
    if edl.is_empty() {
        anyhow::bail!("no Qualcomm EDL (9008) device found");
    }
    let mut t = ctx.open_transport(
        usb::QUALCOMM_VID,
        usb::EDL_PID,
        service::DEFAULT_OUT_EP,
        service::DEFAULT_IN_EP,
    )?;
    t.set_read_timeout_ms(10000);
    service::upload_programmer(&mut t, &prog)?;
    f(&mut t)
}

/// Parse a number that is either decimal or `0x`-prefixed hex.
fn parse_num(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Ok(u64::from_str_radix(hex, 16)?)
    } else {
        Ok(s.parse::<u64>()?)
    }
}

/// Parse a whitespace/comma separated hex byte string (e.g. "de ad be ef").
fn parse_hex_bytes(s: &str) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    for tok in s.split(|c: char| c.is_whitespace() || c == ',') {
        let tok = tok.trim();
        if tok.is_empty() {
            continue;
        }
        let tok = tok.strip_prefix("0x").unwrap_or(tok);
        out.push(u8::from_str_radix(tok, 16)?);
    }
    Ok(out)
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages();

        // While a worker is running, keep polling for its result.
        if self.busy {
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("phonesvc");
                ui.separator();
                ui.label("Independent phone-servicing toolkit");
                if self.busy {
                    ui.add_space(12.0);
                    ui.spinner();
                    ui.label("working…");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                });
            });
        });

        egui::TopBottomPanel::bottom("log").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Log");
                ui.label(format!("({} lines)", self.log.len()));
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.log {
                        ui.label(
                            egui::RichText::new(line)
                                .monospace()
                                .size(12.0)
                                .color(egui::Color32::from_rgb(180, 200, 180)),
                        );
                    }
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Devices, "Devices");
                ui.selectable_value(&mut self.tab, Tab::Qualcomm, "Qualcomm EDL");
                ui.selectable_value(&mut self.tab, Tab::Service, "Service");
                ui.selectable_value(&mut self.tab, Tab::Images, "Images");
                ui.selectable_value(&mut self.tab, Tab::Profiles, "Profiles");
            });
            ui.separator();

            match self.tab {
                Tab::Devices => self.ui_devices(ui),
                Tab::Qualcomm => self.ui_qualcomm(ui),
                Tab::Service => self.ui_service(ui),
                Tab::Images => self.ui_images(ui),
                Tab::Profiles => self.ui_profiles(ui),
            }
        });
    }
}

impl App {
    fn ui_devices(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("↻  Refresh").clicked() && !self.busy {
                self.action_refresh_devices();
            }
            ui.label(format!("{} device(s)", self.devices.len()));
        });
        ui.add_space(6.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("devgrid")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("BUS");
                    ui.strong("ADDR");
                    ui.strong("VID:PID");
                    ui.strong("TYPE");
                    ui.end_row();
                    for d in &self.devices {
                        ui.label(d.bus.to_string());
                        ui.label(d.address.to_string());
                        ui.label(format!("{:04x}:{:04x}", d.vid, d.pid));
                        if d.is_edl() {
                            ui.colored_label(egui::Color32::GREEN, "EDL (9008)");
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        });
    }

    fn ui_qualcomm(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Detect EDL devices").clicked() && !self.busy {
                self.action_detect_edl();
            }
            if ui.button("Read Sahara identity").clicked() && !self.busy {
                self.action_sahara_info();
            }
        });
        ui.add_space(8.0);

        if !self.edl.is_empty() {
            ui.heading(format!("EDL devices ({})", self.edl.len()));
            for d in &self.edl {
                ui.label(format!(
                    "  bus {} addr {}  {:04x}:{:04x}",
                    d.bus, d.address, d.vid, d.pid
                ));
            }
            ui.add_space(8.0);
        } else {
            ui.label("No EDL devices detected yet. Put the phone into 9008 / EDL mode, then Detect.");
            ui.add_space(8.0);
        }

        if let Some(h) = &self.sahara {
            ui.heading("Sahara hello");
            egui::Grid::new("saharagrid").num_columns(2).show(ui, |ui| {
                ui.label("version"); ui.label(h.version.to_string()); ui.end_row();
                ui.label("compatible version"); ui.label(h.compatible_version.to_string()); ui.end_row();
                ui.label("mode"); ui.label(h.mode.to_string()); ui.end_row();
                ui.label("image id"); ui.label(h.image_id.to_string()); ui.end_row();
                ui.label("image length"); ui.label(format!("{} bytes", h.image_len)); ui.end_row();
            });
        }
    }

    fn ui_service(&mut self, ui: &mut egui::Ui) {
        ui.heading("Firehose service");
        ui.label(
            "Upload a Firehose programmer (prog_firehose*.elf) over Sahara, then run read/write/erase/peek/poke/reset. \
             Put the phone into 9008 / EDL mode first. Each action re-runs the connect cycle.",
        );
        ui.add_space(8.0);

        // ---- programmer picker ----
        ui.horizontal(|ui| {
            ui.label("Programmer:");
            ui.add(
                egui::TextEdit::singleline(&mut self.programmer_path)
                    .desired_width(420.0)
                    .hint_text("path to prog_firehose*.elf"),
            );
            if ui.button("Browse…").clicked() && !self.busy {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Firehose programmer", &["elf", "mbn", "bin"])
                    .pick_file()
                {
                    self.programmer_path = p.to_string_lossy().to_string();
                }
            }
            if ui.button("Connect / Upload").clicked() && !self.busy {
                self.logln("service: uploading programmer…");
                self.action_service_connect();
            }
        });
        ui.add_space(4.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            // ---- Read ----
            ui.collapsing("Read sectors → file", |ui| {
                ui.horizontal(|ui| {
                    ui.label("start");
                    ui.add(egui::TextEdit::singleline(&mut self.rd_start).desired_width(110.0));
                    ui.label("sectors");
                    ui.add(egui::TextEdit::singleline(&mut self.rd_sectors).desired_width(110.0));
                    ui.label("out");
                    ui.add(egui::TextEdit::singleline(&mut self.rd_out).desired_width(220.0));
                    if ui.button("Save…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_file_name("dump.bin").save_file() {
                            self.rd_out = p.to_string_lossy().to_string();
                        }
                    }
                    if ui.button("Read").clicked() && !self.busy {
                        self.action_service_read();
                    }
                });
            });

            // ---- Write ----
            ui.collapsing("Write file → sectors", |ui| {
                ui.horizontal(|ui| {
                    ui.label("file");
                    ui.add(egui::TextEdit::singleline(&mut self.wr_file).desired_width(260.0));
                    if ui.button("Open…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                            self.wr_file = p.to_string_lossy().to_string();
                        }
                    }
                    ui.label("start");
                    ui.add(egui::TextEdit::singleline(&mut self.wr_start).desired_width(90.0));
                    ui.label("sectors");
                    ui.add(egui::TextEdit::singleline(&mut self.wr_sectors).desired_width(90.0));
                    if ui.button("Write").clicked() && !self.busy {
                        self.action_service_write();
                    }
                });
            });

            // ---- Erase ----
            ui.collapsing("Erase sectors", |ui| {
                ui.horizontal(|ui| {
                    ui.label("start");
                    ui.add(egui::TextEdit::singleline(&mut self.er_start).desired_width(110.0));
                    ui.label("sectors");
                    ui.add(egui::TextEdit::singleline(&mut self.er_sectors).desired_width(110.0));
                    if ui.button("Erase").clicked() && !self.busy {
                        self.action_service_erase();
                    }
                });
            });

            // ---- Peek / Poke ----
            ui.collapsing("Peek / Poke memory", |ui| {
                ui.horizontal(|ui| {
                    ui.label("peek addr");
                    ui.add(egui::TextEdit::singleline(&mut self.pk_addr).desired_width(120.0));
                    ui.label("size");
                    ui.add(egui::TextEdit::singleline(&mut self.pk_size).desired_width(80.0));
                    if ui.button("Peek").clicked() && !self.busy {
                        self.action_service_peek();
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("poke addr");
                    ui.add(egui::TextEdit::singleline(&mut self.po_addr).desired_width(120.0));
                    ui.label("bytes");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.po_data)
                            .desired_width(240.0)
                            .hint_text("de ad be ef"),
                    );
                    if ui.button("Poke").clicked() && !self.busy {
                        self.action_service_poke();
                    }
                });
                if let Some((addr, data)) = &self.last_peek {
                    ui.add_space(4.0);
                    ui.label(format!("last peek @ {:#x} ({} bytes):", addr, data.len()));
                    let mut line = String::new();
                    for (i, b) in data.iter().enumerate() {
                        if i > 0 && i % 16 == 0 {
                            ui.label(egui::RichText::new(&line).monospace().size(12.0));
                            line.clear();
                        }
                        line.push_str(&format!("{:02x} ", b));
                    }
                    if !line.is_empty() {
                        ui.label(egui::RichText::new(&line).monospace().size(12.0));
                    }
                }
            });

            // ---- Storage info / reset ----
            ui.collapsing("Storage info / reset", |ui| {
                ui.horizontal(|ui| {
                    ui.label("drive");
                    ui.add(egui::TextEdit::singleline(&mut self.si_drive).desired_width(80.0));
                    if ui.button("Storage info").clicked() && !self.busy {
                        self.action_service_storage_info();
                    }
                    if ui.button("Reset device").clicked() && !self.busy {
                        self.action_service_reset();
                    }
                });
                if let Some(info) = &self.last_storage {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new(info).monospace().size(12.0));
                }
            });
        });
    }

    fn ui_images(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Open GPT image…").clicked() && !self.busy {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Disk image", &["img", "bin", "raw"])
                    .pick_file()
                {
                    let path = p.to_string_lossy().to_string();
                    self.logln(format!("reading {}", path));
                    self.action_open_gpt(path);
                }
            }
            if ui.button("Open PIT dump…").clicked() && !self.busy {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("PIT dump", &["bin", "pit"])
                    .pick_file()
                {
                    let path = p.to_string_lossy().to_string();
                    self.logln(format!("reading {}", path));
                    self.action_open_pit(path);
                }
            }
            if ui.button("Open firmware folder…").clicked() && !self.busy {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    let dir = p.to_string_lossy().to_string();
                    self.logln(format!("scanning {}", dir));
                    self.raw_dir = dir.clone();
                    self.action_open_raw(dir);
                }
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some((hdr, parts)) = &self.gpt {
                ui.heading(format!("GPT — {} partitions", parts.len()));
                ui.label(format!("Disk GUID: {}", hdr.disk_guid));
                ui.label(format!(
                    "First usable LBA: {}   Last usable LBA: {}",
                    hdr.first_usable_lba, hdr.last_usable_lba
                ));
                ui.add_space(4.0);
                egui::Grid::new("gptgrid")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("#"); ui.strong("name"); ui.strong("type GUID");
                        ui.strong("sectors"); ui.strong("size");
                        ui.end_row();
                        for p in parts {
                            ui.label(p.index.to_string());
                            ui.label(if p.name.is_empty() { "(unnamed)" } else { &p.name });
                            ui.label(&p.type_guid);
                            ui.label(p.num_sectors().to_string());
                            ui.label(fmt_bytes(p.size_bytes()));
                            ui.end_row();
                        }
                    });
                ui.add_space(12.0);
            }

            if let Some(entries) = &self.pit {
                ui.heading(format!("PIT — {} entries", entries.len()));
                egui::Grid::new("pitgrid")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("#"); ui.strong("name"); ui.strong("dev");
                        ui.strong("blocks"); ui.strong("fs");
                        ui.end_row();
                        for e in entries {
                            ui.label(e.index.to_string());
                            ui.label(&e.name);
                            ui.label(e.device_type.to_string());
                            ui.label(e.block_count.to_string());
                            ui.label(&e.fs_type);
                            ui.end_row();
                        }
                    });
                ui.add_space(12.0);
            }

            if let Some(entries) = &self.raw {
                ui.heading(format!("rawprogram — {} entries", entries.len()));
                if !self.raw_dir.is_empty() {
                    ui.label(format!("folder: {}", self.raw_dir));
                }
                ui.add_space(4.0);
                egui::Grid::new("rawgrid")
                    .num_columns(6)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("kind"); ui.strong("label"); ui.strong("part");
                        ui.strong("start"); ui.strong("sectors"); ui.strong("file");
                        ui.end_row();
                        for e in entries {
                            let kind = match e.kind {
                                rawprogram::EntryKind::Program => "program",
                                rawprogram::EntryKind::Erase => "erase",
                            };
                            ui.label(kind);
                            ui.label(&e.label);
                            ui.label(e.physical_partition.to_string());
                            ui.label(e.start_sector.to_string());
                            ui.label(e.num_sectors.to_string());
                            ui.label(&e.filename);
                            ui.end_row();
                        }
                    });
                ui.add_space(12.0);
            }

            if self.gpt.is_none() && self.pit.is_none() && self.raw.is_none() {
                ui.label("Open a GPT image, a PIT dump, or a Qualcomm firmware folder to inspect it here.");
            }
        });
    }

    fn ui_profiles(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Built-in device profiles:");
        });
        ui.add_space(4.0);
        for p in profiles::BUILTIN {
            if ui
                .selectable_label(self.selected_profile.map(|s| s.name) == Some(p.name), p.name)
                .clicked()
            {
                self.selected_profile = Some(p);
            }
        }
        ui.add_space(8.0);
        ui.separator();

        if let Some(p) = self.selected_profile {
            ui.heading(p.name);
            ui.label(p.description);
            ui.add_space(4.0);
            ui.label(format!("sector_size: {}", p.sector_size));
            ui.add_space(4.0);
            egui::Grid::new("profgrid")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("partition"); ui.strong("sector size"); ui.end_row();
                    for part in p.partitions {
                        ui.label(part.label);
                        ui.label(part.sector_size.to_string());
                        ui.end_row();
                    }
                });
        } else {
            ui.label("Select a profile to see its details.");
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([760.0, 540.0])
            .with_title("phonesvc"),
        ..Default::default()
    };
    eframe::run_native(
        "phonesvc",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}