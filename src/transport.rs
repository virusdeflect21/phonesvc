//! Abstract byte transport used by the Sahara and Firehose protocols.
//!
//! The real implementation is backed by libusb bulk transfers (see `usb.rs`);
//! tests use an in-memory mock so the protocol logic is verifiable without a
//! physical device.

use anyhow::Result;

/// A bidirectional byte stream to a device.
pub trait Transport {
    /// Write all bytes, returning how many were written.
    fn write_all(&mut self, data: &[u8]) -> Result<usize>;

    /// Read up to `buf.len()` bytes, returning how many were read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Read exactly `buf.len()` bytes (looping until full).
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        let mut off = 0;
        while off < buf.len() {
            let n = self.read(&mut buf[off..])?;
            if n == 0 {
                anyhow::bail!("device closed while reading");
            }
            off += n;
        }
        Ok(())
    }

    /// Set the read timeout in milliseconds.
    fn set_read_timeout_ms(&mut self, _ms: u32) {}
}

/// A scripted in-memory transport for tests: plays back a fixed byte stream
/// and records everything the host writes.
#[cfg(test)]
pub struct MockTransport {
    inbuf: Vec<u8>,
    pos: usize,
    pub written: Vec<u8>,
}

#[cfg(test)]
impl MockTransport {
    pub fn new(script: Vec<u8>) -> Self {
        MockTransport { inbuf: script, pos: 0, written: Vec::new() }
    }
}

#[cfg(test)]
impl Transport for MockTransport {
    fn write_all(&mut self, data: &[u8]) -> Result<usize> {
        self.written.extend_from_slice(data);
        Ok(data.len())
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let remaining = self.inbuf.len() - self.pos;
        let n = remaining.min(buf.len());
        buf[..n].copy_from_slice(&self.inbuf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}