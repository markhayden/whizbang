//! The ustar archive a bundle is, and the playground's "Export run" (ADR 0048, "Bundles"):
//! a writer whose headers are fixed, so equal contents make equal bytes, and a reader
//! that trusts nothing it reads.
//!
//! Each member's header is the same but for its name, its size and its checksum: a
//! plain file (type `0`), mode 0644, uid and gid 0, no names of users, and the mtime the
//! writer is given. A name of at most 100 bytes is written in `name`; a longer one is
//! split at a `/` between `prefix` (at most 155 bytes) and `name` (at most 100), at the
//! first `/` that leaves `name` short enough; a name that cannot be so written is
//! refused, and so is a name with a NUL in it. The archive ends with two blocks of
//! zeros.
//!
//! The reader reads one member at a time: its header, whose checksum and size are
//! checked before a byte of the member is read, then its bytes, which are held to what
//! is left of the bound on the whole, and its padding, which must be zeros. It refuses
//! any member that is not a plain file (a link, a device, a directory, a FIFO), and
//! anything but zeros after the end, and joins `prefix` and `name` with a `/`.

use std::io::Read;

/// The size of a block: a header, and the unit a member's bytes are padded to.
pub const BLOCK: usize = 512;

/// The most bytes of ustar's `name` field.
pub const NAME_BYTES: usize = 100;

/// The most bytes of ustar's `prefix` field.
pub const PREFIX_BYTES: usize = 155;

/// Why a name cannot be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unwritable {
    /// The name, cut at 64 characters.
    pub name: String,
}

impl std::fmt::Display for Unwritable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} cannot be a member's name: a name is at most {NAME_BYTES} bytes, or a \
             part of at most {PREFIX_BYTES} bytes, a /, and a part of at most \
             {NAME_BYTES} bytes, with no NUL",
            self.name
        )
    }
}

impl std::error::Error for Unwritable {}

/// The `prefix` and the `name` a member's name is written as, or why it cannot be.
pub fn split(name: &str) -> Result<(&str, &str), Unwritable> {
    let refused = || Unwritable {
        name: name.chars().take(64).collect(),
    };
    if name.is_empty() || name.contains('\0') {
        return Err(refused());
    }
    if name.len() <= NAME_BYTES {
        return Ok(("", name));
    }
    // The first `/` that leaves a `name` short enough and a `prefix` that fits; neither
    // part may be empty.
    let slashes = name.match_indices('/').map(|(at, _)| at);
    let split = slashes
        .filter(|&at| at > 0 && at <= PREFIX_BYTES && name.len() - at - 1 <= NAME_BYTES)
        .find(|&at| at + 1 < name.len());
    match split {
        Some(at) => Ok((&name[..at], &name[at + 1..])),
        None => Err(refused()),
    }
}

/// The header of one member of `bytes` bytes, or why its name cannot be written.
pub fn header(name: &str, bytes: u64, mtime: u64) -> Result<[u8; BLOCK], Unwritable> {
    let (prefix, short) = split(name)?;
    let mut header = [0u8; BLOCK];
    header[..short.len()].copy_from_slice(short.as_bytes());
    let octal = |field: &mut [u8], value: u64| {
        let text = format!("{value:0width$o}\0", width = field.len() - 1);
        // A value too large for its field keeps its last digits; a bundle is far below
        // the size and an export's time far below the date where that happens.
        let text = &text.as_bytes()[text.len() - field.len()..];
        field.copy_from_slice(text);
    };
    octal(&mut header[100..108], 0o644);
    octal(&mut header[108..116], 0);
    octal(&mut header[116..124], 0);
    octal(&mut header[124..136], bytes);
    octal(&mut header[136..148], mtime);
    header[148..156].copy_from_slice(b"        ");
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
    let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
    let text = format!("{sum:06o}\0 ");
    header[148..156].copy_from_slice(text.as_bytes());
    Ok(header)
}

/// An archive being written, in memory: members in the order they are added.
#[derive(Debug, Default)]
pub struct Writer {
    out: Vec<u8>,
    mtime: u64,
}

impl Writer {
    /// An empty archive whose every member has this mtime, in seconds since the Unix
    /// epoch: 0 for a bundle, a run's end for "Export run".
    pub fn new(mtime: u64) -> Writer {
        Writer {
            out: Vec::new(),
            mtime,
        }
    }

    /// Adds one member: its header and its bytes, padded to a block.
    pub fn add(&mut self, name: &str, bytes: &[u8]) -> Result<(), Unwritable> {
        let header = header(name, bytes.len() as u64, self.mtime)?;
        self.out.extend_from_slice(&header);
        self.out.extend_from_slice(bytes);
        let pad = (BLOCK - bytes.len() % BLOCK) % BLOCK;
        self.out.extend(std::iter::repeat_n(0u8, pad));
        Ok(())
    }

    /// How many bytes the archive has so far, without its end.
    pub fn len(&self) -> u64 {
        self.out.len() as u64
    }

    /// Whether nothing was added.
    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    /// The archive, with its end: two blocks of zeros.
    pub fn finish(mut self) -> Vec<u8> {
        self.out.extend(std::iter::repeat_n(0u8, 2 * BLOCK));
        self.out
    }
}

/// Why an archive could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// The file could not be read: the reader's words.
    Io(String),
    /// It ended inside a header or a member.
    Short,
    /// A header that is not ustar's, or whose checksum or a number is wrong.
    Header(String),
    /// A member that is not a plain file: its name and its type, in words.
    NotAFile { name: String, kind: &'static str },
    /// More bytes than the bound on the whole.
    TooLarge { limit: u64 },
    /// Something other than zeros after the end.
    AfterTheEnd,
}

impl std::fmt::Display for Unread {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unread::Io(why) => write!(f, "the file could not be read: {why}"),
            Unread::Short => f.write_str("the archive ends inside a header or a member"),
            Unread::Header(why) => write!(f, "a header is not ustar's: {why}"),
            Unread::NotAFile { name, kind } => {
                write!(f, "the member {name:?} is {kind}, not a plain file")
            }
            Unread::TooLarge { limit } => {
                write!(f, "the archive is over its bound of {limit} bytes")
            }
            Unread::AfterTheEnd => f.write_str("there is something after the archive's end"),
        }
    }
}

impl std::error::Error for Unread {}

/// A member's header, read: its name, `prefix` and `name` joined, and its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Head {
    pub name: String,
    pub bytes: u64,
}

/// A field of text: its bytes up to the first NUL.
fn text(field: &[u8]) -> Result<String, Unread> {
    let end = field.iter().position(|b| *b == 0).unwrap_or(field.len());
    if field[end..].iter().any(|b| *b != 0) {
        return Err(Unread::Header(
            "a name has something after its end".to_owned(),
        ));
    }
    String::from_utf8(field[..end].to_vec())
        .map_err(|_| Unread::Header("a name is not UTF-8".to_owned()))
}

/// A number field: octal digits, ended by a NUL or a space.
fn number(field: &[u8]) -> Result<u64, Unread> {
    let digits = field.split(|b| *b == 0 || *b == b' ').next().unwrap_or(&[]);
    let digits = std::str::from_utf8(digits).ok().filter(|d| !d.is_empty());
    digits
        .and_then(|digits| u64::from_str_radix(digits, 8).ok())
        .ok_or_else(|| Unread::Header("a number is not octal".to_owned()))
}

/// What a type flag names, in words, when it is not a plain file's.
fn kind(flag: u8) -> Option<&'static str> {
    match flag {
        b'0' | 0 => None,
        b'1' => Some("a hard link"),
        b'2' => Some("a symbolic link"),
        b'3' | b'4' => Some("a device"),
        b'5' => Some("a directory"),
        b'6' => Some("a FIFO"),
        _ => Some("of a type that is not a plain file"),
    }
}

/// Reads an archive one member at a time, holding the bytes it has read to a bound.
pub struct Reader<R: Read> {
    from: R,
    /// The most bytes the archive may have.
    limit: u64,
    /// How many it has read.
    read: u64,
    /// Whether the end was met.
    ended: bool,
}

impl<R: Read> Reader<R> {
    /// A reader of an archive of at most `limit` bytes.
    pub fn new(from: R, limit: u64) -> Reader<R> {
        Reader {
            from,
            limit,
            read: 0,
            ended: false,
        }
    }

    /// How many bytes were read.
    pub fn read_so_far(&self) -> u64 {
        self.read
    }

    /// Fills `buffer` from the file, or says why not: the bound is held first.
    fn fill(&mut self, buffer: &mut [u8]) -> Result<(), Unread> {
        let wanted = buffer.len() as u64;
        if self.read.saturating_add(wanted) > self.limit {
            return Err(Unread::TooLarge { limit: self.limit });
        }
        self.from
            .read_exact(buffer)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::UnexpectedEof => Unread::Short,
                _ => Unread::Io(error.to_string()),
            })?;
        self.read += wanted;
        Ok(())
    }

    /// The next member's header, or `None` at the archive's end. The header's checksum
    /// is checked, its type held to a plain file's, and its size to what is left of the
    /// bound, before a byte of the member is read.
    pub fn next_head(&mut self) -> Result<Option<Head>, Unread> {
        if self.ended {
            return Ok(None);
        }
        let mut header = [0u8; BLOCK];
        self.fill(&mut header)?;
        if header.iter().all(|b| *b == 0) {
            // The end: a second block of zeros, then nothing but zeros.
            self.fill(&mut header)?;
            if header.iter().any(|b| *b != 0) {
                return Err(Unread::AfterTheEnd);
            }
            self.ended = true;
            let mut rest = [0u8; BLOCK];
            loop {
                let mut got = 0;
                while got < BLOCK {
                    let read = self
                        .from
                        .read(&mut rest[got..])
                        .map_err(|error| Unread::Io(error.to_string()))?;
                    if read == 0 {
                        break;
                    }
                    got += read;
                }
                self.read = self.read.saturating_add(got as u64);
                if self.read > self.limit {
                    return Err(Unread::TooLarge { limit: self.limit });
                }
                if rest[..got].iter().any(|b| *b != 0) {
                    return Err(Unread::AfterTheEnd);
                }
                if got < BLOCK {
                    return Ok(None);
                }
            }
        }
        if &header[257..263] != b"ustar\0" || &header[263..265] != b"00" {
            return Err(Unread::Header("it has no ustar magic".to_owned()));
        }
        let stated = number(&header[148..156])?;
        let mut summed = header;
        summed[148..156].copy_from_slice(b"        ");
        let sum: u64 = summed.iter().map(|b| u64::from(*b)).sum();
        if sum != stated {
            return Err(Unread::Header("its checksum is wrong".to_owned()));
        }
        let short = text(&header[..100])?;
        let prefix = text(&header[345..500])?;
        let name = match prefix.is_empty() {
            true => short,
            false => format!("{prefix}/{short}"),
        };
        if let Some(kind) = kind(header[156]) {
            return Err(Unread::NotAFile { name, kind });
        }
        let bytes = number(&header[124..136])?;
        let padded = bytes.div_ceil(BLOCK as u64) * BLOCK as u64;
        // Its bytes, and the end's two blocks after them, must fit.
        if self
            .read
            .saturating_add(padded)
            .saturating_add(2 * BLOCK as u64)
            > self.limit
        {
            return Err(Unread::TooLarge { limit: self.limit });
        }
        Ok(Some(Head { name, bytes }))
    }

    /// The bytes of the member whose header was just read, and its padding passed.
    pub fn bytes(&mut self, head: &Head) -> Result<Vec<u8>, Unread> {
        let size =
            usize::try_from(head.bytes).map_err(|_| Unread::TooLarge { limit: self.limit })?;
        let mut bytes = vec![0u8; size];
        self.fill(&mut bytes)?;
        let pad = (BLOCK - size % BLOCK) % BLOCK;
        let mut padding = [0u8; BLOCK];
        self.fill(&mut padding[..pad])?;
        if padding.iter().any(|b| *b != 0) {
            return Err(Unread::Header(format!(
                "the padding after the member {:?} is not zeros",
                head.name
            )));
        }
        Ok(bytes)
    }
}
