//! Reference digests and the validation policy (version 1). Read-only data handling; nothing is fetched or executed.
//!
//! # The trust contract
//!
//! A SHA-256 **fingerprint** computed from a local file says what bytes are there. It says nothing about where they came
//! from. Identity can only be *verified* by comparing the fingerprint of the exact artifact with a **reference digest
//! that was authenticated independently of the file and of every locally editable record**. This policy therefore
//! separates three kinds of reference:
//!
//! * `stored` — rows in the local database (`reference_digests`), which also hold values copied from a README,
//!   `package.json`, a repository or typed by a user. The database is user-writable, so these are **never** trusted,
//!   whatever a row says (the table's CHECK constraint does not even allow "authenticated").
//! * `built_in` — digests compiled into the application and covered by code review ([`BUILT_IN_AUTHENTICATED`]). This is
//!   the only source that can authenticate. It is **empty** in this phase: no authoritative, independently signed digest
//!   evidence for an installed file exists in the catalog, and the application does not fetch any.
//! * nothing — then the answer stays UNKNOWN / evidence available.
//!
//! A digest applies to one kind of artifact. An **archive** or installer digest is never the expected digest of the
//! executable extracted from it, and one file digest does not authenticate a plugin directory or a skill collection:
//! only `executable`, `script` and `plugin_file` references (an exact file) are compared with an exact file.
//!
//! # Type binding (policy version 2, universal Mach-O strictness since version 3)
//!
//! A matching digest is not enough. A reference authenticates the opened file only if **all** of these agree:
//! catalog entry, operating system and architecture (`platform_target`), the **artifact kind** (the role: executable,
//! script, plugin file, …), the **file format verified from the file's own bytes** ([`sniff`] on the same handle that was
//! hashed — never from the file name), and the exact SHA-256. A digest that matches but belongs to another kind (script vs
//! executable, plugin file vs standalone executable, archive vs extracted file), another format (PE vs Mach-O) or whose
//! format evidence is missing or malformed does **not** authenticate; it is reported as `incomparable`. Two authenticated
//! references that disagree about the version of the same bytes (or about the digest of the same version) make the whole
//! set for that entry ambiguous: nothing verifies.
//!
//! Evidence is bound to the policy version and to the exact reference set it was judged against (`epoch`); when either
//! changes, previously stored verdicts are invalidated when read.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

/// Bump when the matching rules change: stored verdicts made under another version are invalidated on read.
pub const POLICY_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// A download archive (zip, tar.gz, …). Never the expected digest of an extracted file.
    Archive,
    Installer,
    /// An exact installed program file.
    Executable,
    /// An exact installed script file.
    Script,
    /// One file of a plugin. Says nothing about the rest of the plugin.
    PluginFile,
    /// A whole package or directory tree. Not comparable with a single file.
    Package,
}

impl ArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactKind::Archive => "archive",
            ArtifactKind::Installer => "installer",
            ArtifactKind::Executable => "executable",
            ArtifactKind::Script => "script",
            ArtifactKind::PluginFile => "plugin_file",
            ArtifactKind::Package => "package",
        }
    }

    pub fn parse(s: &str) -> Option<ArtifactKind> {
        [
            ArtifactKind::Archive,
            ArtifactKind::Installer,
            ArtifactKind::Executable,
            ArtifactKind::Script,
            ArtifactKind::PluginFile,
            ArtifactKind::Package,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }

    /// May a reference of this kind be compared with the fingerprint of one installed file?
    pub fn is_exact_file(self) -> bool {
        matches!(self, ArtifactKind::Executable | ArtifactKind::Script | ArtifactKind::PluginFile)
    }
}

/// The format of an opened file, established from its first bytes (never from its name). `Unrecognised` is the answer for
/// everything the sniffer cannot positively identify, including a header that is truncated or malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ArtifactFormat {
    #[serde(rename = "pe-x64")]
    PeX64,
    #[serde(rename = "pe-arm64")]
    PeArm64,
    #[serde(rename = "pe-x86")]
    PeX86,
    #[serde(rename = "macho-arm64")]
    MachOArm64,
    #[serde(rename = "macho-x64")]
    MachOX64,
    /// LEGACY label of policy v2 (container magic only). Never produced any more and never binds to anything: the
    /// presence of the universal-file magic says nothing about which CPUs the file supports.
    #[serde(rename = "macho-universal")]
    MachOUniversal,
    /// Universal Mach-O whose verified slices are arm64 only (native on Apple Silicon).
    #[serde(rename = "macho-universal-arm64")]
    MachOUniversalArm64,
    /// Universal Mach-O whose verified slices are x86_64 only (Rosetta-only on Apple Silicon; NOT native arm64).
    #[serde(rename = "macho-universal-x64")]
    MachOUniversalX64,
    /// Universal Mach-O with both a verified arm64 and a verified x86_64 slice.
    #[serde(rename = "macho-universal-arm64-x64")]
    MachOUniversalArm64X64,
    #[serde(rename = "elf-x64")]
    ElfX64,
    #[serde(rename = "elf-arm64")]
    ElfArm64,
    #[serde(rename = "script-shebang")]
    ScriptShebang,
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "gzip")]
    Gzip,
    #[default]
    #[serde(rename = "unrecognised")]
    Unrecognised,
}

impl ArtifactFormat {
    const ALL: [ArtifactFormat; 15] = [
        ArtifactFormat::PeX64,
        ArtifactFormat::PeArm64,
        ArtifactFormat::PeX86,
        ArtifactFormat::MachOArm64,
        ArtifactFormat::MachOX64,
        ArtifactFormat::MachOUniversal,
        ArtifactFormat::MachOUniversalArm64,
        ArtifactFormat::MachOUniversalX64,
        ArtifactFormat::MachOUniversalArm64X64,
        ArtifactFormat::ElfX64,
        ArtifactFormat::ElfArm64,
        ArtifactFormat::ScriptShebang,
        ArtifactFormat::Zip,
        ArtifactFormat::Gzip,
        ArtifactFormat::Unrecognised,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ArtifactFormat::PeX64 => "pe-x64",
            ArtifactFormat::PeArm64 => "pe-arm64",
            ArtifactFormat::PeX86 => "pe-x86",
            ArtifactFormat::MachOArm64 => "macho-arm64",
            ArtifactFormat::MachOX64 => "macho-x64",
            ArtifactFormat::MachOUniversal => "macho-universal",
            ArtifactFormat::MachOUniversalArm64 => "macho-universal-arm64",
            ArtifactFormat::MachOUniversalX64 => "macho-universal-x64",
            ArtifactFormat::MachOUniversalArm64X64 => "macho-universal-arm64-x64",
            ArtifactFormat::ElfX64 => "elf-x64",
            ArtifactFormat::ElfArm64 => "elf-arm64",
            ArtifactFormat::ScriptShebang => "script-shebang",
            ArtifactFormat::Zip => "zip",
            ArtifactFormat::Gzip => "gzip",
            ArtifactFormat::Unrecognised => "unrecognised",
        }
    }

    /// Strict: only the canonical identifiers (a free-text or misspelled format is malformed evidence).
    pub fn parse(s: &str) -> Option<ArtifactFormat> {
        ArtifactFormat::ALL.into_iter().find(|f| f.as_str() == s)
    }

    /// The only role this format can have for an installed single file. `None` = nothing can be concluded.
    pub fn role(self) -> Option<ArtifactKind> {
        match self {
            ArtifactFormat::PeX64
            | ArtifactFormat::PeArm64
            | ArtifactFormat::PeX86
            | ArtifactFormat::MachOArm64
            | ArtifactFormat::MachOX64
            | ArtifactFormat::MachOUniversalArm64
            | ArtifactFormat::MachOUniversalX64
            | ArtifactFormat::MachOUniversalArm64X64
            | ArtifactFormat::ElfX64
            | ArtifactFormat::ElfArm64 => Some(ArtifactKind::Executable),
            ArtifactFormat::ScriptShebang => Some(ArtifactKind::Script),
            ArtifactFormat::Zip | ArtifactFormat::Gzip => Some(ArtifactKind::Archive),
            ArtifactFormat::Unrecognised | ArtifactFormat::MachOUniversal => None,
        }
    }

    /// (operating system, architecture) a native binary format belongs to; `None` for formats without one.
    fn target(self) -> Option<(&'static str, &'static [&'static str])> {
        match self {
            ArtifactFormat::PeX64 => Some(("windows", &["x64"])),
            ArtifactFormat::PeArm64 => Some(("windows", &["arm64"])),
            ArtifactFormat::PeX86 => Some(("windows", &["x86"])),
            ArtifactFormat::MachOArm64 | ArtifactFormat::MachOUniversalArm64 => Some(("macos", &["arm64"])),
            ArtifactFormat::MachOX64 | ArtifactFormat::MachOUniversalX64 => Some(("macos", &["x64"])),
            ArtifactFormat::MachOUniversalArm64X64 => Some(("macos", &["arm64", "x64"])),
            // legacy container-only label: no architecture was ever established, so it fits no target
            ArtifactFormat::MachOUniversal => Some(("macos", &[])),
            ArtifactFormat::ElfX64 => Some(("linux", &["x64"])),
            ArtifactFormat::ElfArm64 => Some(("linux", &["arm64"])),
            _ => None,
        }
    }

    /// Can a file of this format be a NATIVE artifact of `platform_target` (`windows-x64`, `macos-arm64`, …)? A PE file
    /// can never be the artifact of a macOS target; an x86_64-only universal binary is not a native arm64 artifact
    /// (running it needs Rosetta, which is a different claim). Script and archive formats are platform-neutral.
    pub fn consistent_with(self, platform_target: &str) -> bool {
        match self.target() {
            None => true,
            Some((os, archs)) => archs.iter().any(|a| platform_target == format!("{os}-{a}")),
        }
    }
}

/// Most architecture entries a universal Mach-O may have to be accepted (real files have 2-3).
const MAX_FAT_ARCHES: usize = 8;
const CPU_ARM64: u32 = 0x0100_000c;
const CPU_X64: u32 = 0x0100_0007;
/// Size of a `mach_header_64`: a slice smaller than this cannot be a Mach-O file.
const MACH_HEADER_64: u64 = 32;

/// One entry of a universal header, as declared (not yet verified against the slice itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FatEntry {
    cpu: u32,
    offset: u64,
    size: u64,
}

/// Parses and bounds-checks the universal header in `head` (big- or little-endian fields, 32- or 64-bit entries) against
/// the real file length. `None` = not a universal header, truncated, or structurally invalid (count out of range, a
/// slice outside the file, overlapping the table or another slice, shorter than a Mach-O header, a repeated CPU).
fn parse_fat(head: &[u8], file_len: u64) -> Option<Vec<FatEntry>> {
    let magic: [u8; 4] = head.get(..4)?.try_into().ok()?;
    // (fields are big-endian, entries are 64-bit)
    let (big_endian, wide) = match magic {
        [0xca, 0xfe, 0xba, 0xbe] => (true, false),
        [0xca, 0xfe, 0xba, 0xbf] => (true, true),
        [0xbe, 0xba, 0xfe, 0xca] => (false, false),
        [0xbf, 0xba, 0xfe, 0xca] => (false, true),
        _ => return None,
    };
    let u32_at = |at: usize| -> Option<u32> {
        let b: [u8; 4] = head.get(at..at + 4)?.try_into().ok()?;
        Some(if big_endian { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) })
    };
    let u64_at = |at: usize| -> Option<u64> {
        let b: [u8; 8] = head.get(at..at + 8)?.try_into().ok()?;
        Some(if big_endian { u64::from_be_bytes(b) } else { u64::from_le_bytes(b) })
    };
    let count = u32_at(4)? as usize;
    if count == 0 || count > MAX_FAT_ARCHES {
        return None;
    }
    let entry_len = if wide { 32 } else { 20 };
    let table_end = 8 + count * entry_len;
    if table_end > head.len() {
        return None; // the table is cut off
    }
    let mut out: Vec<FatEntry> = Vec::with_capacity(count);
    for i in 0..count {
        let at = 8 + i * entry_len;
        let cpu = u32_at(at)?;
        let (offset, size) =
            if wide { (u64_at(at + 8)?, u64_at(at + 16)?) } else { (u64::from(u32_at(at + 8)?), u64::from(u32_at(at + 12)?)) };
        let end = offset.checked_add(size)?;
        if size < MACH_HEADER_64 || offset < table_end as u64 || end > file_len || out.iter().any(|e| e.cpu == cpu) {
            return None;
        }
        out.push(FatEntry { cpu, offset, size });
    }
    // slices may not overlap each other
    let mut by_offset = out.clone();
    by_offset.sort_by_key(|e| e.offset);
    if by_offset.windows(2).any(|w| w[0].offset + w[0].size > w[1].offset) {
        return None;
    }
    Some(out)
}

/// Establishes the architecture of a universal Mach-O: the table must be valid and every slice must start with a
/// little-endian 64-bit Mach-O header (read with two bounded 8-byte reads from the SAME handle) whose CPU equals the
/// table's claim, and be arm64 or x86_64. Anything else (32-bit or other-CPU slices, a slice header that disagrees with
/// the table, a failed read) fails closed to `Unrecognised`.
fn classify_fat<R: std::io::Read + std::io::Seek>(head: &[u8], file_len: u64, reader: &mut R) -> ArtifactFormat {
    let Some(entries) = parse_fat(head, file_len) else { return ArtifactFormat::Unrecognised };
    let (mut arm64, mut x64) = (false, false);
    for e in entries {
        let mut header = [0u8; 8];
        if reader.seek(std::io::SeekFrom::Start(e.offset)).is_err() || reader.read_exact(&mut header).is_err() {
            return ArtifactFormat::Unrecognised;
        }
        let cpu = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
        if header[..4] != [0xcf, 0xfa, 0xed, 0xfe] || cpu != e.cpu {
            return ArtifactFormat::Unrecognised;
        }
        match cpu {
            CPU_ARM64 => arm64 = true,
            CPU_X64 => x64 = true,
            _ => return ArtifactFormat::Unrecognised,
        }
    }
    match (arm64, x64) {
        (true, true) => ArtifactFormat::MachOUniversalArm64X64,
        (true, false) => ArtifactFormat::MachOUniversalArm64,
        (false, true) => ArtifactFormat::MachOUniversalX64,
        (false, false) => ArtifactFormat::Unrecognised,
    }
}

/// Identifies the format from the first bytes of a file (up to 4 KiB are enough) and, for a universal Mach-O, from two
/// bounded reads per slice header from `reader` (the same opened handle) plus the file length. Pure and bounded: it
/// never reads anything else, never executes anything and answers `Unrecognised` for anything that is not positively
/// identified.
pub fn sniff_reader<R: std::io::Read + std::io::Seek>(head: &[u8], file_len: u64, reader: &mut R) -> ArtifactFormat {
    if matches!(head.get(..4), Some([0xca, 0xfe, 0xba, 0xbe | 0xbf] | [0xbe | 0xbf, 0xba, 0xfe, 0xca])) {
        return classify_fat(head, file_len, reader);
    }
    sniff(head)
}

/// Format from the leading bytes alone. A universal Mach-O cannot be judged from the container header: it is
/// `Unrecognised` here; use [`sniff_reader`] to read the slices.
pub fn sniff(head: &[u8]) -> ArtifactFormat {
    let le16 = |at: usize| head.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let le32 = |at: usize| head.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if head.starts_with(b"MZ") {
        let Some(pe) = le32(0x3c).map(|v| v as usize) else { return ArtifactFormat::Unrecognised };
        if pe < 0x40 || head.get(pe..pe + 4) != Some(b"PE\0\0".as_slice()) {
            return ArtifactFormat::Unrecognised;
        }
        return match le16(pe + 4) {
            Some(0x8664) => ArtifactFormat::PeX64,
            Some(0xaa64) => ArtifactFormat::PeArm64,
            Some(0x014c) => ArtifactFormat::PeX86,
            _ => ArtifactFormat::Unrecognised,
        };
    }
    if head.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) {
        return match le32(4) {
            Some(CPU_ARM64) => ArtifactFormat::MachOArm64,
            Some(CPU_X64) => ArtifactFormat::MachOX64,
            _ => ArtifactFormat::Unrecognised,
        };
    }
    if head.starts_with(&[0x7f, b'E', b'L', b'F']) {
        if head.get(4) != Some(&2) || head.get(5) != Some(&1) {
            return ArtifactFormat::Unrecognised;
        }
        return match le16(18) {
            Some(0x3e) => ArtifactFormat::ElfX64,
            Some(0xb7) => ArtifactFormat::ElfArm64,
            _ => ArtifactFormat::Unrecognised,
        };
    }
    if head.starts_with(b"#!") {
        let line = head.split(|b| *b == b'\n').next().unwrap_or(&[]);
        let printable = line.iter().all(|b| (0x20..0x7f).contains(b) || *b == b'\r' || *b == b'\t');
        return if printable && line.len() < 200 && head.len() > line.len() {
            ArtifactFormat::ScriptShebang
        } else {
            ArtifactFormat::Unrecognised
        };
    }
    if head.starts_with(b"PK\x03\x04") {
        return ArtifactFormat::Zip;
    }
    if head.starts_with(&[0x1f, 0x8b]) {
        return ArtifactFormat::Gzip;
    }
    ArtifactFormat::Unrecognised
}

/// Minimal, synthetic file bytes that [`sniff`] identifies as `format`, followed by `payload`. For tests and for the
/// built-in self-check probe only: they are not real programs and are never executed.
#[allow(dead_code)]
pub fn synthetic_file(format: ArtifactFormat, payload: &[u8]) -> Vec<u8> {
    let mut v: Vec<u8> = match format {
        ArtifactFormat::PeX64 | ArtifactFormat::PeArm64 | ArtifactFormat::PeX86 => {
            let mut h = vec![0u8; 0x80 + 24];
            h[..2].copy_from_slice(b"MZ");
            h[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
            h[0x80..0x84].copy_from_slice(b"PE\0\0");
            let machine: u16 = match format {
                ArtifactFormat::PeX64 => 0x8664,
                ArtifactFormat::PeArm64 => 0xaa64,
                _ => 0x014c,
            };
            h[0x84..0x86].copy_from_slice(&machine.to_le_bytes());
            h
        }
        ArtifactFormat::MachOArm64 | ArtifactFormat::MachOX64 => {
            let mut h = vec![0xcf, 0xfa, 0xed, 0xfe];
            let cpu: u32 = if format == ArtifactFormat::MachOArm64 { 0x0100_000c } else { 0x0100_0007 };
            h.extend_from_slice(&cpu.to_le_bytes());
            h.extend_from_slice(&[0u8; 24]);
            h
        }
        ArtifactFormat::MachOUniversal | ArtifactFormat::Unrecognised => Vec::new(),
        ArtifactFormat::MachOUniversalArm64 => synthetic_fat(&[CPU_ARM64], FatStyle::Be32),
        ArtifactFormat::MachOUniversalX64 => synthetic_fat(&[CPU_X64], FatStyle::Be32),
        ArtifactFormat::MachOUniversalArm64X64 => synthetic_fat(&[CPU_X64, CPU_ARM64], FatStyle::Be32),
        ArtifactFormat::ElfX64 | ArtifactFormat::ElfArm64 => {
            let mut h = vec![0u8; 64];
            h[..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
            h[4] = 2;
            h[5] = 1;
            let machine: u16 = if format == ArtifactFormat::ElfX64 { 0x3e } else { 0xb7 };
            h[18..20].copy_from_slice(&machine.to_le_bytes());
            h
        }
        ArtifactFormat::ScriptShebang => b"#!/bin/sh\n".to_vec(),
        ArtifactFormat::Zip => b"PK\x03\x04".to_vec(),
        ArtifactFormat::Gzip => vec![0x1f, 0x8b, 8, 0],
    };
    v.extend_from_slice(payload);
    v
}

/// Container variants the synthetic universal builder can emit (the parser accepts all four).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatStyle {
    Be32,
    Be64,
    Le32,
    Le64,
}

/// A synthetic universal Mach-O: a table of `cpus` and, for each, a 4 KiB-aligned slice that begins with a valid 64-bit
/// Mach-O header of that CPU followed by padding. For tests and the probe only; never executed.
#[allow(dead_code)]
pub fn synthetic_fat(cpus: &[u32], style: FatStyle) -> Vec<u8> {
    let (big, wide) = match style {
        FatStyle::Be32 => (true, false),
        FatStyle::Be64 => (true, true),
        FatStyle::Le32 => (false, false),
        FatStyle::Le64 => (false, true),
    };
    let put32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&if big { x.to_be_bytes() } else { x.to_le_bytes() });
    let put64 = |v: &mut Vec<u8>, x: u64| v.extend_from_slice(&if big { x.to_be_bytes() } else { x.to_le_bytes() });
    let mut v: Vec<u8> = Vec::new();
    let magic: u32 = if wide { 0xcafe_babf } else { 0xcafe_babe };
    v.extend_from_slice(&if big { magic.to_be_bytes() } else { magic.to_le_bytes() });
    put32(&mut v, cpus.len() as u32);
    for (i, cpu) in cpus.iter().enumerate() {
        let offset = 0x1000u64 * (i as u64 + 1);
        put32(&mut v, *cpu);
        put32(&mut v, 0); // cpu subtype
        if wide {
            put64(&mut v, offset);
            put64(&mut v, 0x100);
            put32(&mut v, 12); // align
            put32(&mut v, 0); // reserved
        } else {
            put32(&mut v, offset as u32);
            put32(&mut v, 0x100);
            put32(&mut v, 12);
        }
    }
    for (i, cpu) in cpus.iter().enumerate() {
        v.resize(0x1000 * (i + 1), 0);
        v.extend_from_slice(&[0xcf, 0xfa, 0xed, 0xfe]);
        v.extend_from_slice(&cpu.to_le_bytes());
        v.resize(0x1000 * (i + 1) + 0x100, 0);
    }
    v
}

/// The format a native executable has on `platform_target` (synthetic fixtures and the probe use this).
#[allow(dead_code)]
pub fn native_executable_format(platform_target: &str) -> ArtifactFormat {
    match platform_target {
        "windows-x64" => ArtifactFormat::PeX64,
        "windows-arm64" => ArtifactFormat::PeArm64,
        "macos-arm64" => ArtifactFormat::MachOArm64,
        "macos-x64" => ArtifactFormat::MachOX64,
        _ => ArtifactFormat::Unrecognised,
    }
}

/// One reference digest with everything the contract requires to judge it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceDigest {
    pub entry_id: String,
    pub kind: ArtifactKind,
    /// Canonical format identifier ([`ArtifactFormat::as_str`]). Anything else is malformed evidence and binds nothing.
    pub format: String,
    pub platform_target: String,
    pub version: String,
    pub sha256: String,
    /// Where the value came from (sanitized label, never a path or URL with credentials).
    pub source: String,
}

/// Compiled-in, code-reviewed, independently authenticated references. EMPTY in this phase (see the module docs).
pub const BUILT_IN_AUTHENTICATED: &[BuiltIn] = &[];

/// Static form of an authenticated reference (so the list can be a `const`).
pub struct BuiltIn {
    pub entry_id: &'static str,
    pub kind: ArtifactKind,
    pub format: &'static str,
    pub platform_target: &'static str,
    pub version: &'static str,
    pub sha256: &'static str,
    /// How the digest was authenticated, independently of the file (for the review record).
    pub authentication: &'static str,
}

impl BuiltIn {
    fn to_reference(&self) -> ReferenceDigest {
        ReferenceDigest {
            entry_id: self.entry_id.to_string(),
            kind: self.kind,
            format: self.format.to_string(),
            platform_target: self.platform_target.to_string(),
            version: self.version.to_string(),
            sha256: self.sha256.to_string(),
            source: self.authentication.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReferenceSet {
    /// Authenticated (compiled in). The ONLY set that can verify.
    pub authenticated: Vec<ReferenceDigest>,
    /// Unauthenticated (database rows, README/package.json/user-supplied values). Evidence of a claim, never proof.
    pub unauthenticated: Vec<ReferenceDigest>,
}

impl ReferenceSet {
    /// The production set: the compiled-in list plus whatever the local database holds (as unauthenticated).
    pub fn production(conn: &Connection) -> AppResult<ReferenceSet> {
        Ok(ReferenceSet {
            authenticated: BUILT_IN_AUTHENTICATED.iter().map(BuiltIn::to_reference).collect(),
            unauthenticated: load_stored(conn)?,
        })
    }

    /// A stable fingerprint of the whole set. Verdicts are only valid against the set they were made with.
    pub fn epoch(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for (tag, list) in [("A", &self.authenticated), ("U", &self.unauthenticated)] {
            for r in list.iter() {
                lines.push(format!(
                    "{tag}|{}|{}|{}|{}|{}|{}|{}",
                    r.entry_id,
                    r.kind.as_str(),
                    r.format,
                    r.platform_target,
                    r.version,
                    r.sha256,
                    r.source
                ));
            }
        }
        lines.sort();
        let mut h = Sha256::new();
        h.update(format!("policy-{POLICY_VERSION}\n").as_bytes());
        h.update(lines.join("\n").as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchResult {
    /// No fingerprint, or no reference applies.
    #[default]
    NotApplicable,
    /// A reference exists but cannot be compared (e.g. only an archive digest).
    Incomparable,
    MatchesUnauthenticated,
    MismatchesUnauthenticated,
    MatchesAuthenticated,
    MismatchesAuthenticated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceState {
    #[default]
    None,
    /// A reference value exists but nothing independent authenticated it.
    Unauthenticated,
    Authenticated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub reference: ReferenceState,
    pub result: MatchResult,
    pub kind: Option<ArtifactKind>,
    /// The version of the matching AUTHENTICATED reference (only then).
    pub version: Option<String>,
}

/// What was actually opened and hashed: the facts a reference has to agree with.
pub struct ObservedArtifact<'a> {
    pub entry_id: &'a str,
    pub platform_target: &'a str,
    /// Verified from the opened file's bytes ([`sniff`]). `Unrecognised` when it could not be established.
    pub format: ArtifactFormat,
    pub sha256: &'a str,
}

/// Does this reference describe THIS artifact's type? Entry, platform, kind (role), verified format, and the
/// reference's own internal consistency (a Mach-O format on a Windows target is malformed) must all agree.
fn binds(r: &ReferenceDigest, o: &ObservedArtifact) -> bool {
    let Some(format) = ArtifactFormat::parse(&r.format) else { return false };
    r.entry_id == o.entry_id
        && r.platform_target == o.platform_target
        && format != ArtifactFormat::Unrecognised
        && format == o.format
        && format.consistent_with(&r.platform_target)
        && r.kind.is_exact_file()
        && Some(r.kind) == o.format.role()
}

/// The same digest under different versions, or the same version under different digests: the set cannot be trusted to
/// say what these bytes are.
fn conflicting(list: &[ReferenceDigest]) -> bool {
    list.iter().any(|a| list.iter().any(|b| (a.sha256 == b.sha256) != (a.version == b.version)))
}

/// Compares the fingerprint of an exact installed file with the references for this entry and platform, under the type
/// binding described in the module docs.
pub fn evaluate(o: &ObservedArtifact, set: &ReferenceSet) -> Verdict {
    let applies = |r: &&ReferenceDigest| r.entry_id == o.entry_id && r.platform_target == o.platform_target;
    let auth: Vec<&ReferenceDigest> = set.authenticated.iter().filter(applies).collect();
    let stored: Vec<&ReferenceDigest> = set.unauthenticated.iter().filter(applies).collect();
    let bound = |list: &[&ReferenceDigest]| -> Vec<ReferenceDigest> { list.iter().filter(|r| binds(r, o)).map(|r| (*r).clone()).collect() };
    let (auth_bound, stored_bound) = (bound(&auth), bound(&stored));
    let ambiguous = |state| Verdict { reference: state, result: MatchResult::Incomparable, kind: None, version: None };
    if !auth_bound.is_empty() {
        if conflicting(&auth_bound) {
            return ambiguous(ReferenceState::Authenticated);
        }
        return match auth_bound.iter().find(|r| r.sha256 == o.sha256) {
            Some(hit) => Verdict {
                reference: ReferenceState::Authenticated,
                result: MatchResult::MatchesAuthenticated,
                kind: Some(hit.kind),
                version: Some(hit.version.clone()),
            },
            None => Verdict {
                reference: ReferenceState::Authenticated,
                result: MatchResult::MismatchesAuthenticated,
                kind: Some(auth_bound[0].kind),
                version: None,
            },
        };
    }
    if !stored_bound.is_empty() {
        if conflicting(&stored_bound) {
            return ambiguous(ReferenceState::Unauthenticated);
        }
        return match stored_bound.iter().find(|r| r.sha256 == o.sha256) {
            Some(hit) => Verdict {
                reference: ReferenceState::Unauthenticated,
                result: MatchResult::MatchesUnauthenticated,
                kind: Some(hit.kind),
                version: None,
            },
            None => Verdict {
                reference: ReferenceState::Unauthenticated,
                result: MatchResult::MismatchesUnauthenticated,
                kind: Some(stored_bound[0].kind),
                version: None,
            },
        };
    }
    // References exist for this entry and platform but none describes this artifact's kind and verified format (an
    // archive, installer or package digest; a script digest for an executable; a PE digest for a Mach-O file; missing or
    // malformed format evidence): they cannot be compared with the opened file, whatever their values are.
    if !auth.is_empty() || !stored.is_empty() {
        let state = if auth.is_empty() { ReferenceState::Unauthenticated } else { ReferenceState::Authenticated };
        return ambiguous(state);
    }
    Verdict { reference: ReferenceState::None, result: MatchResult::NotApplicable, kind: None, version: None }
}

fn load_stored(conn: &Connection) -> AppResult<Vec<ReferenceDigest>> {
    let mut stmt = conn.prepare(
        "SELECT entry_id, artifact_kind, format, platform_target, version, sha256, source_label FROM reference_digests WHERE removed_at IS NULL",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (entry_id, kind, format, platform_target, version, sha256, source) = row?;
        // A row the CHECK constraints should have rejected (tampering) is ignored, never trusted.
        if let Some(kind) = ArtifactKind::parse(&kind) {
            if is_hex_digest(&sha256) {
                out.push(ReferenceDigest { entry_id, kind, format, platform_target, version, sha256, source });
            }
        }
    }
    Ok(out)
}

pub fn is_hex_digest(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Where an unauthenticated reference value came from. None of these can authenticate it.
#[allow(dead_code)] // used by the (test-only for now) import path
pub const UNAUTHENTICATED_SOURCES: &[&str] =
    &["user_provided", "readme_declared", "package_metadata_declared", "catalog_unreviewed", "synthetic_test"];

/// Records a reference digest as **unauthenticated**. Used by tests and by a future, separately reviewed import path; no
/// command, UI or scan calls it. Refuses malformed values; never creates a digest by hashing a local file.
#[allow(clippy::too_many_arguments, dead_code)]
pub fn insert_unauthenticated(
    conn: &Connection,
    entry_id: &str,
    kind: ArtifactKind,
    format: &str,
    platform_target: &str,
    version: &str,
    sha256: &str,
    source_kind: &str,
    source_label: &str,
) -> AppResult<()> {
    if !is_hex_digest(sha256) || !UNAUTHENTICATED_SOURCES.contains(&source_kind) {
        return Err(AppError::new("REFERENCE_INVALID", "Referans özeti veya kaynağı geçersiz."));
    }
    conn.execute(
        "INSERT INTO reference_digests (entry_id, artifact_kind, format, platform_target, version, algorithm, sha256, source_kind, source_label, authentication, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'sha256', ?6, ?7, ?8, 'none', ?9)",
        rusqlite::params![
            entry_id,
            kind.as_str(),
            crate::redact::sanitize(format, 40),
            platform_target,
            crate::redact::sanitize(version, 64),
            sha256,
            source_kind,
            crate::redact::sanitize(source_label, 80),
            crate::audit::now_utc()
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod unit {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const WIN: &str = "windows-x64";
    const MAC: &str = "macos-arm64";

    fn r(entry: &str, kind: ArtifactKind, format: &str, platform: &str, version: &str, digest: &str) -> ReferenceDigest {
        ReferenceDigest {
            entry_id: entry.into(),
            kind,
            format: format.into(),
            platform_target: platform.into(),
            version: version.into(),
            sha256: digest.into(),
            source: "test".into(),
        }
    }

    fn exe(entry: &str, version: &str, digest: &str) -> ReferenceDigest {
        r(entry, ArtifactKind::Executable, "pe-x64", WIN, version, digest)
    }

    fn seen<'a>(entry: &'a str, platform: &'a str, format: ArtifactFormat, sha: &'a str) -> ObservedArtifact<'a> {
        ObservedArtifact { entry_id: entry, platform_target: platform, format, sha256: sha }
    }

    fn auth(list: Vec<ReferenceDigest>) -> ReferenceSet {
        ReferenceSet { authenticated: list, unauthenticated: vec![] }
    }

    #[test]
    fn only_an_authenticated_exact_file_reference_of_the_right_type_can_match_authenticated() {
        let set = auth(vec![exe("t", "1.2.3", A)]);
        let v = evaluate(&seen("t", WIN, ArtifactFormat::PeX64, A), &set);
        assert_eq!(
            (v.reference, v.result, v.version.as_deref()),
            (ReferenceState::Authenticated, MatchResult::MatchesAuthenticated, Some("1.2.3"))
        );
        assert_eq!(evaluate(&seen("t", WIN, ArtifactFormat::PeX64, B), &set).result, MatchResult::MismatchesAuthenticated);
        // other tool / other platform: nothing applies
        assert_eq!(evaluate(&seen("other", WIN, ArtifactFormat::PeX64, A), &set).result, MatchResult::NotApplicable);
        assert_eq!(evaluate(&seen("t", MAC, ArtifactFormat::MachOArm64, A), &set).result, MatchResult::NotApplicable);
    }

    #[test]
    fn unauthenticated_references_never_verify_even_when_they_match() {
        let set = ReferenceSet { authenticated: vec![], unauthenticated: vec![exe("t", "1.2.3", A)] };
        let v = evaluate(&seen("t", WIN, ArtifactFormat::PeX64, A), &set);
        assert_eq!((v.reference, v.result, v.version), (ReferenceState::Unauthenticated, MatchResult::MatchesUnauthenticated, None));
        assert_eq!(evaluate(&seen("t", WIN, ArtifactFormat::PeX64, B), &set).result, MatchResult::MismatchesUnauthenticated);
    }

    #[test]
    fn an_archive_installer_or_package_digest_is_never_compared_with_an_installed_file() {
        for (kind, format) in [(ArtifactKind::Archive, "zip"), (ArtifactKind::Installer, "pe-x64"), (ArtifactKind::Package, "gzip")] {
            let set = auth(vec![r("t", kind, format, WIN, "1.0.0", A)]);
            let v = evaluate(&seen("t", WIN, ArtifactFormat::PeX64, A), &set);
            assert_eq!(v.result, MatchResult::Incomparable, "{kind:?}: even an equal value must not authenticate an extracted file");
            assert!(v.version.is_none());
        }
    }

    #[test]
    fn a_matching_digest_of_another_kind_or_format_never_authenticates() {
        let observed = |f| seen("t", WIN, f, A);
        // script reference vs an executable file
        let script_ref = auth(vec![r("t", ArtifactKind::Script, "script-shebang", WIN, "1.0.0", A)]);
        assert_eq!(evaluate(&observed(ArtifactFormat::PeX64), &script_ref).result, MatchResult::Incomparable);
        // executable reference vs a script file
        let exe_ref = auth(vec![exe("t", "1.0.0", A)]);
        assert_eq!(evaluate(&observed(ArtifactFormat::ScriptShebang), &exe_ref).result, MatchResult::Incomparable);
        // plugin file reference vs a standalone executable (and the reverse can never bind either: no hashed file is a plugin file)
        let plugin_ref = auth(vec![r("t", ArtifactKind::PluginFile, "pe-x64", WIN, "1.0.0", A)]);
        assert_eq!(evaluate(&observed(ArtifactFormat::PeX64), &plugin_ref).result, MatchResult::Incomparable);
        // PE reference vs a Mach-O file on the same (mislabelled) platform, and a Mach-O reference on a Windows target
        let pe_ref = auth(vec![exe("t", "1.0.0", A)]);
        assert_eq!(evaluate(&seen("t", WIN, ArtifactFormat::MachOArm64, A), &pe_ref).result, MatchResult::Incomparable);
        let macho_on_windows = auth(vec![r("t", ArtifactKind::Executable, "macho-arm64", WIN, "1.0.0", A)]);
        assert_eq!(evaluate(&observed(ArtifactFormat::PeX64), &macho_on_windows).result, MatchResult::Incomparable);
        assert_eq!(
            evaluate(&observed(ArtifactFormat::MachOArm64), &macho_on_windows).result,
            MatchResult::Incomparable,
            "malformed: Mach-O cannot belong to a Windows target"
        );
        // another architecture of the same OS
        let arm_pe = auth(vec![r("t", ArtifactKind::Executable, "pe-arm64", WIN, "1.0.0", A)]);
        assert_eq!(evaluate(&observed(ArtifactFormat::PeX64), &arm_pe).result, MatchResult::Incomparable);
        // an unverified format: the file's bytes could not be identified
        assert_eq!(evaluate(&observed(ArtifactFormat::Unrecognised), &exe_ref).result, MatchResult::Incomparable);
        // missing / malformed format evidence in the reference
        for bad in ["", "pe-exe", "PE-X64", "exe", "unrecognised", "pe-x64 "] {
            let set = auth(vec![r("t", ArtifactKind::Executable, bad, WIN, "1.0.0", A)]);
            assert_eq!(evaluate(&observed(ArtifactFormat::PeX64), &set).result, MatchResult::Incomparable, "{bad:?}");
        }
        // control: the exactly matching artifact
        assert_eq!(evaluate(&observed(ArtifactFormat::PeX64), &exe_ref).result, MatchResult::MatchesAuthenticated);
    }

    #[test]
    fn a_reference_of_another_type_does_not_turn_into_a_mismatch_either() {
        // a DIFFERENT digest under an unbound reference says nothing about this file: no "conflicted" verdict is created
        let set = auth(vec![r("t", ArtifactKind::Script, "script-shebang", WIN, "1.0.0", B)]);
        let v = evaluate(&seen("t", WIN, ArtifactFormat::PeX64, A), &set);
        assert_eq!(v.result, MatchResult::Incomparable);
    }

    #[test]
    fn conflicting_authenticated_versions_make_the_set_ambiguous_and_nothing_verifies() {
        let observed = seen("t", WIN, ArtifactFormat::PeX64, A);
        // same bytes, two versions
        let same_bytes = auth(vec![exe("t", "1.0.0", A), exe("t", "2.0.0", A)]);
        let v = evaluate(&observed, &same_bytes);
        assert_eq!((v.result, v.version), (MatchResult::Incomparable, None));
        // same version, two digests
        let same_version = auth(vec![exe("t", "1.0.0", A), exe("t", "1.0.0", B)]);
        assert_eq!(evaluate(&observed, &same_version).result, MatchResult::Incomparable);
        // control: different versions of different builds are fine, and the matching one is reported
        let fine = auth(vec![exe("t", "1.0.0", B), exe("t", "2.0.0", A)]);
        let v = evaluate(&observed, &fine);
        assert_eq!((v.result, v.version.as_deref()), (MatchResult::MatchesAuthenticated, Some("2.0.0")));
        // an unbound reference does not take part in the conflict
        let with_unbound = auth(vec![exe("t", "2.0.0", A), r("t", ArtifactKind::Archive, "zip", WIN, "9.9.9", A)]);
        assert_eq!(evaluate(&observed, &with_unbound).result, MatchResult::MatchesAuthenticated);
    }

    #[test]
    fn the_authenticated_set_wins_over_a_conflicting_unauthenticated_one_and_the_epoch_tracks_every_change() {
        let mut set = ReferenceSet { authenticated: vec![exe("t", "1.0.0", A)], unauthenticated: vec![exe("t", "9.9.9", B)] };
        assert_eq!(
            evaluate(&seen("t", WIN, ArtifactFormat::PeX64, B), &set).result,
            MatchResult::MismatchesAuthenticated,
            "a self-declared digest cannot override the authenticated one"
        );
        let e1 = set.epoch();
        assert_eq!(e1, set.epoch(), "stable");
        set.unauthenticated[0].sha256 = A.to_string();
        assert_ne!(set.epoch(), e1, "tampering with any reference changes the epoch");
        assert_ne!(ReferenceSet::default().epoch(), e1);
    }

    #[test]
    fn the_format_is_established_from_bytes_and_unclear_input_is_unrecognised() {
        use ArtifactFormat::*;
        for f in [
            PeX64,
            PeArm64,
            PeX86,
            MachOArm64,
            MachOX64,
            MachOUniversalArm64,
            MachOUniversalX64,
            MachOUniversalArm64X64,
            ElfX64,
            ElfArm64,
            ScriptShebang,
            Zip,
            Gzip,
        ] {
            let bytes = synthetic_file(f, b"payload");
            assert_eq!(sniff_reader(&bytes, bytes.len() as u64, &mut std::io::Cursor::new(&bytes)), f, "{f:?}");
            assert_eq!(ArtifactFormat::parse(f.as_str()), Some(f));
        }
        assert_eq!(sniff(b""), Unrecognised);
        assert_eq!(sniff(b"MZ"), Unrecognised, "a bare MZ is not a PE file");
        assert_eq!(sniff(b"stub"), Unrecognised);
        // a PE header cut short, or with a wrong signature or an unknown machine
        let pe = synthetic_file(PeX64, b"");
        assert_eq!(sniff(&pe[..0x84]), Unrecognised);
        let mut bad_sig = pe.clone();
        bad_sig[0x80] = b'X';
        assert_eq!(sniff(&bad_sig), Unrecognised);
        let mut unknown_machine = pe.clone();
        unknown_machine[0x84] = 0x01;
        unknown_machine[0x85] = 0x01;
        assert_eq!(sniff(&unknown_machine), Unrecognised);
        // a Java class file has the universal Mach-O magic but not a small architecture count
        assert_eq!(sniff(&[0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 52]), Unrecognised);
        assert_eq!(sniff(b"#!"), Unrecognised);
        assert_eq!(sniff(b"#!/bin/sh"), Unrecognised, "no end of the first line within the bytes seen");
        // the name never matters: there is no name in this API at all
        assert_eq!(sniff(&synthetic_file(ScriptShebang, b"echo hi")), ScriptShebang);
        assert!(PeX64.consistent_with("windows-x64") && !PeX64.consistent_with("macos-arm64"));
        assert!(MachOUniversalArm64.consistent_with("macos-arm64") && !MachOUniversalArm64.consistent_with("macos-x64"));
        assert!(
            MachOUniversalX64.consistent_with("macos-x64") && !MachOUniversalX64.consistent_with("macos-arm64"),
            "Rosetta is not native arm64"
        );
        assert!(MachOUniversalArm64X64.consistent_with("macos-arm64") && MachOUniversalArm64X64.consistent_with("macos-x64"));
        assert!(!MachOUniversal.consistent_with("macos-arm64"), "the legacy container-only label fits no target");
        assert!(!MachOUniversalArm64.consistent_with("windows-x64"));
        assert!(Zip.consistent_with("windows-x64") && Zip.consistent_with("macos-arm64"));
    }

    fn judged_fat(bytes: &[u8]) -> ArtifactFormat {
        sniff_reader(&bytes[..bytes.len().min(4096)], bytes.len() as u64, &mut std::io::Cursor::new(bytes.to_vec()))
    }

    const ARM: u32 = CPU_ARM64;
    const X64: u32 = CPU_X64;

    #[test]
    fn a_universal_mach_o_is_judged_by_its_verified_slices_not_by_its_magic() {
        use ArtifactFormat::*;
        for style in [FatStyle::Be32, FatStyle::Be64, FatStyle::Le32, FatStyle::Le64] {
            assert_eq!(judged_fat(&synthetic_fat(&[ARM], style)), MachOUniversalArm64, "{style:?}");
            assert_eq!(judged_fat(&synthetic_fat(&[X64], style)), MachOUniversalX64, "{style:?}");
            assert_eq!(judged_fat(&synthetic_fat(&[X64, ARM], style)), MachOUniversalArm64X64, "{style:?}");
            assert_eq!(judged_fat(&synthetic_fat(&[ARM, X64], style)), MachOUniversalArm64X64, "{style:?}");
        }
        // an x86_64-only universal file is NOT compatible with a native Apple Silicon target
        assert!(!MachOUniversalX64.consistent_with("macos-arm64"));
        // the container header alone (no reader for the slices) is never enough
        assert_eq!(sniff(&synthetic_fat(&[ARM], FatStyle::Be32)), Unrecognised);
    }

    #[test]
    fn malformed_universal_headers_fail_closed() {
        use ArtifactFormat::Unrecognised;
        let good = synthetic_fat(&[X64, ARM], FatStyle::Be32);
        assert_ne!(judged_fat(&good), Unrecognised, "control");
        let put_be = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_be_bytes());
        // entry 0 starts at byte 8: cpu(8) subtype(12) offset(16) size(20) align(24); entry 1 at 28
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("zero architectures", {
                let mut b = good.clone();
                put_be(&mut b, 4, 0);
                b
            }),
            ("absurd architecture count", {
                let mut b = good.clone();
                put_be(&mut b, 4, 0x4000_0000);
                b
            }),
            ("count larger than the table that is present", {
                let mut b = good.clone();
                put_be(&mut b, 4, 8);
                b.truncate(40);
                b
            }),
            ("table truncated inside an entry", good[..20].to_vec()),
            ("header only", good[..8].to_vec()),
            ("offset points past the end of the file", {
                let mut b = good.clone();
                put_be(&mut b, 16, 0x00ff_0000);
                b
            }),
            ("offset overlaps the table", {
                let mut b = good.clone();
                put_be(&mut b, 16, 4);
                b
            }),
            ("slice runs past the end of the file", {
                let mut b = good.clone();
                put_be(&mut b, 20, 0x0100_0000);
                b
            }),
            ("slice shorter than a Mach-O header", {
                let mut b = good.clone();
                put_be(&mut b, 20, 8);
                b
            }),
            ("zero-length slice", {
                let mut b = good.clone();
                put_be(&mut b, 20, 0);
                b
            }),
            ("huge offset and size", {
                let mut b = good.clone();
                put_be(&mut b, 16, u32::MAX);
                put_be(&mut b, 20, u32::MAX);
                b
            }),
            ("overlapping slices", {
                let mut b = good.clone();
                put_be(&mut b, 36, 0x1000 + 0x10);
                b
            }),
            ("the same CPU twice", {
                let mut b = good.clone();
                put_be(&mut b, 28, X64);
                b
            }),
            ("slice header does not carry the Mach-O magic", {
                let mut b = good.clone();
                b[0x1000] = 0;
                b
            }),
            ("slice header CPU disagrees with the table", {
                let mut b = good.clone();
                b[0x1000 + 4] = 0x09;
                b
            }),
            ("an unsupported (32-bit i386) slice", {
                let mut b = good.clone();
                put_be(&mut b, 8, 7);
                b[0x1000 + 4..0x1000 + 8].copy_from_slice(&7u32.to_le_bytes());
                b
            }),
            ("a slice in an unsupported container (32-bit Mach-O magic)", {
                let mut b = good.clone();
                b[0x1000..0x1004].copy_from_slice(&[0xce, 0xfa, 0xed, 0xfe]);
                b
            }),
            ("file shorter than the declared slices", good[..0x1080].to_vec()),
        ];
        for (name, bytes) in cases {
            assert_eq!(judged_fat(&bytes), Unrecognised, "{name}");
        }
        // an unreadable handle (the slice reads fail): fail closed
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("denied"))
            }
        }
        impl std::io::Seek for Broken {
            fn seek(&mut self, _: std::io::SeekFrom) -> std::io::Result<u64> {
                Ok(0)
            }
        }
        assert_eq!(sniff_reader(&good[..4096.min(good.len())], good.len() as u64, &mut Broken), Unrecognised);
    }

    #[test]
    fn a_java_class_file_and_other_cafebabe_files_are_not_universal_binaries() {
        let mut class = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 52];
        class.extend_from_slice(&[0u8; 200]);
        assert_eq!(judged_fat(&class), ArtifactFormat::Unrecognised);
    }

    #[test]
    fn an_x64_only_universal_digest_never_authenticates_on_apple_silicon_and_the_legacy_label_binds_nothing() {
        let mac = "macos-arm64";
        let reference = |format: &str| r("t", ArtifactKind::Executable, format, mac, "1.0.0", A);
        // the file is x86_64-only universal; every reference variant refuses to bind it on an arm64 target
        for f in ["macho-universal-x64", "macho-universal", "macho-universal-arm64", "macho-universal-arm64-x64"] {
            let v = evaluate(&seen("t", mac, ArtifactFormat::MachOUniversalX64, A), &auth(vec![reference(f)]));
            assert_eq!(v.result, MatchResult::Incomparable, "reference format {f} vs x64-only file on arm64");
        }
        // the genuine arm64-capable cases bind
        for (file, f) in [
            (ArtifactFormat::MachOUniversalArm64, "macho-universal-arm64"),
            (ArtifactFormat::MachOUniversalArm64X64, "macho-universal-arm64-x64"),
        ] {
            let v = evaluate(&seen("t", mac, file, A), &auth(vec![reference(f)]));
            assert_eq!(v.result, MatchResult::MatchesAuthenticated, "{f}");
        }
        // the legacy label (container magic only) binds to nothing, even to itself
        let v = evaluate(&seen("t", mac, ArtifactFormat::MachOUniversal, A), &auth(vec![reference("macho-universal")]));
        assert_eq!(v.result, MatchResult::Incomparable);
    }

    #[test]
    fn the_compiled_in_authenticated_list_is_empty_in_this_phase() {
        assert!(
            BUILT_IN_AUTHENTICATED.is_empty(),
            "no independently authenticated digest evidence exists yet; adding one needs a reviewed policy change"
        );
    }
}
