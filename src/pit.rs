//! Samsung PIT (Partition Information Table) parser.
//!
//! Little-endian. Header is 12 bytes (magic + count + unknown); entries are
//! 132 bytes each and follow immediately.

use anyhow::{bail, Result};

pub const PIT_MAGIC: u32 = 0x12349876;
pub const PIT_HEADER_SIZE: usize = 12;
pub const PIT_ENTRY_SIZE: usize = 132;

#[derive(Debug, Clone)]
pub struct PitEntry {
    pub index: usize,
    pub bin_type: u8,
    pub device_type: u8,
    pub unique_id: u8,
    pub update_flag: u8,
    pub block_size: u32,
    pub block_count: u32,
    pub file_offset: u32,
    pub partition_flags: u32,
    pub attribute: u32,
    pub name: String,
    pub fs_type: String,
    pub info: String,
}

impl PitEntry {
    pub fn size_bytes(&self) -> u64 {
        self.block_size as u64 * self.block_count as u64
    }
}

fn rd_u32(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn cstr(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    String::from_utf8_lossy(&raw[..end]).into_owned()
}

pub fn parse_pit(data: &[u8]) -> Result<Vec<PitEntry>> {
    if data.len() < PIT_HEADER_SIZE {
        bail!("buffer too small for PIT header: {} bytes", data.len());
    }
    let magic = rd_u32(data, 0);
    if magic != PIT_MAGIC {
        bail!("bad PIT magic 0x{:08x} (expected 0x{:08x})", magic, PIT_MAGIC);
    }
    let count = rd_u32(data, 4) as usize;
    let needed = PIT_HEADER_SIZE + count * PIT_ENTRY_SIZE;
    if data.len() < needed {
        bail!(
            "PIT truncated: header declares {} entries (need {} bytes, have {})",
            count,
            needed,
            data.len()
        );
    }
    let mut entries = Vec::with_capacity(count);
    let mut off = PIT_HEADER_SIZE;
    for i in 0..count {
        let e = &data[off..off + PIT_ENTRY_SIZE];
        entries.push(PitEntry {
            index: i,
            bin_type: e[0],
            device_type: e[1],
            unique_id: e[2],
            update_flag: e[3],
            block_size: rd_u32(e, 4),
            block_count: rd_u32(e, 8),
            file_offset: rd_u32(e, 12),
            partition_flags: rd_u32(e, 16),
            attribute: rd_u32(e, 20),
            name: cstr(&e[40..40 + 32]),
            fs_type: cstr(&e[72..72 + 32]),
            info: cstr(&e[108..108 + 16]),
        });
        off += PIT_ENTRY_SIZE;
    }
    Ok(entries)
}

pub fn find_entry<'a>(entries: &'a [PitEntry], name: &str) -> Option<&'a PitEntry> {
    let lname = name.to_lowercase();
    entries.iter().find(|e| e.name.to_lowercase() == lname)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(bin: u8, dev: u8, uid: u8, upd: u8, bs: u32, bc: u32, fo: u32, name: &str, fs: &str) -> Vec<u8> {
        let mut e = vec![0u8; PIT_ENTRY_SIZE];
        e[0] = bin;
        e[1] = dev;
        e[2] = uid;
        e[3] = upd;
        e[4..8].copy_from_slice(&bs.to_le_bytes());
        e[8..12].copy_from_slice(&bc.to_le_bytes());
        e[12..16].copy_from_slice(&fo.to_le_bytes());
        e[40..40 + name.len()].copy_from_slice(name.as_bytes());
        e[72..72 + fs.len()].copy_from_slice(fs.as_bytes());
        e
    }

    fn build() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&PIT_MAGIC.to_le_bytes());
        v.extend_from_slice(&5u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend(entry(1, 1, 1, 1, 0x200000, 100, 0, "IBL+PBL", "emmc"));
        v.extend(entry(1, 1, 2, 1, 0x200000, 200, 100, "PIT", "emmc"));
        v.extend(entry(1, 1, 3, 1, 0x200000, 400, 300, "EFS", "ext4"));
        v.extend(entry(1, 1, 4, 1, 0x200000, 1000, 700, "SYSTEM", "ext4"));
        v.extend(entry(1, 1, 5, 1, 0x200000, 5000, 1700, "USERDATA", "ext4"));
        v
    }

    #[test]
    fn parses_pit() {
        let entries = parse_pit(&build()).unwrap();
        assert_eq!(entries.len(), 5);
        let by: std::collections::HashMap<_, _> =
            entries.iter().map(|e| (e.name.as_str(), e)).collect();
        assert_eq!(by["SYSTEM"].block_count, 1000);
        assert_eq!(by["SYSTEM"].block_size, 0x200000);
        assert_eq!(by["SYSTEM"].device_type, 1);
        assert_eq!(by["USERDATA"].size_bytes(), 0x200000 * 5000);
        assert_eq!(by["EFS"].fs_type, "ext4");
    }

    #[test]
    fn finds_entry() {
        let entries = parse_pit(&build()).unwrap();
        assert_eq!(find_entry(&entries, "system").unwrap().name, "SYSTEM");
        assert!(find_entry(&entries, "nope").is_none());
    }

    #[test]
    fn rejects_bad_magic() {
        assert!(parse_pit(&[0u8; 20]).is_err());
    }

    #[test]
    fn rejects_truncated() {
        let mut v = Vec::new();
        v.extend_from_slice(&PIT_MAGIC.to_le_bytes());
        v.extend_from_slice(&5u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&[0u8; 10]);
        assert!(parse_pit(&v).is_err());
    }
}