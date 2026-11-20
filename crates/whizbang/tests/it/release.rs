//! What the binary needs from the machine it is installed on (plan m4, task R1, item 2),
//! read from the binary's own file: on Linux, the shared libraries it names and the
//! versions of their symbols it asks for; on the Mac, the minimum system it states.
//!
//! The file read is the binary cargo built for these tests, which is linked as a release
//! build is: by the same toolchain, against the same libraries, under the same
//! `.cargo/config.toml`. It is read as bytes and never started. The readers take what
//! they need of the two formats and nothing else, and each test first holds that its
//! reader found something, so that a reader that finds nothing does not pass.

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn binary_file() -> Vec<u8> {
    let binary = crate::wired::binary();
    std::fs::read(binary.get_program()).expect("the binary's file is read")
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

#[cfg(target_os = "linux")]
mod elf {
    use super::u32_at;

    fn u16_at(bytes: &[u8], at: usize) -> usize {
        usize::from(u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap()))
    }

    fn u64_at(bytes: &[u8], at: usize) -> usize {
        usize::try_from(u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())).unwrap()
    }

    /// One section's header: its type, where it is, its size, and the two fields whose
    /// meaning its type gives.
    #[derive(Clone, Copy)]
    struct Section {
        kind: u32,
        offset: usize,
        size: usize,
        link: usize,
        info: usize,
    }

    /// The text that starts at `at` in a table of texts, up to its zero byte.
    fn text(bytes: &[u8], table: Section, at: usize) -> String {
        let from = table.offset + at;
        let len = bytes[from..]
            .iter()
            .position(|b| *b == 0)
            .expect("the text ends");
        String::from_utf8(bytes[from..from + len].to_vec()).expect("the text is ASCII")
    }

    /// What a 64-bit little-endian ELF file asks of the machine: each shared library it
    /// names (`DT_NEEDED`), and each version of a library's symbols it needs
    /// (`.gnu.version_r`), as the library and the version.
    pub fn needs(bytes: &[u8]) -> (Vec<String>, Vec<(String, String)>) {
        assert_eq!(&bytes[..4], b"\x7fELF", "an ELF file");
        assert_eq!((bytes[4], bytes[5]), (2, 1), "64-bit, little-endian");
        let (table, each, count) = (
            u64_at(bytes, 0x28),
            u16_at(bytes, 0x3a),
            u16_at(bytes, 0x3c),
        );
        let sections: Vec<Section> = (0..count)
            .map(|n| {
                let at = table + n * each;
                Section {
                    kind: u32_at(bytes, at + 4),
                    offset: u64_at(bytes, at + 0x18),
                    size: u64_at(bytes, at + 0x20),
                    link: usize::try_from(u32_at(bytes, at + 0x28)).unwrap(),
                    info: usize::try_from(u32_at(bytes, at + 0x2c)).unwrap(),
                }
            })
            .collect();

        // SHT_DYNAMIC: entries of a tag and a value; DT_NEEDED is tag 1, its value a place
        // in the table of texts the section links to.
        let mut libraries = Vec::new();
        for dynamic in sections.iter().filter(|s| s.kind == 6) {
            let texts = sections[dynamic.link];
            for entry in 0..dynamic.size / 16 {
                let at = dynamic.offset + entry * 16;
                if u64_at(bytes, at) == 1 {
                    libraries.push(text(bytes, texts, u64_at(bytes, at + 8)));
                }
            }
        }

        // SHT_GNU_verneed: `info` records, each a file and a chain of versions.
        let mut versions = Vec::new();
        for needed in sections.iter().filter(|s| s.kind == 0x6fff_fffe) {
            let texts = sections[needed.link];
            let mut at = needed.offset;
            for _ in 0..needed.info {
                let aux_count = u16_at(bytes, at + 2);
                let file = text(
                    bytes,
                    texts,
                    usize::try_from(u32_at(bytes, at + 4)).unwrap(),
                );
                let mut aux = at + usize::try_from(u32_at(bytes, at + 8)).unwrap();
                for _ in 0..aux_count {
                    let name = usize::try_from(u32_at(bytes, aux + 8)).unwrap();
                    versions.push((file.clone(), text(bytes, texts, name)));
                    aux += usize::try_from(u32_at(bytes, aux + 12)).unwrap();
                }
                at += usize::try_from(u32_at(bytes, at + 12)).unwrap();
            }
        }
        (libraries, versions)
    }

    /// The numbers of a version written as a prefix and numbers apart by dots, such as
    /// `GLIBC_2.34`; none when it is not of that prefix or holds anything else.
    pub fn numbers(version: &str, prefix: &str) -> Option<Vec<u32>> {
        version
            .strip_prefix(prefix)?
            .split('.')
            .map(|part| part.parse().ok())
            .collect()
    }
}

/// The Linux binary runs on Ubuntu 22.04 as it is installed, with nothing added: every
/// shared library it names is the C library's own (glibc 2.35 there) or the compiler's
/// runtime library that the C library itself depends on (`libgcc_s.so.1`, of GCC 12
/// there), and every version of a symbol it asks for is one those have.
#[cfg(target_os = "linux")]
#[test]
fn the_linux_binary_needs_only_the_c_library_of_ubuntu_22_04() {
    // The files of glibc, and the one library beside them.
    const OF_THE_C_LIBRARY: [&str; 7] = [
        "libc.so.6",
        "libm.so.6",
        "libpthread.so.0",
        "libdl.so.2",
        "librt.so.1",
        "libutil.so.1",
        "ld-linux-x86-64.so.2",
    ];
    const COMPILER_RUNTIME: &str = "libgcc_s.so.1";

    let (libraries, versions) = elf::needs(&binary_file());
    assert!(
        libraries.iter().any(|l| l == "libc.so.6"),
        "the reader finds the C library among {libraries:?}"
    );
    for library in &libraries {
        assert!(
            OF_THE_C_LIBRARY.contains(&library.as_str()) || library == COMPILER_RUNTIME,
            "the binary needs {library}, which a bare Ubuntu 22.04 may not hold"
        );
    }
    assert!(
        versions
            .iter()
            .any(|(file, version)| file == "libc.so.6" && version.starts_with("GLIBC_")),
        "the reader finds a version of the C library among {versions:?}"
    );
    for (file, version) in &versions {
        if file == COMPILER_RUNTIME {
            let numbers = elf::numbers(version, "GCC_");
            assert!(
                numbers.as_ref().is_some_and(|n| n[0] <= 12),
                "{file} is asked for {version}, and Ubuntu 22.04 has it of GCC 12"
            );
        } else {
            assert!(OF_THE_C_LIBRARY.contains(&file.as_str()), "{file}");
            let numbers = elf::numbers(version, "GLIBC_");
            assert!(
                numbers
                    .as_ref()
                    .is_some_and(|n| n.as_slice() <= [2, 35].as_slice()),
                "{file} is asked for {version}, and Ubuntu 22.04 has glibc 2.35"
            );
        }
    }
    // The reader of versions, on what it must and must not take.
    assert_eq!(elf::numbers("GLIBC_2.34", "GLIBC_"), Some(vec![2, 34]));
    assert_eq!(elf::numbers("GLIBC_2.2.5", "GLIBC_"), Some(vec![2, 2, 5]));
    assert!(vec![2, 2, 5] <= vec![2, 35] && vec![2, 36] > vec![2, 35]);
    assert_eq!(elf::numbers("GLIBC_PRIVATE", "GLIBC_"), None);
    assert_eq!(elf::numbers("GCC_3.0", "GLIBC_"), None);
}

/// The Mac binary says the oldest system it runs on, and says macOS 15.0: the build
/// version its file states (`LC_BUILD_VERSION`) is for macOS, with that minimum, which
/// `.cargo/config.toml` sets for every build.
#[cfg(target_os = "macos")]
#[test]
fn the_mac_binary_states_macos_15_as_its_minimum() {
    const LC_BUILD_VERSION: u32 = 0x32;
    const MACOS: u32 = 1;

    let bytes = binary_file();
    assert_eq!(u32_at(&bytes, 0), 0xfeed_facf, "a 64-bit Mach-O file");
    let commands = u32_at(&bytes, 16);
    let mut at = 32;
    let mut stated = Vec::new();
    for _ in 0..commands {
        let (command, size) = (u32_at(&bytes, at), u32_at(&bytes, at + 4));
        if command == LC_BUILD_VERSION {
            // The platform, then the minimum as major, minor and patch in 16, 8 and 8 bits.
            let minimum = u32_at(&bytes, at + 12);
            stated.push((
                u32_at(&bytes, at + 8),
                (minimum >> 16, (minimum >> 8) & 0xff, minimum & 0xff),
            ));
        }
        at += usize::try_from(size).unwrap();
    }
    assert_eq!(
        stated,
        [(MACOS, (15, 0, 0))],
        "one build version, for macOS 15.0"
    );
}
