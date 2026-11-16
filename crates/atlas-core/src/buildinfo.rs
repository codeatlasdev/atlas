//! Which binary is this: version, channel, commit and build id. `atlas version` prints it, and every crash report
//! (`crate::crash`, `crate::crash_native`) carries it, so a report attached to an issue names the exact build and the
//! symbols published for it (`scripts/symbols.sh`, `docs/release.md`).
//!
//! The **build id** is the linker's identity of the executable: the ELF `NT_GNU_BUILD_ID` note on Linux, the Mach-O
//! `LC_UUID` on macOS, the CodeView signature (PDB GUID + age) on Windows. The symbol archive of a release names or
//! matches its debug files by it, so a crash file finds its symbols without trusting a version string. A PE built
//! without a PDB has none: there the build id is absent and version + commit identify the build.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::LazyLock;

use semver::Version;

use crate::channel::Channel;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Short commit hash of the build, `unknown` when it was built outside git (see `build.rs`).
pub const COMMIT: &str = env!("ATLAS_COMMIT");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    pub version: String,
    /// The channel the version was cut for (`alpha`, `beta`, `stable`); `unknown` for a label Atlas does not ship.
    pub channel: String,
    pub commit: String,
    /// Lowercase hex; `None` where the executable carries none.
    pub build_id: Option<String>,
}

/// The channel a `version` string was cut for.
pub fn channel_of(version: &str) -> String {
    Version::parse(version)
        .ok()
        .and_then(|v| Channel::of_version(&v))
        .map_or("unknown", Channel::as_str)
        .to_string()
}

/// This process's build, read once.
pub fn current() -> &'static BuildInfo {
    static CURRENT: LazyLock<BuildInfo> = LazyLock::new(|| BuildInfo {
        version: VERSION.to_string(),
        channel: channel_of(VERSION),
        commit: COMMIT.to_string(),
        build_id: std::env::current_exe().ok().and_then(|p| build_id_of(&p)),
    });
    &CURRENT
}

/// The text of `atlas version`.
pub fn render(b: &BuildInfo, os: &str, arch: &str) -> String {
    format!(
        "atlas {}\n  channel: {}\n  commit:  {}\n  build:   {}\n  os:      {}\n  arch:    {}",
        b.version,
        b.channel,
        b.commit,
        b.build_id.as_deref().unwrap_or("none"),
        os,
        arch,
    )
}

/// The build id of the executable at `path`; `None` when it has none or is not a format read here.
pub fn build_id_of(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut magic = [0u8; 4];
    f.read_exact(&mut magic).ok()?;
    f.seek(SeekFrom::Start(0)).ok()?;
    match magic {
        [0x7f, b'E', b'L', b'F'] => elf_build_id(&mut f),
        // Mach-O 64-bit, little endian (`MH_MAGIC_64` as the bytes on disk).
        [0xcf, 0xfa, 0xed, 0xfe] => macho_uuid(&mut f),
        [b'M', b'Z', ..] => pe_codeview_id(&mut f),
        _ => None,
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn read_at<R: Read + Seek>(r: &mut R, at: u64, len: usize) -> Option<Vec<u8>> {
    r.seek(SeekFrom::Start(at)).ok()?;
    let mut v = vec![0; len];
    r.read_exact(&mut v).ok()?;
    Some(v)
}

/// `NT_GNU_BUILD_ID` from the `PT_NOTE` segments of a little-endian 64-bit ELF (every Linux target Atlas ships).
fn elf_build_id<R: Read + Seek>(r: &mut R) -> Option<String> {
    let h = read_at(r, 0, 64)?;
    if h[4] != 2 || h[5] != 1 {
        return None; // not 64-bit little endian
    }
    let u16_at = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let u32_at = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let u64_at = |b: &[u8], o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let (phoff, phentsize, phnum) = (u64_at(&h, 32), u16_at(&h, 54), u16_at(&h, 56));
    if phentsize < 56 {
        return None;
    }
    for i in 0..phnum {
        let ph = read_at(r, phoff + (i * phentsize) as u64, 56)?;
        const PT_NOTE: u32 = 4;
        if u32_at(&ph, 0) != PT_NOTE {
            continue;
        }
        let (off, size) = (u64_at(&ph, 8), u64_at(&ph, 32) as usize);
        if size > 1 << 20 {
            continue;
        }
        let notes = read_at(r, off, size)?;
        let mut p = 0;
        while p + 12 <= notes.len() {
            let (namesz, descsz, ty) = (
                u32_at(&notes, p) as usize,
                u32_at(&notes, p + 4) as usize,
                u32_at(&notes, p + 8),
            );
            let name = p + 12;
            let desc = name + namesz.div_ceil(4) * 4;
            let end = desc + descsz;
            if end > notes.len() {
                break;
            }
            const NT_GNU_BUILD_ID: u32 = 3;
            if ty == NT_GNU_BUILD_ID && notes[name..name + namesz].starts_with(b"GNU") {
                return Some(hex(&notes[desc..end]));
            }
            p = desc + descsz.div_ceil(4) * 4;
        }
    }
    None
}

/// `LC_UUID` of a little-endian 64-bit Mach-O, formatted as the 32 hex digits `dwarfdump --uuid` prints (no dashes,
/// lowercase).
fn macho_uuid<R: Read + Seek>(r: &mut R) -> Option<String> {
    let h = read_at(r, 0, 32)?;
    let ncmds = u32::from_le_bytes(h[16..20].try_into().unwrap());
    let mut at = 32u64;
    for _ in 0..ncmds {
        let c = read_at(r, at, 8)?;
        let (cmd, size) = (
            u32::from_le_bytes(c[0..4].try_into().unwrap()),
            u32::from_le_bytes(c[4..8].try_into().unwrap()),
        );
        const LC_UUID: u32 = 0x1b;
        if cmd == LC_UUID {
            return Some(hex(&read_at(r, at + 8, 16)?));
        }
        if size < 8 {
            return None;
        }
        at += u64::from(size);
    }
    None
}

/// The PDB signature of a PE image: the `RSDS` CodeView record of its debug directory, as the GUID's 32 hex digits
/// followed by the age in hex (what a symbol server and `dumpbin /headers` show, lowercase).
fn pe_codeview_id<R: Read + Seek>(r: &mut R) -> Option<String> {
    let u16_at = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
    let u32_at = |b: &[u8], o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize;
    let dos = read_at(r, 0, 64)?;
    let pe = u32_at(&dos, 0x3c) as u64;
    let h = read_at(r, pe, 24)?;
    if &h[0..4] != b"PE\0\0" {
        return None;
    }
    let (nsec, optsz) = (u16_at(&h, 6), u16_at(&h, 20));
    let opt = read_at(r, pe + 24, optsz)?;
    // PE32+ keeps its data directories at 112, PE32 at 96; the debug directory is the seventh.
    let dirs = match u16_at(&opt, 0) {
        0x20b => 112,
        0x10b => 96,
        _ => return None,
    };
    let (rva, size) = (u32_at(&opt, dirs + 6 * 8), u32_at(&opt, dirs + 6 * 8 + 4));
    if rva == 0 || size == 0 {
        return None;
    }
    let sections = read_at(r, pe + 24 + optsz as u64, nsec * 40)?;
    let offset_of = |rva: usize| {
        sections.chunks_exact(40).find_map(|s| {
            let (vsize, va, rawsize, raw) =
                (u32_at(s, 8), u32_at(s, 12), u32_at(s, 16), u32_at(s, 20));
            (rva >= va && rva < va + vsize.max(rawsize)).then(|| raw + (rva - va))
        })
    };
    let dir = read_at(r, offset_of(rva)? as u64, size.min(28 * 16))?;
    for e in dir.chunks_exact(28) {
        const IMAGE_DEBUG_TYPE_CODEVIEW: usize = 2;
        if u32_at(e, 12) != IMAGE_DEBUG_TYPE_CODEVIEW {
            continue;
        }
        let rec = read_at(r, u32_at(e, 24) as u64, 24)?;
        if &rec[0..4] != b"RSDS" {
            continue;
        }
        // A GUID is written as one u32, two u16 and eight bytes; the first three are little endian on disk.
        let (d1, d2, d3) = (u32_at(&rec, 4), u16_at(&rec, 8), u16_at(&rec, 10));
        return Some(format!(
            "{d1:08x}{d2:04x}{d3:04x}{}{:x}",
            hex(&rec[12..20]),
            u32_at(&rec, 20)
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(build_id: Option<&str>) -> BuildInfo {
        BuildInfo {
            version: "1.5.0-alpha.25".into(),
            channel: "alpha".into(),
            commit: "0123456789ab".into(),
            build_id: build_id.map(String::from),
        }
    }

    /// #318: `atlas version` names channel, commit and build id, one per line, aligned.
    #[test]
    fn version_text_names_channel_commit_and_build_id() {
        assert_eq!(
            render(&info(Some("d249ae24dcf95bb0")), "linux", "x86_64"),
            "atlas 1.5.0-alpha.25\n  channel: alpha\n  commit:  0123456789ab\n  build:   d249ae24dcf95bb0\n  os:      linux\n  arch:    x86_64"
        );
        assert!(render(&info(None), "windows", "x86_64").contains("build:   none"));
    }

    #[test]
    fn the_channel_is_the_one_the_version_label_was_cut_for() {
        assert_eq!(channel_of("1.5.0-alpha.25"), "alpha");
        assert_eq!(channel_of("1.5.0-beta.2"), "beta");
        assert_eq!(channel_of("1.5.0-rc.1"), "beta");
        assert_eq!(channel_of("1.5.0"), "stable");
        assert_eq!(channel_of("1.5.0-nightly.1"), "unknown");
        assert_eq!(channel_of("not a version"), "unknown");
    }

    /// The running test executable is a real linker output: its build id is what `readelf -n` prints, and the value
    /// `current()` reports for this process (the symbol archive of a release is keyed by it).
    #[cfg(target_os = "linux")]
    #[test]
    fn the_build_id_is_the_one_the_linker_put_in_this_executable() {
        let exe = std::env::current_exe().unwrap();
        let id = build_id_of(&exe).expect("rustc links with --build-id");
        assert!(
            id.len() >= 16 && id.bytes().all(|c| c.is_ascii_hexdigit()),
            "{id}"
        );
        assert_eq!(current().build_id.as_deref(), Some(id.as_str()));
        if let Ok(o) = std::process::Command::new("readelf")
            .arg("-n")
            .arg(&exe)
            .output()
        {
            let text = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = text.lines().find(|l| l.contains("Build ID:")) {
                assert_eq!(line.trim().trim_start_matches("Build ID:").trim(), id);
            }
        }
    }

    #[test]
    fn a_file_that_is_not_an_executable_has_no_build_id() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x");
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        assert_eq!(build_id_of(&p), None);
        assert_eq!(build_id_of(&d.path().join("missing")), None);
        // A truncated ELF header is not a panic.
        std::fs::write(&p, [0x7f, b'E', b'L', b'F', 2, 1]).unwrap();
        assert_eq!(build_id_of(&p), None);
    }

    /// A hand-built PE32+ image with a debug directory holding one CodeView record, as `link.exe` / `lld-link` write
    /// for a PDB. The id is the GUID in its usual order plus the age.
    #[test]
    fn a_pe_codeview_signature_is_the_pdb_guid_and_age() {
        let mut m = vec![0u8; 0x400];
        m[0..2].copy_from_slice(b"MZ");
        m[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        m[0x80..0x84].copy_from_slice(b"PE\0\0");
        m[0x86..0x88].copy_from_slice(&1u16.to_le_bytes()); // one section
        m[0x94..0x96].copy_from_slice(&240u16.to_le_bytes()); // optional header size
        m[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes()); // PE32+
        let dir_entry = 0x98 + 112 + 6 * 8;
        m[dir_entry..dir_entry + 4].copy_from_slice(&0x2000u32.to_le_bytes()); // debug directory RVA
        m[dir_entry + 4..dir_entry + 8].copy_from_slice(&28u32.to_le_bytes());
        let sec = 0x98 + 240;
        m[sec..sec + 6].copy_from_slice(b".rdata");
        m[sec + 8..sec + 12].copy_from_slice(&0x200u32.to_le_bytes()); // virtual size
        m[sec + 12..sec + 16].copy_from_slice(&0x2000u32.to_le_bytes()); // virtual address
        m[sec + 16..sec + 20].copy_from_slice(&0x200u32.to_le_bytes()); // raw size
        m[sec + 20..sec + 24].copy_from_slice(&0x200u32.to_le_bytes()); // raw pointer
        // The directory entry at file offset 0x200: type 2 (CodeView), pointer to the record at 0x300.
        m[0x200 + 12..0x200 + 16].copy_from_slice(&2u32.to_le_bytes());
        m[0x200 + 24..0x200 + 28].copy_from_slice(&0x300u32.to_le_bytes());
        m[0x300..0x304].copy_from_slice(b"RSDS");
        m[0x304..0x308].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        m[0x308..0x30a].copy_from_slice(&0x5566u16.to_le_bytes());
        m[0x30a..0x30c].copy_from_slice(&0x7788u16.to_le_bytes());
        m[0x30c..0x314].copy_from_slice(&[0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00]);
        m[0x314..0x318].copy_from_slice(&3u32.to_le_bytes()); // age
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("atlas.exe");
        std::fs::write(&p, &m).unwrap();
        assert_eq!(
            build_id_of(&p).as_deref(),
            Some("112233445566778899aabbccddeeff003")
        );
        // An image with no debug directory has none.
        m[dir_entry..dir_entry + 8].fill(0);
        std::fs::write(&p, &m).unwrap();
        assert_eq!(build_id_of(&p), None);
    }

    /// A hand-built Mach-O header with one LC_UUID among other load commands (what `lld`/`ld64` emit).
    #[test]
    fn a_macho_uuid_is_found_among_other_load_commands() {
        let mut m = vec![0xcf, 0xfa, 0xed, 0xfe];
        m.extend([0u8; 12]); // cputype, cpusubtype, filetype
        m.extend(2u32.to_le_bytes()); // ncmds
        m.extend(24u32.to_le_bytes()); // sizeofcmds
        m.extend([0u8; 8]); // flags, reserved
        m.extend(0x32u32.to_le_bytes()); // LC_BUILD_VERSION: skipped
        m.extend(8u32.to_le_bytes());
        m.extend(0x1bu32.to_le_bytes());
        m.extend(24u32.to_le_bytes());
        m.extend((0u8..16).map(|i| i * 17));
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("atlas");
        std::fs::write(&p, &m).unwrap();
        assert_eq!(
            build_id_of(&p).as_deref(),
            Some("00112233445566778899aabbccddeeff")
        );
    }
}
