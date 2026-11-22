//! Qualcomm Sahara protocol (EDL loader upload), host side.
//!
//! Runs over any `Transport`. After reset into 9008 (EDL) mode the device
//! speaks Sahara: it sends HELLO, the host replies HELLO_RESPONSE, then the
//! device repeatedly sends READ_DATA requests and the host streams the
//! programmer image as RAW_DATA packets until END_OF_IMAGE.

use anyhow::{bail, Result};
use crate::transport::Transport;

pub const SAHARA_HELLO: u32 = 1;
pub const SAHARA_HELLO_RESPONSE: u32 = 2;
pub const SAHARA_READ_DATA: u32 = 3;
pub const SAHARA_END_OF_IMAGE: u32 = 4;
pub const SAHARA_DONE_RESP: u32 = 5;
pub const SAHARA_RAW_DATA: u32 = 6;

const HEADER_SIZE: usize = 8;

#[derive(Debug, Clone)]
pub struct SaharaHello {
    pub version: u32,
    pub compatible_version: u32,
    pub mode: u32,
    pub image_id: u32,
    pub image_len: u32,
}

#[derive(Debug, Clone)]
pub struct SaharaReadData {
    pub image_id: u32,
    pub data_offset: u32,
    pub data_len: u32,
}

pub struct SaharaSession<'a, T: Transport> {
    t: &'a mut T,
}

impl<'a, T: Transport> SaharaSession<'a, T> {
    pub fn new(t: &'a mut T) -> Self {
        SaharaSession { t }
    }

    fn read_packet(&mut self) -> Result<(u32, Vec<u8>)> {
        let mut hdr = [0u8; HEADER_SIZE];
        self.t.read_exact(&mut hdr)?;
        let cmd = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        let len = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
        let mut payload = vec![0u8; len];
        self.t.read_exact(&mut payload)?;
        Ok((cmd, payload))
    }

    fn write_packet(&mut self, cmd: u32, payload: &[u8]) -> Result<()> {
        let mut buf = Vec::with_capacity(HEADER_SIZE + payload.len());
        buf.extend_from_slice(&cmd.to_le_bytes());
        buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buf.extend_from_slice(payload);
        self.t.write_all(&buf)?;
        Ok(())
    }

    fn parse_hello(payload: &[u8]) -> Result<SaharaHello> {
        if payload.len() < 24 {
            bail!("Sahara hello payload too short: {} bytes", payload.len());
        }
        let g = |o: usize| u32::from_le_bytes([payload[o], payload[o + 1], payload[o + 2], payload[o + 3]]);
        // payload[0..4] is the command echo; fields follow.
        Ok(SaharaHello {
            version: g(4),
            compatible_version: g(8),
            mode: g(12),
            image_id: g(16),
            image_len: g(20),
        })
    }

    fn parse_read_data(payload: &[u8]) -> Result<SaharaReadData> {
        if payload.len() < 16 {
            bail!("Sahara read-data payload too short: {} bytes", payload.len());
        }
        let g = |o: usize| u32::from_le_bytes([payload[o], payload[o + 1], payload[o + 2], payload[o + 3]]);
        Ok(SaharaReadData {
            image_id: g(4),
            data_offset: g(8),
            data_len: g(12),
        })
    }

    /// Wait for the device's HELLO packet.
    pub fn wait_hello(&mut self) -> Result<SaharaHello> {
        let (cmd, payload) = self.read_packet()?;
        if cmd != SAHARA_HELLO {
            bail!("expected Sahara HELLO (1), got command {}", cmd);
        }
        Self::parse_hello(&payload)
    }

    /// Send HELLO_RESPONSE acknowledging the device's hello.
    pub fn hello_response(&mut self, hello: &SaharaHello) -> Result<()> {
        // version=2, compatible=echo, mode=0 (IMAGE_TX_PENDING), id/len echo.
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&2u32.to_le_bytes()); // host version
        payload.extend_from_slice(&hello.compatible_version.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes()); // mode: image tx pending
        payload.extend_from_slice(&hello.image_id.to_le_bytes());
        payload.extend_from_slice(&hello.image_len.to_le_bytes());
        // size/flags word
        payload.extend_from_slice(&0u32.to_le_bytes());
        self.write_packet(SAHARA_HELLO_RESPONSE, &payload)
    }

    /// Stream the programmer image in response to READ_DATA requests until the
    /// device has the whole image, then signal END_OF_IMAGE.
    pub fn upload(&mut self, image: &[u8]) -> Result<()> {
        loop {
            let (cmd, payload) = self.read_packet()?;
            match cmd {
                SAHARA_READ_DATA => {
                    let rd = Self::parse_read_data(&payload)?;
                    let off = rd.data_offset as usize;
                    let len = rd.data_len as usize;
                    if off + len > image.len() {
                        bail!(
                            "device requested bytes past image end (offset {} len {}, image {})",
                            off,
                            len,
                            image.len()
                        );
                    }
                    let chunk = &image[off..off + len];
                    let mut raw = Vec::with_capacity(len);
                    raw.extend_from_slice(chunk);
                    self.write_packet(SAHARA_RAW_DATA, &raw)?;
                }
                SAHARA_END_OF_IMAGE => {
                    // Acknowledge end-of-image.
                    let mut payload = Vec::new();
                    payload.extend_from_slice(&SAHARA_END_OF_IMAGE.to_le_bytes());
                    self.write_packet(SAHARA_END_OF_IMAGE, &payload)?;
                    break;
                }
                SAHARA_DONE_RESP => {
                    // Per-chunk ack; nothing to do, keep reading.
                }
                other => bail!("unexpected Sahara command {} during upload", other),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;

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
        v.extend_from_slice(&2u32.to_le_bytes()); // version
        v.extend_from_slice(&1u32.to_le_bytes()); // compatible
        v.extend_from_slice(&0u32.to_le_bytes()); // mode
        v.extend_from_slice(&13u32.to_le_bytes()); // image_id
        v.extend_from_slice(&4096u32.to_le_bytes()); // image_len
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

    #[test]
    fn parses_hello() {
        let mut t = MockTransport::new(pkt(SAHARA_HELLO, &hello_payload()));
        let mut s = SaharaSession::new(&mut t);
        let h = s.wait_hello().unwrap();
        assert_eq!(h.version, 2);
        assert_eq!(h.compatible_version, 1);
        assert_eq!(h.image_id, 13);
        assert_eq!(h.image_len, 4096);
    }

    #[test]
    fn full_upload_flow() {
        let image: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
        let mut script = Vec::new();
        script.extend(pkt(SAHARA_HELLO, &hello_payload()));
        script.extend(pkt(SAHARA_READ_DATA, &read_data_payload(0, 1024)));
        script.extend(pkt(SAHARA_DONE_RESP, &SAHARA_DONE_RESP.to_le_bytes()));
        script.extend(pkt(SAHARA_READ_DATA, &read_data_payload(1024, 4096 - 1024)));
        script.extend(pkt(SAHARA_DONE_RESP, &SAHARA_DONE_RESP.to_le_bytes()));
        script.extend(pkt(SAHARA_END_OF_IMAGE, &SAHARA_END_OF_IMAGE.to_le_bytes()));

        let mut t = MockTransport::new(script);
        let mut s = SaharaSession::new(&mut t);
        let h = s.wait_hello().unwrap();
        s.hello_response(&h).unwrap();
        s.upload(&image).unwrap();

        // Walk what the host wrote: expect HELLO_RESPONSE first, then RAW_DATA
        // chunks reassembling to the full image, then END_OF_IMAGE.
        let w = &t.written;
        let mut off = 0;
        let mut cmds = Vec::new();
        let mut reassembled = Vec::new();
        while off + HEADER_SIZE <= w.len() {
            let cmd = u32::from_le_bytes([w[off], w[off + 1], w[off + 2], w[off + 3]]);
            let len = u32::from_le_bytes([w[off + 4], w[off + 5], w[off + 6], w[off + 7]]) as usize;
            let payload = &w[off + HEADER_SIZE..off + HEADER_SIZE + len];
            cmds.push(cmd);
            if cmd == SAHARA_RAW_DATA {
                reassembled.extend_from_slice(payload);
            }
            off += HEADER_SIZE + len;
        }
        assert_eq!(cmds[0], SAHARA_HELLO_RESPONSE);
        assert!(cmds.contains(&SAHARA_RAW_DATA));
        assert_eq!(reassembled, image);
    }
}