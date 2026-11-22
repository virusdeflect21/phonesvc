//! Qualcomm rawprogram XML parser.
//!
//! Qualcomm firmware (QFIL/QPST) ships `rawprogram0.xml`..`rawprogramN.xml`
//! describing how to program a device. Each contains `<program>` and `<erase>`
//! elements. This module extracts them into `RawEntry` records.
//!
//! A tiny attribute scanner is used instead of a full XML crate because the
//! format is simple and machine-generated (`key="value"` pairs per tag).

use anyhow::{bail, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryKind {
    Program,
    Erase,
}

#[derive(Debug, Clone)]
pub struct RawEntry {
    pub kind: EntryKind,
    pub label: String,
    pub filename: String,
    pub start_sector: u64,
    pub num_sectors: u64,
    pub sector_size: u32,
    pub physical_partition: u32,
    pub sparse: bool,
    pub file_sector_offset: u64,
}

impl RawEntry {
    pub fn size_bytes(&self) -> u64 {
        self.num_sectors * self.sector_size as u64
    }
}

/// Extract `key="value"` pairs from a single tag's attribute string.
fn extract_attrs(s: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // find a key: [A-Za-z_][A-Za-z0-9_]*
        if (bytes[i] as char).is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len()
                && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            let key = &s[start..i];
            // skip whitespace, expect '='
            while i < bytes.len() && (bytes[i] as char).is_whitespace() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'=' {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_whitespace() {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'"' {
                    i += 1;
                    let vstart = i;
                    while i < bytes.len() && bytes[i] != b'"' {
                        i += 1;
                    }
                    out.insert(key.to_string(), s[vstart..i].to_string());
                    if i < bytes.len() {
                        i += 1; // closing quote
                    }
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

fn attr_u64(attrs: &BTreeMap<String, String>, key: &str, default: u64) -> u64 {
    attrs.get(key).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn attr_bool(attrs: &BTreeMap<String, String>, key: &str) -> bool {
    match attrs.get(key).map(|v| v.to_lowercase()) {
        Some(v) => v == "true" || v == "1",
        None => false,
    }
}

/// Parse the `<program>`/`<erase>` elements out of one XML document string.
pub fn parse_rawprogram_str(xml: &str) -> Vec<RawEntry> {
    let mut entries = Vec::new();
    let bytes = xml.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'<' {
            // read tag name
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] as char).is_ascii_alphabetic() {
                j += 1;
            }
            let tag = &xml[i + 1..j];
            // find end of tag '>'
            let mut k = j;
            while k < bytes.len() && bytes[k] != b'>' {
                k += 1;
            }
            let tag_inner = &xml[j..k];
            let is_program = tag.eq_ignore_ascii_case("program");
            let is_erase = tag.eq_ignore_ascii_case("erase");
            if is_program || is_erase {
                let attrs = extract_attrs(tag_inner);
                let kind = if is_program { EntryKind::Program } else { EntryKind::Erase };
                entries.push(RawEntry {
                    kind,
                    label: attrs.get("label").cloned().unwrap_or_default(),
                    filename: attrs.get("filename").cloned().unwrap_or_default(),
                    start_sector: attr_u64(&attrs, "start_sector", 0),
                    num_sectors: attr_u64(&attrs, "num_partition_sectors", 0),
                    sector_size: attr_u64(&attrs, "SECTOR_SIZE_IN_BYTES", 512) as u32,
                    physical_partition: attr_u64(&attrs, "physical_partition_number", 0) as u32,
                    sparse: attr_bool(&attrs, "sparse"),
                    file_sector_offset: attr_u64(&attrs, "file_sector_offset", 0),
                });
            }
            i = if k < bytes.len() { k + 1 } else { bytes.len() };
        } else {
            i += 1;
        }
    }
    entries
}

pub fn parse_rawprogram_file(path: &str) -> Result<Vec<RawEntry>> {
    let xml = std::fs::read_to_string(path)?;
    Ok(parse_rawprogram_str(&xml))
}

/// Parse all rawprogram*.xml files in a directory, in numeric order.
pub fn parse_rawprogram_dir(dir: &str) -> Result<Vec<RawEntry>> {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("rawprogram") && name.ends_with(".xml") {
                files.push(p);
            }
        }
    }
    // Sort by the trailing number before .xml.
    files.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let digits: String = name
            .chars()
            .rev()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit())
            .collect();
        digits.parse::<u32>().unwrap_or(0)
    });
    if files.is_empty() {
        bail!("no rawprogram*.xml files in {}", dir);
    }
    let mut all = Vec::new();
    for f in files {
        all.extend(parse_rawprogram_file(f.to_str().unwrap())?);
    }
    Ok(all)
}

/// Keep only `<program>` entries (skip `<erase>`), as flash entries.
pub fn programs_only(entries: &[RawEntry]) -> Vec<&RawEntry> {
    entries
        .iter()
        .filter(|e| e.kind == EntryKind::Program)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML0: &str = r#"<?xml version="1.0" ?>
<data>
  <program SECTOR_SIZE_IN_BYTES="4096" file_sector_offset="0" filename="modem.img"
           label="modem" num_partition_sectors="3072" physical_partition_number="0"
           sparse="false" start_sector="6144"/>
  <erase SECTOR_SIZE_IN_BYTES="4096" num_partition_sectors="1024"
         physical_partition_number="0" start_sector="32768" label="cache"/>
</data>"#;

    const XML1: &str = r#"<?xml version="1.0" ?>
<data>
  <program SECTOR_SIZE_IN_BYTES="4096" filename="userdata.img" label="userdata"
           num_partition_sectors="1000000" physical_partition_number="1"
           sparse="true" start_sector="2048"/>
</data>"#;

    #[test]
    fn parses_program_and_erase() {
        let entries = parse_rawprogram_str(XML0);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].label, "modem");
        assert_eq!(entries[0].kind, EntryKind::Program);
        assert_eq!(entries[0].start_sector, 6144);
        assert_eq!(entries[0].num_sectors, 3072);
        assert_eq!(entries[0].sector_size, 4096);
        assert_eq!(entries[0].size_bytes(), 3072 * 4096);
        assert!(!entries[0].sparse);
        assert_eq!(entries[1].kind, EntryKind::Erase);
        assert_eq!(entries[1].label, "cache");
        assert_eq!(entries[1].filename, "");
    }

    #[test]
    fn parses_sparse_flag() {
        let entries = parse_rawprogram_str(XML1);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].sparse);
        assert_eq!(entries[0].physical_partition, 1);
    }

    #[test]
    fn programs_only_filters_erase() {
        let mut entries = parse_rawprogram_str(XML0);
        entries.extend(parse_rawprogram_str(XML1));
        let progs = programs_only(&entries);
        assert_eq!(progs.len(), 2);
        assert!(progs.iter().all(|e| e.kind == EntryKind::Program));
    }

    #[test]
    fn reads_dir_in_order() {
        let d = std::env::temp_dir().join("rp_dir_test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("rawprogram0.xml"), XML0).unwrap();
        std::fs::write(d.join("rawprogram1.xml"), XML1).unwrap();
        let entries = parse_rawprogram_dir(d.to_str().unwrap()).unwrap();
        let labels: Vec<_> = entries.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, vec!["modem", "cache", "userdata"]);
        let _ = std::fs::remove_dir_all(&d);
    }
}