//! GPT (GUID Partition Table) parser.
//!
//! Parses the protective-MBR + GPT header + partition-entry array from a disk
//! image (starting at LBA 0) and enumerates partitions.

use anyhow::{bail, Result};

pub const SECTOR_SIZE: usize = 512;
const PART_ENTRY_LBA: usize = 2; // entries begin at LBA 2 (after MBR + header)
const PART_ENTRY_SIZE: usize = 128;
const NUM_PART_ENTRIES: usize = 128;
const GPT_MAGIC: &[u8; 8] = b"EFI PART";

#[derive(Debug, Clone)]
pub struct GptHeader {
    pub disk_guid: String,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub part_entry_lba: u64,
    pub num_part_entries: u32,
}

#[derive(Debug, Clone)]
pub struct GptPartition {
    pub index: usize,
    pub type_guid: String,
    pub first_lba: u64,
    pub last_lba: u64,
    pub name: String,
}

impl GptPartition {
    pub fn num_sectors(&self) -> u64 {
        self.last_lba.saturating_sub(self.first_lba) + 1
    }
    pub fn size_bytes(&self) -> u64 {
        self.num_sectors() * SECTOR_SIZE as u64
    }
}

fn rd_u32(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}
fn rd_u64(d: &[u8], off: usize) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[off..off + 8]);
    u64::from_le_bytes(b)
}

/// Format a GUID from its on-disk little-endian byte representation.
fn format_guid(raw: &[u8]) -> String {
    if raw.len() < 16 {
        return String::new();
    }
    let d1 = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let d2 = u16::from_le_bytes([raw[4], raw[5]]);
    let d3 = u16::from_le_bytes([raw[6], raw[7]]);
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        d1, d2, d3, raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15]
    )
}

fn parse_name(raw: &[u8]) -> String {
    // UTF-16LE, NUL-terminated, up to 36 chars (72 bytes).
    let mut units = Vec::new();
    let mut i = 0;
    while i + 1 < raw.len() {
        let u = u16::from_le_bytes([raw[i], raw[i + 1]]);
        if u == 0 {
            break;
        }
        units.push(u);
        i += 2;
    }
    String::from_utf16_lossy(&units)
}

/// Parse a GPT image. `data` must start at LBA 0 and cover the header plus the
/// full partition-entry array.
pub fn parse_gpt(data: &[u8]) -> Result<(GptHeader, Vec<GptPartition>)> {
    if data.len() < SECTOR_SIZE * 2 {
        bail!("image too small for GPT header");
    }
    // Header lives in LBA 1.
    let hdr = &data[SECTOR_SIZE..SECTOR_SIZE * 2];
    if &hdr[0..8] != GPT_MAGIC {
        bail!("bad GPT magic (not an EFI PART header)");
    }
    let part_entry_lba = rd_u64(hdr, 72);
    let num_part_entries = rd_u32(hdr, 80);
    let first_usable_lba = rd_u64(hdr, 40);
    let last_usable_lba = rd_u64(hdr, 48);
    let disk_guid = format_guid(&hdr[56..72]);

    let header = GptHeader {
        disk_guid,
        first_usable_lba,
        last_usable_lba,
        part_entry_lba,
        num_part_entries,
    };

    // Partition entries start at LBA `part_entry_lba` (normally 2).
    let entry_start = (part_entry_lba as usize) * SECTOR_SIZE;
    let count = num_part_entries.min(NUM_PART_ENTRIES as u32) as usize;
    let need = entry_start + count * PART_ENTRY_SIZE;
    if data.len() < need {
        bail!(
            "image truncated: need {} bytes for {} entries, have {}",
            need,
            count,
            data.len()
        );
    }

    let mut parts = Vec::new();
    for i in 0..count {
        let off = entry_start + i * PART_ENTRY_SIZE;
        let e = &data[off..off + PART_ENTRY_SIZE];
        let type_guid = format_guid(&e[0..16]);
        // An all-zero type GUID marks an unused entry.
        if type_guid == "00000000-0000-0000-0000-000000000000" {
            continue;
        }
        let first_lba = rd_u64(e, 32);
        let last_lba = rd_u64(e, 40);
        let name = parse_name(&e[56..56 + 72]);
        parts.push(GptPartition {
            index: i,
            type_guid,
            first_lba,
            last_lba,
            name,
        });
    }

    // Silence unused warning for the constant in release builds.
    let _ = PART_ENTRY_LBA;
    Ok((header, parts))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guid(s: &str) -> [u8; 16] {
        // Parse a GUID string into little-endian on-disk bytes.
        let hex: String = s.chars().filter(|c| *c != '-').collect();
        let b = hex::decode(&hex).unwrap();
        let mut out = [0u8; 16];
        // first three groups are little-endian on disk
        out[0..4].copy_from_slice(&b[0..4]);
        out[0] = b[3];
        out[1] = b[2];
        out[2] = b[1];
        out[3] = b[0];
        out[4] = b[5];
        out[5] = b[4];
        out[6] = b[7];
        out[7] = b[6];
        out[8..16].copy_from_slice(&b[8..16]);
        out
    }

    fn build() -> Vec<u8> {
        let mut img = vec![0u8; SECTOR_SIZE * (PART_ENTRY_LBA + 33)];
        // header at LBA 1
        let h = SECTOR_SIZE;
        img[h..h + 8].copy_from_slice(GPT_MAGIC);
        img[h + 40..h + 48].copy_from_slice(&34u64.to_le_bytes());
        img[h + 48..h + 56].copy_from_slice(&500099u64.to_le_bytes());
        img[h + 56..h + 72].copy_from_slice(&guid("12345678-1234-5678-1234-567812345678"));
        img[h + 72..h + 80].copy_from_slice(&2u64.to_le_bytes());
        img[h + 80..h + 84].copy_from_slice(&128u32.to_le_bytes());
        let parts = [
            ("modem", "4E36E6B4-1F5A-4C21-9A3E-8B2D1C3E4F50", 34u64, 100u64),
            ("boot", "BD8CE2A3-4F5B-4C21-8A3E-1B2D3C4E5F60", 101, 200),
            ("system", "BD8CE2A3-4F5B-4C21-8A3E-1B2D3C4E5F60", 201, 1000),
            ("userdata", "BD8CE2A3-4F5B-4C21-8A3E-1B2D3C4E5F60", 1001, 500000),
        ];
        let base = PART_ENTRY_LBA * SECTOR_SIZE;
        for (i, (name, typ, first, last)) in parts.iter().enumerate() {
            let off = base + i * PART_ENTRY_SIZE;
            img[off..off + 16].copy_from_slice(&guid(typ));
            img[off + 32..off + 40].copy_from_slice(&first.to_le_bytes());
            img[off + 40..off + 48].copy_from_slice(&last.to_le_bytes());
            let nm: Vec<u16> = name.encode_utf16().collect();
            for (j, u) in nm.iter().enumerate() {
                img[off + 56 + j * 2..off + 56 + j * 2 + 2].copy_from_slice(&u.to_le_bytes());
            }
        }
        img
    }

    #[test]
    fn parses_gpt() {
        let (hdr, parts) = parse_gpt(&build()).unwrap();
        assert_eq!(hdr.disk_guid, "12345678-1234-5678-1234-567812345678");
        assert_eq!(parts.len(), 4);
        let by: std::collections::HashMap<_, _> =
            parts.iter().map(|p| (p.name.as_str(), p)).collect();
        assert_eq!(by["modem"].num_sectors(), 100 - 34 + 1);
        assert_eq!(by["userdata"].size_bytes(), (500000 - 1001 + 1) * 512);
        assert_eq!(by["system"].type_guid, "bd8ce2a3-4f5b-4c21-8a3e-1b2d3c4e5f60");
    }

    #[test]
    fn rejects_bad_magic() {
        let mut img = build();
        let h = SECTOR_SIZE;
        img[h..h + 8].copy_from_slice(b"NOTGPT!!");
        assert!(parse_gpt(&img).is_err());
    }
}