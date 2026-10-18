//! Phase 3B: a metadata-only archive policy checker. It reads the HEADERS of a ZIP (central directory) or of an uncompressed
//! ustar TAR held in memory and judges them against an expected-entry manifest. It never decompresses, never extracts and never
//! touches the file system: a real extraction step (Phase 3C or later) may only run on an archive for which this check returned
//! no finding, and must still enforce the same limits while it writes.
//!
//! Findings carry a code and an entry INDEX, never an entry name (names come from an untrusted archive).
//!
//! Not covered here, by design: the gzip layer of a `.tar.gz` (a bounded decompressor that counts output against the limits is
//! required before the TAR reader may see the bytes), PAX / GNU long-name extensions (refused, fail closed), ZIP64, encrypted
//! or multi-disk ZIPs (refused).
#![allow(dead_code)] // used by the tests and the specification only; nothing in the application calls it yet

use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    File,
    Directory,
    Symlink,
    Hardlink,
    Other,
}

/// What the archive's headers claim about one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryMeta {
    pub name: Vec<u8>,
    pub kind: Kind,
    pub size: u64,
    pub packed_size: u64,
    /// Unix permission/type bits when the archive carries them.
    pub mode: Option<u32>,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: usize,
    pub max_entry_size: u64,
    pub max_total_size: u64,
    /// Largest accepted ratio of unpacked to packed size (checked for entries above `ratio_floor` bytes).
    pub max_ratio: u64,
    pub ratio_floor: u64,
    pub max_name_len: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_entries: 64,
            max_entry_size: 64 * 1024 * 1024,
            max_total_size: 128 * 1024 * 1024,
            max_ratio: 20,
            ratio_floor: 64 * 1024,
            max_name_len: 255,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedEntry {
    pub name: String,
    /// Size and SHA-256 of the file AFTER extraction: evidence of its own, never the digest of the archive.
    pub size: u64,
    pub sha256: String,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedManifest {
    /// Digest of the archive file as downloaded (a different fact from the digests of the files inside it).
    pub archive_sha256: String,
    pub entries: Vec<ExpectedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub code: &'static str,
    pub entry: Option<usize>,
}

fn f(code: &'static str) -> Finding {
    Finding { code, entry: None }
}

fn fe(code: &'static str, entry: usize) -> Finding {
    Finding { code, entry: Some(entry) }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// ---------------------------------------------------------------------------------------------------------------------
// ZIP: central directory only
// ---------------------------------------------------------------------------------------------------------------------

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Reads the entries of a ZIP from its central directory and cross-checks every local header (name equality, plausible data
/// ranges, no overlapping data). Nothing is decompressed. Anything unusual is an error, not a guess.
pub fn read_zip(bytes: &[u8]) -> Result<Vec<EntryMeta>, &'static str> {
    // end of central directory: scan back over at most a 65535-byte comment
    let min = bytes.len().saturating_sub(22 + 65535);
    let eocd = (min..=bytes.len().checked_sub(22).ok_or("zip_too_short")?)
        .rev()
        .find(|&i| bytes[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]))
        .ok_or("zip_end_record_missing")?;
    let disk = u16le(bytes, eocd + 4).ok_or("zip_malformed")?;
    let cd_disk = u16le(bytes, eocd + 6).ok_or("zip_malformed")?;
    let on_disk = u16le(bytes, eocd + 8).ok_or("zip_malformed")?;
    let total = u16le(bytes, eocd + 10).ok_or("zip_malformed")?;
    let cd_size = u32le(bytes, eocd + 12).ok_or("zip_malformed")?;
    let cd_off = u32le(bytes, eocd + 16).ok_or("zip_malformed")?;
    if disk != 0 || cd_disk != 0 || on_disk != total {
        return Err("zip_multi_disk_unsupported");
    }
    if total == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
        return Err("zip64_unsupported");
    }
    let (cd_off, cd_size) = (cd_off as usize, cd_size as usize);
    if cd_off.checked_add(cd_size).is_none_or(|end| end > eocd) {
        return Err("zip_central_directory_out_of_range");
    }
    let mut at = cd_off;
    let mut out = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for _ in 0..total {
        if bytes.get(at..at + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err("zip_central_header_malformed");
        }
        let made_by = u16le(bytes, at + 4).ok_or("zip_malformed")?;
        let flags = u16le(bytes, at + 8).ok_or("zip_malformed")?;
        let method = u16le(bytes, at + 10).ok_or("zip_malformed")?;
        let csize = u32le(bytes, at + 20).ok_or("zip_malformed")?;
        let usize_ = u32le(bytes, at + 24).ok_or("zip_malformed")?;
        let nlen = u16le(bytes, at + 28).ok_or("zip_malformed")? as usize;
        let elen = u16le(bytes, at + 30).ok_or("zip_malformed")? as usize;
        let clen = u16le(bytes, at + 32).ok_or("zip_malformed")? as usize;
        let disk_start = u16le(bytes, at + 34).ok_or("zip_malformed")?;
        let ext = u32le(bytes, at + 38).ok_or("zip_malformed")?;
        let local = u32le(bytes, at + 42).ok_or("zip_malformed")?;
        if csize == 0xFFFF_FFFF || usize_ == 0xFFFF_FFFF || local == 0xFFFF_FFFF {
            return Err("zip64_unsupported");
        }
        if disk_start != 0 {
            return Err("zip_multi_disk_unsupported");
        }
        if method != 0 && method != 8 {
            return Err("zip_compression_method_unsupported");
        }
        let name = bytes.get(at + 46..at + 46 + nlen).ok_or("zip_malformed")?.to_vec();
        let next = at + 46 + nlen + elen + clen;
        if next > cd_off + cd_size {
            return Err("zip_central_header_malformed");
        }
        // the local header must describe the same entry
        let lo = local as usize;
        if bytes.get(lo..lo + 4) != Some(&[0x50, 0x4b, 0x03, 0x04]) {
            return Err("zip_local_header_missing");
        }
        let lnlen = u16le(bytes, lo + 26).ok_or("zip_malformed")? as usize;
        let lelen = u16le(bytes, lo + 28).ok_or("zip_malformed")? as usize;
        if bytes.get(lo + 30..lo + 30 + lnlen) != Some(&name[..]) {
            return Err("zip_local_header_name_mismatch");
        }
        let data_start = lo + 30 + lnlen + lelen;
        let data_end = data_start.checked_add(csize as usize).ok_or("zip_malformed")?;
        if data_end > cd_off {
            return Err("zip_entry_data_out_of_range");
        }
        ranges.push((lo, data_end));
        // kind and mode: Unix hosts (3) carry the full st_mode in the high half of the external attributes
        let host = (made_by >> 8) as u8;
        let (mut kind, mode) = if host == 3 {
            let mode = ext >> 16;
            let kind = match mode & 0o170000 {
                0o120000 => Kind::Symlink,
                0o040000 => Kind::Directory,
                0o100000 | 0 => Kind::File,
                _ => Kind::Other,
            };
            (kind, Some(mode))
        } else {
            (Kind::File, None)
        };
        if ext & 0x10 != 0 || name.last() == Some(&b'/') {
            kind = if kind == Kind::File { Kind::Directory } else { kind };
        }
        if host != 3 && ext & 0x400 != 0 {
            kind = Kind::Symlink; // an NTFS reparse point
        }
        out.push(EntryMeta { name, kind, size: u64::from(usize_), packed_size: u64::from(csize), mode, encrypted: flags & 1 != 0 });
        at = next;
    }
    if at != cd_off + cd_size {
        return Err("zip_central_directory_has_trailing_bytes");
    }
    // two entries must never share bytes (overlapping-data bombs)
    ranges.sort_unstable();
    if ranges.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err("zip_entries_overlap");
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------
// TAR: uncompressed ustar headers
// ---------------------------------------------------------------------------------------------------------------------

fn octal(field: &[u8]) -> Option<u64> {
    if field.first().is_some_and(|b| b & 0x80 != 0) {
        return None; // base-256 numbers are not accepted
    }
    let text: Vec<u8> = field.iter().copied().take_while(|b| *b != 0 && *b != b' ').collect();
    if text.is_empty() {
        return Some(0);
    }
    u64::from_str_radix(std::str::from_utf8(&text).ok()?, 8).ok()
}

fn cstr(field: &[u8]) -> &[u8] {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    &field[..end]
}

/// Reads the headers of an uncompressed ustar archive. PAX and GNU extension headers are refused.
pub fn read_tar(bytes: &[u8]) -> Result<Vec<EntryMeta>, &'static str> {
    let mut out = Vec::new();
    let mut at = 0usize;
    let mut ended = false;
    while at + 512 <= bytes.len() {
        let h = &bytes[at..at + 512];
        if h.iter().all(|b| *b == 0) {
            ended = true;
            break;
        }
        let stored = octal(&h[148..156]).ok_or("tar_checksum_malformed")?;
        let sum: u64 = h.iter().enumerate().map(|(i, b)| if (148..156).contains(&i) { 32 } else { u64::from(*b) }).sum();
        if sum != stored {
            return Err("tar_checksum_mismatch");
        }
        if &h[257..262] != b"ustar" {
            return Err("tar_not_ustar");
        }
        let size = octal(&h[124..136]).ok_or("tar_size_malformed")?;
        let mode = octal(&h[100..108]).ok_or("tar_mode_malformed")? as u32;
        let mut name = cstr(&h[345..500]).to_vec();
        if !name.is_empty() {
            name.push(b'/');
        }
        name.extend_from_slice(cstr(&h[0..100]));
        let (kind, data) = match h[156] {
            b'0' | 0 => (Kind::File, true),
            b'5' => (Kind::Directory, false),
            b'1' => (Kind::Hardlink, false),
            b'2' => (Kind::Symlink, false),
            b'x' | b'g' | b'L' | b'K' => return Err("tar_extension_header_unsupported"),
            _ => (Kind::Other, false),
        };
        let data_len = if data { size } else { 0 };
        let blocks = data_len.div_ceil(512) as usize;
        let next = at.checked_add(512 + blocks * 512).ok_or("tar_malformed")?;
        if next > bytes.len() {
            return Err("tar_entry_data_out_of_range");
        }
        out.push(EntryMeta { name, kind, size: data_len, packed_size: data_len, mode: Some(mode), encrypted: false });
        at = next;
    }
    if !ended {
        return Err("tar_end_marker_missing");
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------------------------------------------------

const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4",
    "lpt5", "lpt6", "lpt7", "lpt8", "lpt9", "conin$", "conout$",
];

const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "dll", "bat", "cmd", "ps1", "psm1", "vbs", "js", "wsf", "msi", "scr", "com", "jar", "sh", "py", "dylib", "so", "command", "app",
];

/// Findings about one entry NAME (shared by archive entries and by the manifest's expected names).
fn name_findings(raw: &[u8], limits: &Limits, index: Option<usize>) -> Vec<Finding> {
    let mk = |code: &'static str| Finding { code, entry: index };
    let mut out = Vec::new();
    if raw.len() > limits.max_name_len {
        out.push(mk("name_too_long"));
    }
    if raw.contains(&0) {
        out.push(mk("nul_in_name"));
    }
    if raw.iter().any(|b| *b < 0x20 || *b == 0x7f) {
        out.push(mk("control_character_in_name"));
    }
    // ASCII only: Unicode normalisation differences (NFC/NFD) and look-alikes cannot create collisions or surprises
    let Ok(name) = std::str::from_utf8(raw) else {
        out.push(mk("name_not_utf8"));
        return out;
    };
    if !name.is_ascii() {
        out.push(mk("non_ascii_name"));
    }
    if name.contains('\\') {
        out.push(mk("backslash_in_name"));
    }
    if name.starts_with('/') {
        out.push(mk("absolute_path"));
    }
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        out.push(mk("drive_letter_path"));
    } else if name.contains(':') {
        out.push(mk("colon_in_name")); // NTFS alternate data streams
    }
    let trimmed = name.strip_suffix('/').unwrap_or(name);
    for (i, comp) in trimmed.split('/').enumerate() {
        let _ = i;
        match comp {
            "" => out.push(mk("empty_path_component")),
            "." => out.push(mk("dot_path_component")),
            ".." => out.push(mk("path_traversal")),
            _ => {}
        }
        if comp.ends_with('.') && comp != "." && comp != ".." || comp.ends_with(' ') {
            out.push(mk("trailing_dot_or_space"));
        }
        if comp.chars().any(|c| "<>\"|?*".contains(c)) {
            out.push(mk("windows_forbidden_character"));
        }
        let stem = comp.split('.').next().unwrap_or("").trim_end().to_ascii_lowercase();
        if RESERVED.contains(&stem.as_str()) {
            out.push(mk("windows_reserved_name"));
        }
    }
    out
}

/// Sanity of the expected manifest itself: evidence of the archive and evidence of the extracted files must be different facts.
pub fn check_manifest(m: &ExpectedManifest, limits: &Limits) -> Vec<Finding> {
    let mut out = Vec::new();
    if !is_sha256(&m.archive_sha256) {
        out.push(f("manifest_archive_digest_malformed"));
    }
    if m.entries.is_empty() {
        out.push(f("manifest_empty"));
    }
    let mut seen: HashSet<String> = HashSet::new();
    for (i, e) in m.entries.iter().enumerate() {
        if !is_sha256(&e.sha256) {
            out.push(fe("manifest_file_digest_malformed", i));
        } else if e.sha256 == m.archive_sha256 {
            out.push(fe("archive_digest_reused_as_file_digest", i));
        }
        if !name_findings(e.name.as_bytes(), limits, Some(i)).is_empty() || e.name.ends_with('/') {
            out.push(fe("manifest_entry_name_unsafe", i));
        }
        if !seen.insert(e.name.to_ascii_lowercase()) {
            out.push(fe("manifest_duplicate_entry", i));
        }
        if e.size > limits.max_entry_size {
            out.push(fe("entry_size_limit", i));
        }
    }
    out
}

/// Judges the headers against the manifest and the limits. An empty result means "nothing in the headers objects"; it is not a
/// statement about the content, which is checked per file after extraction (`verify_extracted`).
pub fn check(entries: &[EntryMeta], manifest: &ExpectedManifest, limits: &Limits) -> Vec<Finding> {
    let mut out = check_manifest(manifest, limits);
    if entries.len() > limits.max_entries {
        out.push(f("entry_count_limit"));
    }
    let mut total: u64 = 0;
    let mut exact: HashSet<&[u8]> = HashSet::new();
    let mut folded: HashSet<String> = HashSet::new();
    let mut matched: HashSet<usize> = HashSet::new();
    for (i, e) in entries.iter().enumerate() {
        out.extend(name_findings(&e.name, limits, Some(i)));
        if !exact.insert(&e.name) {
            out.push(fe("duplicate_entry", i));
        } else if !folded.insert(String::from_utf8_lossy(&e.name).to_ascii_lowercase()) {
            out.push(fe("case_collision", i));
        }
        match e.kind {
            Kind::Symlink => out.push(fe("symlink_entry", i)),
            Kind::Hardlink => out.push(fe("hardlink_entry", i)),
            Kind::Other => out.push(fe("special_file_entry", i)),
            Kind::File | Kind::Directory => {}
        }
        if e.encrypted {
            out.push(fe("encrypted_entry", i));
        }
        if let Some(mode) = e.mode {
            if mode & 0o7000 != 0 {
                out.push(fe("setuid_setgid_or_sticky", i));
            }
            if mode & 0o002 != 0 && e.kind == Kind::File {
                out.push(fe("world_writable_file", i));
            }
        }
        total = total.saturating_add(e.size);
        if e.size > limits.max_entry_size {
            out.push(fe("entry_size_limit", i));
        }
        if e.size > limits.ratio_floor && e.size / e.packed_size.max(1) > limits.max_ratio {
            out.push(fe("compression_ratio_limit", i));
        }
        let name = String::from_utf8_lossy(&e.name).into_owned();
        match e.kind {
            Kind::Directory => {
                // a folder is acceptable only as the parent of something expected
                let prefix = name.trim_end_matches('/').to_string() + "/";
                if !manifest.entries.iter().any(|x| x.name.starts_with(&prefix)) {
                    out.push(fe("unexpected_entry", i));
                }
            }
            Kind::File => match manifest.entries.iter().position(|x| x.name == name) {
                Some(p) => {
                    if !matched.insert(p) {
                        out.push(fe("duplicate_entry", i));
                    }
                    let want = &manifest.entries[p];
                    if e.size != want.size {
                        out.push(fe("size_mismatch", i));
                    }
                    if let Some(mode) = e.mode {
                        if (mode & 0o111 != 0) != want.executable {
                            out.push(fe("executable_bit_mismatch", i));
                        }
                    }
                }
                None => {
                    out.push(fe("unexpected_entry", i));
                    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
                    if EXECUTABLE_EXTENSIONS.contains(&ext.as_str()) || e.mode.is_some_and(|m| m & 0o111 != 0) {
                        out.push(fe("unexpected_executable", i));
                    }
                }
            },
            _ => {}
        }
    }
    if total > limits.max_total_size {
        out.push(f("total_size_limit"));
    }
    for (p, _) in manifest.entries.iter().enumerate() {
        if !matched.contains(&p) {
            out.push(fe("missing_expected_entry", p));
        }
    }
    out
}

/// After a (future) extraction: is this exactly the file the manifest expects? The archive's own digest is no substitute.
pub fn verify_extracted(want: &ExpectedEntry, bytes: &[u8]) -> bool {
    bytes.len() as u64 == want.size && sha256_hex(bytes) == want.sha256
}

#[cfg(test)]
#[path = "archive_check_tests.rs"]
mod tests;
