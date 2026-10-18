//! Phase 3B archive-policy tests. Every archive is built byte by byte in memory from made-up names and contents: nothing is
//! downloaded, decompressed, extracted or written to disk.

use super::*;

// ---- builders -------------------------------------------------------------------------------------------------------------------

#[derive(Clone)]
struct Z {
    name: Vec<u8>,
    data: Vec<u8>,
    size: u32,
    method: u16,
    flags: u16,
    host: u8,
    mode: u32,
    ext_extra: u32,
}

impl Z {
    fn file(name: &str, content: &[u8]) -> Z {
        Z {
            name: name.as_bytes().to_vec(),
            data: content.to_vec(),
            size: content.len() as u32,
            method: 0,
            flags: 0,
            host: 3,
            mode: 0o100644,
            ext_extra: 0,
        }
    }
    fn raw_name(mut self, n: &[u8]) -> Z {
        self.name = n.to_vec();
        self
    }
    fn mode(mut self, m: u32) -> Z {
        self.mode = m;
        self
    }
    fn dir(name: &str) -> Z {
        Z { mode: 0o040755, ext_extra: 0x10, ..Z::file(name, b"") }
    }
}

fn le16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn le32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn zip(entries: &[Z]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for e in entries {
        let off = out.len() as u32;
        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        le16(&mut out, 20);
        le16(&mut out, e.flags);
        le16(&mut out, e.method);
        le16(&mut out, 0);
        le16(&mut out, 0);
        le32(&mut out, 0);
        le32(&mut out, e.data.len() as u32);
        le32(&mut out, e.size);
        le16(&mut out, e.name.len() as u16);
        le16(&mut out, 0);
        out.extend_from_slice(&e.name);
        out.extend_from_slice(&e.data);

        central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        le16(&mut central, (u16::from(e.host) << 8) | 20);
        le16(&mut central, 20);
        le16(&mut central, e.flags);
        le16(&mut central, e.method);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, 0);
        le32(&mut central, e.data.len() as u32);
        le32(&mut central, e.size);
        le16(&mut central, e.name.len() as u16);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, (e.mode << 16) | e.ext_extra);
        le32(&mut central, off);
        central.extend_from_slice(&e.name);
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, entries.len() as u16);
    le16(&mut out, entries.len() as u16);
    le32(&mut out, central.len() as u32);
    le32(&mut out, cd_off);
    le16(&mut out, 0);
    out
}

struct T {
    name: Vec<u8>,
    data: Vec<u8>,
    flag: u8,
    mode: u32,
}

impl T {
    fn file(name: &str, content: &[u8]) -> T {
        T { name: name.as_bytes().to_vec(), data: content.to_vec(), flag: b'0', mode: 0o644 }
    }
    fn flag(mut self, f: u8) -> T {
        self.flag = f;
        self.data.clear();
        self
    }
    fn mode(mut self, m: u32) -> T {
        self.mode = m;
        self
    }
}

fn tar_header(e: &T, size: usize) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    h[..e.name.len().min(100)].copy_from_slice(&e.name[..e.name.len().min(100)]);
    let put = |h: &mut Vec<u8>, at: usize, text: String| h[at..at + text.len()].copy_from_slice(text.as_bytes());
    put(&mut h, 100, format!("{:07o}", e.mode));
    put(&mut h, 108, "0000000".into());
    put(&mut h, 116, "0000000".into());
    put(&mut h, 124, format!("{:011o}", size));
    put(&mut h, 136, "00000000000".into());
    h[156] = e.flag;
    h[257..262].copy_from_slice(b"ustar");
    h[263..265].copy_from_slice(b"00");
    h[148..156].copy_from_slice(b"        ");
    let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
    let text = format!("{sum:06o}\0 ");
    h[148..156].copy_from_slice(text.as_bytes());
    h
}

fn tar(entries: &[T]) -> Vec<u8> {
    let mut out = Vec::new();
    for e in entries {
        out.extend_from_slice(&tar_header(e, e.data.len()));
        out.extend_from_slice(&e.data);
        out.resize(out.len().next_multiple_of(512), 0);
    }
    out.extend_from_slice(&[0u8; 1024]);
    out
}

// ---- a made-up expected manifest -----------------------------------------------------------------------------------------------

const TOOL_BYTES: &[u8] = b"synthetic tool body";
const LICENSE_BYTES: &[u8] = b"synthetic licence text";

fn manifest() -> ExpectedManifest {
    ExpectedManifest {
        archive_sha256: "a".repeat(64),
        entries: vec![
            ExpectedEntry { name: "pkg/tool.bin".into(), size: TOOL_BYTES.len() as u64, sha256: sha256_hex(TOOL_BYTES), executable: true },
            ExpectedEntry {
                name: "pkg/LICENSE".into(),
                size: LICENSE_BYTES.len() as u64,
                sha256: sha256_hex(LICENSE_BYTES),
                executable: false,
            },
        ],
    }
}

fn good_zip() -> Vec<u8> {
    zip(&[Z::dir("pkg/"), Z::file("pkg/tool.bin", TOOL_BYTES).mode(0o100755), Z::file("pkg/LICENSE", LICENSE_BYTES)])
}

fn good_tar() -> Vec<u8> {
    tar(&[
        T::file("pkg/", b"").flag(b'5').mode(0o755),
        T::file("pkg/tool.bin", TOOL_BYTES).mode(0o755),
        T::file("pkg/LICENSE", LICENSE_BYTES),
    ])
}

fn codes(entries: &[EntryMeta]) -> Vec<&'static str> {
    check(entries, &manifest(), &Limits::default()).into_iter().map(|f| f.code).collect()
}

fn zip_codes(entries: &[Z]) -> Vec<&'static str> {
    codes(&read_zip(&zip(entries)).expect("readable"))
}

fn with_good(extra: Z) -> Vec<Z> {
    vec![Z::dir("pkg/"), Z::file("pkg/tool.bin", TOOL_BYTES).mode(0o100755), Z::file("pkg/LICENSE", LICENSE_BYTES), extra]
}

// ---- the accepted shape ---------------------------------------------------------------------------------------------------------

#[test]
fn the_expected_zip_and_tar_pass_with_no_finding_and_the_files_verify_by_their_own_digests() {
    for entries in [read_zip(&good_zip()).unwrap(), read_tar(&good_tar()).unwrap()] {
        assert_eq!(entries.len(), 3);
        assert_eq!(codes(&entries), Vec::<&str>::new());
    }
    let m = manifest();
    assert!(verify_extracted(&m.entries[0], TOOL_BYTES));
    assert!(!verify_extracted(&m.entries[0], b"synthetic tool bodY"));
    assert!(!verify_extracted(&m.entries[0], &good_zip()), "the archive is not the file inside it");
}

// ---- names ----------------------------------------------------------------------------------------------------------------------

#[test]
fn hostile_entry_names_are_found_in_both_formats() {
    let long = "a".repeat(300);
    let table: Vec<(Vec<u8>, &str)> = vec![
        (b"../evil".to_vec(), "path_traversal"),
        (b"pkg/../../evil".to_vec(), "path_traversal"),
        (b"/etc/evil".to_vec(), "absolute_path"),
        (b"//server/share/evil".to_vec(), "absolute_path"),
        (b"C:/evil".to_vec(), "drive_letter_path"),
        (b"C:evil".to_vec(), "drive_letter_path"),
        (b"pkg\\evil".to_vec(), "backslash_in_name"),
        (b"pkg/evil:stream".to_vec(), "colon_in_name"),
        (b"pkg/CON".to_vec(), "windows_reserved_name"),
        (b"pkg/nul.txt".to_vec(), "windows_reserved_name"),
        (b"pkg/Com1.bin".to_vec(), "windows_reserved_name"),
        (b"pkg/evil.".to_vec(), "trailing_dot_or_space"),
        (b"pkg/evil ".to_vec(), "trailing_dot_or_space"),
        (b"pkg/a<b".to_vec(), "windows_forbidden_character"),
        (b"pkg//evil".to_vec(), "empty_path_component"),
        (b"pkg/./evil".to_vec(), "dot_path_component"),
        (b"pkg/ev\x01il".to_vec(), "control_character_in_name"),
        ("pkg/\u{e9}vil".as_bytes().to_vec(), "non_ascii_name"),
        (b"pkg/\xff\xfe".to_vec(), "name_not_utf8"),
        (long.into_bytes(), "name_too_long"),
    ];
    for (name, code) in table {
        let from_zip = zip_codes(&with_good(Z::file("x", b"x").raw_name(&name)));
        assert!(from_zip.contains(&code), "zip {:?}: {from_zip:?}", String::from_utf8_lossy(&name));
        let t = tar(&[
            T::file("pkg/", b"").flag(b'5').mode(0o755),
            T::file("pkg/tool.bin", TOOL_BYTES).mode(0o755),
            T::file("pkg/LICENSE", LICENSE_BYTES),
            T { name: name.clone(), data: b"x".to_vec(), flag: b'0', mode: 0o644 },
        ]);
        let from_tar = codes(&read_tar(&t).expect("readable"));
        // (a tar name is limited to 100 + 155 bytes by the format; an over-long one is cut by the writer, so skip that detail)
        if name.len() <= 100 {
            assert!(from_tar.contains(&code), "tar {:?}: {from_tar:?}", String::from_utf8_lossy(&name));
        }
    }
}

#[test]
fn a_nul_inside_a_name_is_found_in_zip_headers() {
    let c = zip_codes(&with_good(Z::file("x", b"x").raw_name(b"pkg/a\0b")));
    assert!(c.contains(&"nul_in_name"), "{c:?}");
}

#[test]
fn duplicates_and_case_only_differences_are_found() {
    let c = zip_codes(&with_good(Z::file("pkg/tool.bin", b"another body")));
    assert!(c.contains(&"duplicate_entry"), "{c:?}");
    let c = zip_codes(&with_good(Z::file("PKG/TOOL.BIN", b"another body")));
    assert!(c.contains(&"case_collision"), "{c:?}");
}

// ---- kinds and permissions ------------------------------------------------------------------------------------------------------

#[test]
fn links_special_files_and_dangerous_permission_bits_are_found() {
    let c = zip_codes(&with_good(Z::file("pkg/link", b"tool.bin").mode(0o120777)));
    assert!(c.contains(&"symlink_entry"), "{c:?}");
    let mut reparse = Z::file("pkg/rp", b"x");
    reparse.host = 0;
    reparse.ext_extra = 0x400;
    reparse.mode = 0;
    assert!(zip_codes(&with_good(reparse)).contains(&"symlink_entry"));
    assert!(zip_codes(&with_good(Z::file("pkg/dev", b"").mode(0o020644))).contains(&"special_file_entry"));
    assert!(zip_codes(&with_good(Z::file("pkg/suid", b"x").mode(0o104755))).contains(&"setuid_setgid_or_sticky"));
    assert!(zip_codes(&with_good(Z::file("pkg/sgid", b"x").mode(0o102755))).contains(&"setuid_setgid_or_sticky"));
    assert!(zip_codes(&with_good(Z::file("pkg/sticky", b"x").mode(0o101755))).contains(&"setuid_setgid_or_sticky"));
    assert!(zip_codes(&with_good(Z::file("pkg/ww", b"x").mode(0o100666))).contains(&"world_writable_file"));
    let mut enc = Z::file("pkg/secret", b"x");
    enc.flags = 1;
    assert!(zip_codes(&with_good(enc)).contains(&"encrypted_entry"));

    for (flag, code) in [
        (b'2', "symlink_entry"),
        (b'1', "hardlink_entry"),
        (b'3', "special_file_entry"),
        (b'4', "special_file_entry"),
        (b'6', "special_file_entry"),
    ] {
        let t = tar(&[
            T::file("pkg/", b"").flag(b'5').mode(0o755),
            T::file("pkg/tool.bin", TOOL_BYTES).mode(0o755),
            T::file("pkg/LICENSE", LICENSE_BYTES),
            T::file("pkg/odd", b"").flag(flag),
        ]);
        let c = codes(&read_tar(&t).unwrap());
        assert!(c.contains(&code), "{flag}: {c:?}");
    }
}

#[test]
fn unexpected_entries_unexpected_executables_and_wrong_modes_are_found() {
    let c = zip_codes(&with_good(Z::file("pkg/extra.txt", b"x")));
    assert!(c.contains(&"unexpected_entry") && !c.contains(&"unexpected_executable"), "{c:?}");
    for name in ["pkg/run.bat", "pkg/x.sh", "pkg/y.ps1", "pkg/z.dll", "pkg/w.exe", "pkg/Q.CMD", "pkg/m.dylib"] {
        let c = zip_codes(&with_good(Z::file(name, b"x")));
        assert!(c.contains(&"unexpected_executable"), "{name}: {c:?}");
    }
    let c = zip_codes(&with_good(Z::file("pkg/noext", b"x").mode(0o100755)));
    assert!(c.contains(&"unexpected_executable"), "{c:?}");
    // an expected file whose executable bit differs from the manifest, and an unexpected folder
    let c = zip_codes(&[Z::dir("pkg/"), Z::file("pkg/tool.bin", TOOL_BYTES).mode(0o100644), Z::file("pkg/LICENSE", LICENSE_BYTES)]);
    assert!(c.contains(&"executable_bit_mismatch"), "{c:?}");
    let c = zip_codes(&with_good(Z::dir("other/")));
    assert!(c.contains(&"unexpected_entry"), "{c:?}");
    // a missing expected entry and a wrong size
    let c = zip_codes(&[Z::dir("pkg/"), Z::file("pkg/tool.bin", TOOL_BYTES).mode(0o100755)]);
    assert_eq!(c, vec!["missing_expected_entry"]);
    let c = zip_codes(&[Z::dir("pkg/"), Z::file("pkg/tool.bin", b"short").mode(0o100755), Z::file("pkg/LICENSE", LICENSE_BYTES)]);
    assert_eq!(c, vec!["size_mismatch"]);
}

// ---- limits ---------------------------------------------------------------------------------------------------------------------

#[test]
fn count_size_total_and_ratio_limits_are_enforced_from_the_headers() {
    let entries = read_zip(&good_zip()).unwrap();
    let tight = |l: Limits| check(&entries, &manifest(), &l).into_iter().map(|f| f.code).collect::<Vec<_>>();
    assert!(tight(Limits { max_entries: 2, ..Limits::default() }).contains(&"entry_count_limit"));
    assert!(tight(Limits { max_entry_size: 10, ..Limits::default() }).contains(&"entry_size_limit"));
    assert!(tight(Limits { max_total_size: 10, ..Limits::default() }).contains(&"total_size_limit"));
    // a "bomb": the header claims 200 MB from 1 KB
    let mut bomb = Z::file("pkg/tool.bin", &[0u8; 1000]).mode(0o100755);
    bomb.size = 200 * 1024 * 1024;
    bomb.method = 8;
    let b = zip(&[Z::dir("pkg/"), bomb, Z::file("pkg/LICENSE", LICENSE_BYTES)]);
    let c = codes(&read_zip(&b).unwrap());
    assert!(c.contains(&"compression_ratio_limit") && c.contains(&"entry_size_limit") && c.contains(&"total_size_limit"), "{c:?}");
    // a small, highly compressible file stays below the floor
    let mut small = Z::file("pkg/tool.bin", &[0u8; 1]).mode(0o100755);
    small.size = TOOL_BYTES.len() as u32;
    small.method = 8;
    let c = codes(&read_zip(&zip(&[Z::dir("pkg/"), small, Z::file("pkg/LICENSE", LICENSE_BYTES)])).unwrap());
    assert!(!c.contains(&"compression_ratio_limit"), "{c:?}");
}

// ---- malformed containers -------------------------------------------------------------------------------------------------------

#[test]
fn malformed_zip_containers_are_refused_not_guessed_at() {
    let good = good_zip();
    assert_eq!(read_zip(&good[..10]).unwrap_err(), "zip_too_short");
    assert_eq!(read_zip(&[0u8; 100]).unwrap_err(), "zip_end_record_missing");
    // zip64 markers
    let mut z64 = good.clone();
    let eocd = z64.len() - 22;
    z64[eocd + 16..eocd + 20].copy_from_slice(&[0xff; 4]);
    assert_eq!(read_zip(&z64).unwrap_err(), "zip64_unsupported");
    // multi-disk
    let mut md = good.clone();
    let eocd = md.len() - 22;
    md[eocd + 4] = 1;
    assert_eq!(read_zip(&md).unwrap_err(), "zip_multi_disk_unsupported");
    // central directory pointing outside the file
    let mut oob = good.clone();
    let eocd = oob.len() - 22;
    oob[eocd + 16..eocd + 20].copy_from_slice(&(good.len() as u32).to_le_bytes());
    assert_eq!(read_zip(&oob).unwrap_err(), "zip_central_directory_out_of_range");
    // an unsupported compression method
    let mut m = Z::file("pkg/tool.bin", TOOL_BYTES);
    m.method = 99;
    assert_eq!(read_zip(&zip(&[m])).unwrap_err(), "zip_compression_method_unsupported");
    // local header names a different entry than the central directory (the classic confusion)
    let mut mismatch = good.clone();
    let pos = mismatch.windows(12).position(|w| w == b"pkg/tool.bin").unwrap();
    mismatch[pos] = b'q';
    assert_eq!(read_zip(&mismatch).unwrap_err(), "zip_local_header_name_mismatch");
    // two entries sharing the same bytes
    let one = zip(&[Z::file("pkg/tool.bin", TOOL_BYTES)]);
    let mut two = Vec::new();
    {
        let cd_off = u32::from_le_bytes(one[one.len() - 6..one.len() - 2].try_into().unwrap()) as usize;
        let central = one[cd_off..one.len() - 22].to_vec();
        let second = central.clone();
        let mut cd = central.clone();
        cd.extend_from_slice(&second);
        two.extend_from_slice(&one[..cd_off]);
        let new_off = two.len() as u32;
        two.extend_from_slice(&cd);
        two.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        le16(&mut two, 0);
        le16(&mut two, 0);
        le16(&mut two, 2);
        le16(&mut two, 2);
        le32(&mut two, cd.len() as u32);
        le32(&mut two, new_off);
        le16(&mut two, 0);
    }
    // a second central entry for the very same local header: two entries share the same bytes
    assert_eq!(read_zip(&two).unwrap_err(), "zip_entries_overlap");
}

#[test]
fn malformed_tar_containers_are_refused_not_guessed_at() {
    let good = good_tar();
    let mut bad_sum = good.clone();
    bad_sum[0] ^= 1;
    assert_eq!(read_tar(&bad_sum).unwrap_err(), "tar_checksum_mismatch");
    let mut not_ustar = good.clone();
    not_ustar[257] = b'x';
    // (changing the magic also changes the checksum; either refusal is acceptable)
    assert!(read_tar(&not_ustar).is_err());
    let no_end = &good[..good.len() - 1024];
    assert_eq!(read_tar(no_end).unwrap_err(), "tar_end_marker_missing");
    let cut = &good[..512 * 2 + 100];
    assert_eq!(read_tar(cut).unwrap_err(), "tar_entry_data_out_of_range");
    for flag in *b"xgLK" {
        let t = tar(&[T::file("pkg/ext", b"").flag(flag)]);
        assert_eq!(read_tar(&t).unwrap_err(), "tar_extension_header_unsupported", "{flag}");
    }
    let mut big = tar(&[T::file("pkg/f", b"x")]);
    big[124] = 0x80; // base-256 size field
    assert!(read_tar(&big).is_err());
}

#[test]
fn no_prefix_of_and_no_single_byte_change_to_a_valid_archive_panics() {
    for archive in [good_zip(), good_tar()] {
        let is_zip = archive.starts_with(b"PK");
        for end in 0..archive.len() {
            let _ = if is_zip { read_zip(&archive[..end]).map(|e| codes(&e)) } else { read_tar(&archive[..end]).map(|e| codes(&e)) };
        }
        for i in 0..archive.len() {
            let mut b = archive.clone();
            b[i] ^= 0xff;
            let _ = if is_zip { read_zip(&b).map(|e| codes(&e)) } else { read_tar(&b).map(|e| codes(&e)) };
        }
    }
}

// ---- the manifest ---------------------------------------------------------------------------------------------------------------

#[test]
fn the_archive_digest_is_never_accepted_as_the_digest_of_an_extracted_file() {
    let mut m = manifest();
    m.entries[0].sha256 = m.archive_sha256.clone();
    let c: Vec<_> = check_manifest(&m, &Limits::default()).into_iter().map(|f| f.code).collect();
    assert_eq!(c, vec!["archive_digest_reused_as_file_digest"]);
    // and the real archive digest differs from every expected file digest
    let archive = good_zip();
    let mut m = manifest();
    m.archive_sha256 = sha256_hex(&archive);
    assert!(check_manifest(&m, &Limits::default()).is_empty());
    assert!(m.entries.iter().all(|e| e.sha256 != m.archive_sha256));
}

#[test]
fn a_malformed_or_unsafe_manifest_is_refused() {
    let l = Limits::default();
    let mut m = manifest();
    m.archive_sha256 = "XYZ".into();
    assert!(check_manifest(&m, &l).iter().any(|f| f.code == "manifest_archive_digest_malformed"));
    let mut m = manifest();
    m.entries[1].sha256 = "A".repeat(64);
    assert!(check_manifest(&m, &l).iter().any(|f| f.code == "manifest_file_digest_malformed"));
    let mut m = manifest();
    m.entries.clear();
    assert!(check_manifest(&m, &l).iter().any(|f| f.code == "manifest_empty"));
    let mut m = manifest();
    m.entries[1].name = "../escape".into();
    assert!(check_manifest(&m, &l).iter().any(|f| f.code == "manifest_entry_name_unsafe"));
    let mut m = manifest();
    m.entries[1].name = "PKG/TOOL.BIN".into();
    assert!(check_manifest(&m, &l).iter().any(|f| f.code == "manifest_duplicate_entry"));
}

// ---- hygiene --------------------------------------------------------------------------------------------------------------------

#[test]
fn findings_never_repeat_an_entry_name() {
    let hostile = "pkg/../SECRET-NAME-9d1f";
    let c = check(&read_zip(&zip(&with_good(Z::file(hostile, b"x")))).unwrap(), &manifest(), &Limits::default());
    assert!(!c.is_empty());
    assert!(!format!("{c:?}").contains("SECRET-NAME"));
}

#[test]
fn the_checker_has_no_file_system_process_network_or_decompression_capability() {
    let production = include_str!("archive_check.rs").split("#[cfg(test)]").next().unwrap();
    for needle in ["std::fs", "std::process", "std::net", "File::", "Command", "flate2", "inflate", "zip::", "tar::", "tokio", "reqwest"] {
        assert!(!production.contains(needle), "{needle} must not appear in the checker");
    }
}
