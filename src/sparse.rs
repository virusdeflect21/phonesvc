//! Android sparse image format and conversion to raw.
//!
//! Sparse images omit runs of zero blocks. This module parses the header and
//! chunks and expands them to a raw image (zeros for DONT_CARE).

use anyhow::{bail, Result};

pub const SPARSE_MAGIC: u32 = 0xED26FF3A;

pub const CHUNK_TYPE_RAW: u16 = 0xCAC1;
pub const CHUNK_TYPE_FILL: u16 = 0xCAC2;
pub const CHUNK_TYPE_DONT_CARE: u16 = 0xCAC3;
pub const CHUNK_TYPE_CRC32: u16 = 0xCAC4;

const SPARSE_HEADER_SIZE: usize = 28;
const CHUNK_HEADER_SIZE: usize = 12;

#[derive(Debug, Clone)]
pub struct SparseHeader {
    pub blk_sz: u32,
    pub total_blks: u32,
    pub total_chunks: u32,
    pub file_hdr_sz: u32,
    pub chunk_hdr_sz: u32,
}

impl SparseHeader {
    pub fn raw_size(&self) -> u64 {
        self.total_blks as u64 * self.blk_sz as u64
    }
}

fn rd_u16(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

pub fn is_sparse(data: &[u8]) -> bool {
    data.len() >= 4 && rd_u32(data, 0) == SPARSE_MAGIC
}

pub fn parse_header(data: &[u8]) -> Result<SparseHeader> {
    if data.len() < SPARSE_HEADER_SIZE {
        bail!("buffer too small for sparse header: {} bytes", data.len());
    }
    let magic = rd_u32(data, 0);
    if magic != SPARSE_MAGIC {
        bail!("bad sparse magic 0x{:08x}", magic);
    }
    let major = rd_u16(data, 4);
    if major != 1 {
        bail!("unsupported sparse major version {}", major);
    }
    // Layout (AOSP sparse_header_t, 28 bytes):
    //   +8 file_hdr_sz (u16), +10 chunk_hdr_sz (u16), +12 blk_sz (u32),
    //   +16 total_blks (u32), +20 total_chunks (u32), +24 image_checksum.
    let file_hdr_sz = rd_u16(data, 8) as u32;
    let chunk_hdr_sz = rd_u16(data, 10) as u32;
    let blk_sz = rd_u32(data, 12);
    if blk_sz == 0 {
        bail!("sparse block size is zero");
    }
    Ok(SparseHeader {
        blk_sz,
        total_blks: rd_u32(data, 16),
        total_chunks: rd_u32(data, 20),
        file_hdr_sz,
        chunk_hdr_sz,
    })
}

/// Convert a full sparse image (in memory) to a raw image.
pub fn convert_to_raw(data: &[u8]) -> Result<Vec<u8>> {
    let hdr = parse_header(data)?;
    let mut out: Vec<u8> = Vec::with_capacity(hdr.raw_size() as usize);
    let mut off = hdr.file_hdr_sz as usize;
    for _ in 0..hdr.total_chunks {
        if off + CHUNK_HEADER_SIZE > data.len() {
            bail!("truncated chunk header");
        }
        let chunk_type = rd_u16(data, off);
        let chunk_sz = rd_u32(data, off + 4); // in blocks
        let total_sz = rd_u32(data, off + 8);
        if (total_sz as usize) < hdr.chunk_hdr_sz as usize {
            bail!("invalid chunk total_sz {}", total_sz);
        }
        let payload = &data[off + hdr.chunk_hdr_sz as usize..off + total_sz as usize];
        let nbytes = chunk_sz as usize * hdr.blk_sz as usize;
        match chunk_type {
            CHUNK_TYPE_RAW => {
                if payload.len() < nbytes {
                    bail!("RAW chunk payload shorter than declared");
                }
                out.extend_from_slice(&payload[..nbytes]);
            }
            CHUNK_TYPE_FILL => {
                if payload.len() < 4 {
                    bail!("FILL chunk payload shorter than 4 bytes");
                }
                let fill = &payload[..4];
                for _ in 0..(nbytes / 4) {
                    out.extend_from_slice(fill);
                }
            }
            CHUNK_TYPE_DONT_CARE => {
                out.resize(out.len() + nbytes, 0);
            }
            CHUNK_TYPE_CRC32 => {
                // checksum only, no output
            }
            other => bail!("unknown chunk type 0x{:x}", other),
        }
        off += total_sz as usize;
    }
    Ok(out)
}

/// Convert an on-disk sparse image file to a raw image file. Returns bytes written.
pub fn sparse_to_raw_file(sparse_path: &str, raw_path: &str) -> Result<u64> {
    let data = std::fs::read(sparse_path)?;
    let raw = convert_to_raw(&data)?;
    std::fs::write(raw_path, &raw)?;
    Ok(raw.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(ctype: u16, sz_blocks: u32, payload: &[u8]) -> Vec<u8> {
        let total = CHUNK_HEADER_SIZE + payload.len();
        let mut v = Vec::new();
        v.extend_from_slice(&ctype.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // reserved
        v.extend_from_slice(&sz_blocks.to_le_bytes());
        v.extend_from_slice(&(total as u32).to_le_bytes());
        v.extend_from_slice(payload);
        v
    }

    fn build() -> Vec<u8> {
        let blk = 4096u32;
        let raw0 = vec![0xAAu8; (2 * blk) as usize];
        let raw3 = vec![0xBBu8; blk as usize];
        let mut body = Vec::new();
        body.extend(chunk(CHUNK_TYPE_RAW, 2, &raw0));
        body.extend(chunk(CHUNK_TYPE_FILL, 3, &0x11223344u32.to_le_bytes()));
        body.extend(chunk(CHUNK_TYPE_DONT_CARE, 4, &[]));
        body.extend(chunk(CHUNK_TYPE_RAW, 1, &raw3));
        body.extend(chunk(CHUNK_TYPE_CRC32, 0, &0xDEADBEEFu32.to_le_bytes()));
        let total_blks: u32 = 2 + 3 + 4 + 1;
        let mut hdr = Vec::new();
        hdr.extend_from_slice(&SPARSE_MAGIC.to_le_bytes());
        hdr.extend_from_slice(&1u16.to_le_bytes()); // major
        hdr.extend_from_slice(&0u16.to_le_bytes()); // minor
        hdr.extend_from_slice(&28u16.to_le_bytes()); // file_hdr_sz
        hdr.extend_from_slice(&12u16.to_le_bytes()); // chunk_hdr_sz
        hdr.extend_from_slice(&blk.to_le_bytes());
        hdr.extend_from_slice(&total_blks.to_le_bytes());
        hdr.extend_from_slice(&5u32.to_le_bytes()); // total_chunks
        hdr.extend_from_slice(&0u32.to_le_bytes()); // checksum
        hdr.extend(body);
        hdr
    }

    #[test]
    fn converts_all_chunk_types() {
        let sparse = build();
        let blk = 4096usize;
        assert!(is_sparse(&sparse));
        let h = parse_header(&sparse).unwrap();
        assert_eq!(h.blk_sz, 4096);
        assert_eq!(h.total_blks, 10);

        let raw = convert_to_raw(&sparse).unwrap();
        assert_eq!(raw.len(), 10 * blk);
        assert_eq!(&raw[0..2 * blk], &vec![0xAAu8; 2 * blk]);
        assert_eq!(&raw[2 * blk..5 * blk], &0x11223344u32.to_le_bytes().repeat(3 * blk / 4));
        assert_eq!(&raw[5 * blk..9 * blk], &vec![0u8; 4 * blk]);
        assert_eq!(&raw[9 * blk..10 * blk], &vec![0xBBu8; blk]);
    }
}