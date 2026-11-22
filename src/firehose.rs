//! Qualcomm Firehose protocol (partition operations over the programmer).
//!
//! After a Firehose programmer is uploaded via Sahara, the device speaks
//! Firehose on the same link: the host sends `<xxx .../>` XML terminated by an
//! `FS` (0x1C) byte, and the device replies with a response plus, for bulk
//! reads, a length-prefixed raw-data phase.

use anyhow::{bail, Result};
use crate::transport::Transport;

pub const FS: u8 = 0x1C;

pub const FH_RAW_CLOSE: u32 = 0;
pub const FH_RAW_READ: u32 = 1;
pub const FH_RAW_WRITE: u32 = 2;

#[derive(Debug)]
pub struct FirehoseResponse {
    pub raw_mode: u32,
    pub raw_data: Vec<u8>,
    pub text: String,
}

impl FirehoseResponse {
    pub fn ok(&self) -> bool {
        // raw_mode 0 (close) with a non-empty/non-error body, or a raw mode.
        self.raw_mode != 0 || !self.text.is_empty()
    }
}

pub struct FirehoseSession<'a, T: Transport> {
    t: &'a mut T,
}

impl<'a, T: Transport> FirehoseSession<'a, T> {
    pub fn new(t: &'a mut T) -> Self {
        FirehoseSession { t }
    }

    fn send(&mut self, xml: &str) -> Result<()> {
        let mut buf = Vec::with_capacity(xml.len() + 1);
        buf.extend_from_slice(xml.as_bytes());
        buf.push(FS);
        self.t.write_all(&buf)?;
        Ok(())
    }

    /// Read bytes until (and including) the FS terminator; return the text.
    fn recv_until_fs(&mut self) -> Result<String> {
        let mut out = Vec::new();
        let mut b = [0u8; 1];
        loop {
            self.t.read_exact(&mut b)?;
            if b[0] == FS {
                break;
            }
            out.push(b[0]);
            if out.len() > 1 << 20 {
                bail!("Firehose response too long (missing FS?)");
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn parse_response(text: &str) -> (u32, String) {
        // The raw mode lives in the <?xml ... value="N" ...?> PI; fall back to
        // a <response value="..."> attribute, else 0.
        if let Some(idx) = text.find("value=\"") {
            let rest = &text[idx + 7..];
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(mode) = digits.parse::<u32>() {
                return (mode, text.to_string());
            }
        }
        (FH_RAW_CLOSE, text.to_string())
    }

    fn read_response(&mut self) -> Result<FirehoseResponse> {
        let text = self.recv_until_fs()?;
        let (raw_mode, text) = Self::parse_response(&text);
        let mut raw = Vec::new();
        if raw_mode == FH_RAW_READ {
            let mut lenb = [0u8; 4];
            self.t.read_exact(&mut lenb)?;
            let raw_len = u32::from_le_bytes(lenb) as usize;
            raw = vec![0u8; raw_len];
            self.t.read_exact(&mut raw)?;
            // consume trailing FS after the raw block
            let _ = self.recv_until_fs()?;
        }
        Ok(FirehoseResponse { raw_mode, raw_data: raw, text })
    }

    fn command(&mut self, xml: &str) -> Result<FirehoseResponse> {
        self.send(xml)?;
        let resp = self.read_response()?;
        if resp.raw_mode == FH_RAW_CLOSE && !resp.ok() {
            bail!("Firehose command failed: {}", xml);
        }
        Ok(resp)
    }

    // ---- commands ----

    pub fn configure(&mut self, usb_max_packet: u32, zlp: bool) -> Result<FirehoseResponse> {
        let z = if zlp { 1 } else { 0 };
        self.command(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <configure MemoryName=\"eMMC\" Verbose=\"0\" AlwaysValidate=\"0\" \
             MaxDigestTableSize=\"2048\" MaxPayloadSizeToTargetInBytes=\"{}\" \
             ZLPAwareHost=\"{}\" SkipZLPGuided=\"0\"/></data>",
            usb_max_packet, z
        ))
    }

    pub fn nop(&mut self) -> Result<FirehoseResponse> {
        self.command("<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data><nop/></data>")
    }

    pub fn peek(&mut self, address: u64, size: u32) -> Result<Vec<u8>> {
        let resp = self.command(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <peek address64=\"{}\" size_in_bytes=\"{}\"/></data>",
            address, size
        ))?;
        Ok(resp.raw_data)
    }

    pub fn poke(&mut self, address: u64, data: &[u8]) -> Result<FirehoseResponse> {
        let hexdata = hex::encode(data);
        self.command(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <poke address64=\"{}\" size_in_bytes=\"{}\" value64=\"{}\"/></data>",
            address,
            data.len(),
            hexdata
        ))
    }

    pub fn erase(&mut self, start_sector: u64, num_sectors: u64) -> Result<FirehoseResponse> {
        self.command(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <erase SECTOR_SIZE_IN_BYTES=\"512\" num_partition_sectors=\"{}\" \
             physical_partition_number=\"0\" start_sector=\"{}\"/></data>",
            num_sectors, start_sector
        ))
    }

    pub fn reset(&mut self) -> Result<FirehoseResponse> {
        self.command("<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data><reset value=\"true\"/></data>")
    }

    pub fn set_bootable_storage_drive(&mut self, drive: u32) -> Result<FirehoseResponse> {
        self.command(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <setbootablestoragedrive value=\"{}\"/></data>",
            drive
        ))
    }

    pub fn power(&mut self, value: &str) -> Result<FirehoseResponse> {
        self.command(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <power value=\"{}\"/></data>",
            value
        ))
    }

    pub fn patch(
        &mut self,
        filename: &str,
        start_sector: u64,
        byte_offset: u32,
        value: &str,
        size: u32,
    ) -> Result<FirehoseResponse> {
        self.command(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <patch SECTOR_SIZE_IN_BYTES=\"512\" byte_offset=\"{}\" filename=\"{}\" \
             physical_partition_number=\"0\" size_in_bytes=\"{}\" start_sector=\"{}\" \
             value=\"{}\"/></data>",
            byte_offset, filename, size, start_sector, value
        ))
    }

    pub fn get_storage_info(&mut self, drive: u32) -> Result<String> {
        self.send(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <getstorageinfo physical_partition_number=\"{}\"/></data>",
            drive
        ))?;
        Ok(self.recv_until_fs()?)
    }

    /// Read `num_sectors` 512-byte sectors starting at `start_sector` to a file.
    pub fn read(&mut self, start_sector: u64, num_sectors: u64, out_file: &str) -> Result<u64> {
        self.send(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <read SECTOR_SIZE_IN_BYTES=\"512\" num_partition_sectors=\"{}\" \
             physical_partition_number=\"0\" start_sector=\"{}\"/></data>",
            num_sectors, start_sector
        ))?;
        let resp = self.read_response()?;
        if resp.raw_mode != FH_RAW_READ {
            bail!("device did not enter raw-read mode (got {})", resp.raw_mode);
        }
        std::fs::write(out_file, &resp.raw_data)?;
        // consume the final ack
        let _ = self.read_response()?;
        Ok(resp.raw_data.len() as u64)
    }

    /// Write a local file to the device starting at `start_sector`.
    /// `progress(sent, total)` is called periodically if provided.
    pub fn program<F: FnMut(u64, u64)>(
        &mut self,
        filename: &str,
        start_sector: u64,
        num_sectors: u64,
        file_size: u64,
        mut progress: Option<F>,
    ) -> Result<FirehoseResponse> {
        self.send(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" ?><data>\
             <program SECTOR_SIZE_IN_BYTES=\"512\" num_partition_sectors=\"{}\" \
             physical_partition_number=\"0\" start_sector=\"{}\" filename=\"{}\" \
             file_size={} sparse=\"false\"/></data>",
            num_sectors, start_sector, filename, file_size
        ))?;
        let resp = self.read_response()?;
        if resp.raw_mode != FH_RAW_WRITE {
            bail!("device did not enter raw-write mode (got {})", resp.raw_mode);
        }
        let data = std::fs::read(filename)?;
        let total = data.len() as u64;
        let mut sent = 0u64;
        for chunk in data.chunks(1 << 20) {
            self.t.write_all(chunk)?;
            sent += chunk.len() as u64;
            if let Some(p) = progress.as_mut() {
                p(sent, total);
            }
        }
        let _ = file_size; // already declared in the command
        let final_resp = self.read_response()?;
        Ok(final_resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

    fn resp(raw_mode: u32, text: &str) -> Vec<u8> {
        let body = format!(
            "<?xml version=\"1.0\" value=\"{}\" ><response>{}</response>",
            raw_mode, text
        );
        let mut v = body.into_bytes();
        v.push(FS);
        v
    }

    #[test]
    fn configure_and_nop() {
        let mut script = Vec::new();
        script.extend(resp(0, "configure"));
        script.extend(resp(0, "nop"));
        let mut t = MockTransport::new(script);
        let mut s = FirehoseSession::new(&mut t);
        assert!(s.configure(0, false).unwrap().ok());
        assert!(s.nop().unwrap().ok());
    }

    #[test]
    fn peek_reads_raw() {
        let data: Vec<u8> = (0..16).collect();
        let mut script = Vec::new();
        script.extend(resp(FH_RAW_READ, "peek"));
        script.extend(&(data.len() as u32).to_le_bytes());
        script.extend(&data);
        script.push(FS);
        let mut t = MockTransport::new(script);
        let mut s = FirehoseSession::new(&mut t);
        let got = s.peek(0x1000, 16).unwrap();
        assert_eq!(got, data);
        assert!(String::from_utf8_lossy(&t.written).contains("<peek"));
    }

    #[test]
    fn program_streams_file() {
        let dir = std::env::temp_dir().join("fh_prog_test");
        std::fs::create_dir_all(&dir).unwrap();
        let fp = dir.join("in.bin");
        let payload: Vec<u8> = (0..5000).map(|i| (i % 251) as u8).collect();
        std::fs::write(&fp, &payload).unwrap();

        let mut script = Vec::new();
        script.extend(resp(FH_RAW_WRITE, "program"));
        script.extend(resp(0, "done"));
        let mut t = MockTransport::new(script);
        let mut s = FirehoseSession::new(&mut t);
        let mut calls = 0;
        let final_resp = s
            .program(
                fp.to_str().unwrap(),
                0,
                10,
                payload.len() as u64,
                Some(|_s, _t| calls += 1),
            )
            .unwrap();
        assert!(final_resp.ok());
        assert!(t.written.windows(payload.len()).any(|w| w == &payload[..]));
        assert!(calls >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn erase_and_reset() {
        let mut script = Vec::new();
        script.extend(resp(0, "erase"));
        let mut t = MockTransport::new(script);
        let mut s = FirehoseSession::new(&mut t);
        assert!(s.erase(100, 50).unwrap().ok());

        let mut script = Vec::new();
        script.extend(resp(0, "reset"));
        let mut t = MockTransport::new(script);
        let mut s = FirehoseSession::new(&mut t);
        assert!(s.reset().unwrap().ok());
    }
}