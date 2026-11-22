//! High-level service layer tying Sahara + Firehose together.
//!
//! A real Qualcomm service session is two phases on one USB link:
//!   1. **Sahara** — the device sends HELLO, the host uploads a Firehose
//!      programmer (`prog_firehose*.elf`) via READ_DATA/RAW_DATA.
//!   2. **Firehose** — with the programmer running, the host issues XML
//!      commands (configure, peek, poke, read, program, erase, reset, ...).
//!
//! These helpers are deliberately thin so both the GUI and the CLI share the
//! exact same wire behaviour. They operate over any [`Transport`].

use anyhow::{bail, Result};

use crate::firehose::{FirehoseResponse, FirehoseSession};
use crate::sahara::SaharaSession;
use crate::transport::Transport;

/// Default Qualcomm EDL bulk endpoints (interface 0).
pub const DEFAULT_OUT_EP: u8 = 0x02;
pub const DEFAULT_IN_EP: u8 = 0x81;

/// Run the Sahara handshake and stream `programmer` to the device.
///
/// On success the device is running the Firehose programmer and the same link
/// speaks Firehose. The transport is borrowed only for the duration of the
/// upload, so the caller can immediately wrap it in a [`FirehoseSession`].
pub fn upload_programmer<T: Transport>(t: &mut T, programmer: &[u8]) -> Result<SaharaSummary> {
    let mut s = SaharaSession::new(t);
    let hello = s.wait_hello()?;
    let summary = SaharaSummary {
        version: hello.version,
        compatible_version: hello.compatible_version,
        mode: hello.mode,
        image_id: hello.image_id,
        image_len: hello.image_len,
    };
    s.hello_response(&hello)?;
    s.upload(programmer)?;
    Ok(summary)
}

/// Summary of the device's Sahara HELLO, returned by [`upload_programmer`].
#[derive(Debug, Clone)]
pub struct SaharaSummary {
    pub version: u32,
    pub compatible_version: u32,
    pub mode: u32,
    pub image_id: u32,
    pub image_len: u32,
}

/// One-shot: open nothing, just upload + configure over an existing transport.
/// Returns the final configure response.
pub fn configure<T: Transport>(t: &mut T, usb_max_packet: u32) -> Result<FirehoseResponse> {
    let mut fh = FirehoseSession::new(t);
    fh.configure(usb_max_packet, true)
}

/// Read `num_sectors` sectors starting at `start_sector` and write them to
/// `out_file`. Returns the number of bytes written.
pub fn read_sectors<T: Transport>(
    t: &mut T,
    start_sector: u64,
    num_sectors: u64,
    out_file: &str,
) -> Result<u64> {
    let mut fh = FirehoseSession::new(t);
    fh.read(start_sector, num_sectors, out_file)
}

/// Program a local `filename` to the device at `start_sector`.
pub fn write_file<T: Transport, F: FnMut(u64, u64)>(
    t: &mut T,
    filename: &str,
    start_sector: u64,
    num_sectors: u64,
    file_size: u64,
    progress: Option<F>,
) -> Result<FirehoseResponse> {
    let mut fh = FirehoseSession::new(t);
    fh.program(filename, start_sector, num_sectors, file_size, progress)
}

/// Erase `num_sectors` starting at `start_sector`.
pub fn erase<T: Transport>(t: &mut T, start_sector: u64, num_sectors: u64) -> Result<FirehoseResponse> {
    let mut fh = FirehoseSession::new(t);
    fh.erase(start_sector, num_sectors)
}

/// Peek `size` bytes at `address`.
pub fn peek<T: Transport>(t: &mut T, address: u64, size: u32) -> Result<Vec<u8>> {
    let mut fh = FirehoseSession::new(t);
    fh.peek(address, size)
}

/// Poke `data` at `address`.
pub fn poke<T: Transport>(t: &mut T, address: u64, data: &[u8]) -> Result<FirehoseResponse> {
    let mut fh = FirehoseSession::new(t);
    fh.poke(address, data)
}

/// Reset the device.
pub fn reset<T: Transport>(t: &mut T) -> Result<FirehoseResponse> {
    let mut fh = FirehoseSession::new(t);
    fh.reset()
}

/// Query storage info for a physical partition (drive).
pub fn storage_info<T: Transport>(t: &mut T, drive: u32) -> Result<String> {
    let mut fh = FirehoseSession::new(t);
    fh.get_storage_info(drive)
}

/// Ensure a programmer image looks sane before we try to upload it.
pub fn validate_programmer(image: &[u8]) -> Result<()> {
    if image.is_empty() {
        bail!("programmer image is empty");
    }
    // Firehose programmers are ELF (or occasionally raw MBN). Warn-free accept:
    // we only reject clearly-too-small blobs that can't be a real loader.
    if image.len() < 1024 {
        bail!(
            "programmer image is only {} bytes — too small to be a Firehose loader",
            image.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sahara::{
        SAHARA_DONE_RESP, SAHARA_END_OF_IMAGE, SAHARA_HELLO, SAHARA_HELLO_RESPONSE,
        SAHARA_RAW_DATA, SAHARA_READ_DATA,
    };
    use crate::transport::MockTransport;
    use crate::firehose::FS;

    fn pkt(cmd: u32, payload: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&cmd.to_le_bytes());
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        v
    }

    fn hello_payload() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&SAHARA_HELLO.to_le_bytes());
        v.extend_from_slice(&2u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&13u32.to_le_bytes());
        v.extend_from_slice(&4096u32.to_le_bytes());
        v
    }

    fn read_data_payload(off: u32, len: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&SAHARA_READ_DATA.to_le_bytes());
        v.extend_from_slice(&13u32.to_le_bytes());
        v.extend_from_slice(&off.to_le_bytes());
        v.extend_from_slice(&len.to_le_bytes());
        v
    }

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
    fn rejects_bad_programmer() {
        assert!(validate_programmer(&[]).is_err());
        assert!(validate_programmer(&[0u8; 100]).is_err());
        assert!(validate_programmer(&vec![0u8; 4096]).is_ok());
    }

    #[test]
    fn upload_then_configure() {
        // Programmer is 4096 bytes; device reads it in two chunks.
        let programmer = vec![0xABu8; 4096];
        let mut script = Vec::new();
        script.extend(pkt(SAHARA_HELLO, &hello_payload()));
        script.extend(pkt(SAHARA_READ_DATA, &read_data_payload(0, 2048)));
        script.extend(pkt(SAHARA_DONE_RESP, &SAHARA_DONE_RESP.to_le_bytes()));
        script.extend(pkt(SAHARA_READ_DATA, &read_data_payload(2048, 2048)));
        script.extend(pkt(SAHARA_DONE_RESP, &SAHARA_DONE_RESP.to_le_bytes()));
        script.extend(pkt(SAHARA_END_OF_IMAGE, &SAHARA_END_OF_IMAGE.to_le_bytes()));
        // After Sahara completes, Firehose configure reply.
        script.extend(resp(0, "configure"));

        let mut t = MockTransport::new(script);
        let summary = upload_programmer(&mut t, &programmer).unwrap();
        assert_eq!(summary.image_id, 13);
        assert_eq!(summary.image_len, 4096);

        let r = configure(&mut t, 1 << 20).unwrap();
        assert!(r.ok());

        // The host must have written a HELLO_RESPONSE, RAW_DATA chunks, and a
        // configure command.
        let w = String::from_utf8_lossy(&t.written);
        assert!(w.contains("<configure"));
    }
}