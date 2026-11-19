//! The token of the Hub's command-line sign-in (plan 1C, D12; ADR 0012, "A download's
//! token"): where the Hub's tool keeps it, and the reading of it.
//!
//! **Where it is** is worked out by [`token_file`] from values it is handed, and never
//! from the environment of this process: the binary's `main` hands it its own, by the
//! one function that names the real place, and nothing else does (`just check` refuses
//! a call of that function anywhere but `main`: `scripts/check-sign-in.sh`). Every test
//! hands it a temporary folder.
//!
//! **What is read** is one small file, by [`read`]: a regular file that only its owner
//! can read and write, of printable ASCII and nothing else but one line break at its
//! end, at least as long as a secret and at most [`TOKEN_MAX_BYTES`]. A refusal says
//! what is wrong in fixed words, how to sign in or which key to set instead, and quotes
//! nothing of the file and nothing of its path. The bytes read are overwritten with
//! zeros before the reading ends; the token itself is handed back in a [`Token`],
//! whose `{:?}` shows nothing of it.
//!
//! The variables `HF_TOKEN` and `HUGGING_FACE_HUB_TOKEN`, which the Hub's own tools
//! take in place of the file, are not read: the owner chose the sign-in, and whizbang's
//! own other way in is a key of the env file (D12).

use std::ffi::OsStr;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use whizbang_core::contract::SECRET_MIN_BYTES;
use whizbang_core::store::fs::{Fs as _, RealFs, this_user};

/// The most bytes a token file may hold. A token of the Hub is some forty; a file
/// longer than this is no sign-in's.
pub const TOKEN_MAX_BYTES: u64 = 512;

/// What a refusal of the sign-in says to do: sign in, or name a key instead.
pub const HOW_TO_SIGN_IN: &str = "sign in with the Hub's command-line tool (`hf auth login`), \
     or set the token in the env file and name its variable as `key_env` for the origin in \
     the config's `[[models.tokens]]`";

/// Where the Hub's command-line tool keeps the token of its sign-in, from the values
/// of the four variables that move it, as the Hub's documentation of its environment
/// variables gives them
/// (`https://huggingface.co/docs/huggingface_hub/package_reference/environment_variables`):
///
/// 1. `HF_TOKEN_PATH`, the file itself, when it is set;
/// 2. else `token` in `HF_HOME`, when that is set;
/// 3. else `huggingface/token` in `XDG_CACHE_HOME`, which the Hub uses "only when
///    `HF_HOME` is not set";
/// 4. else `.cache/huggingface/token` in the home folder.
///
/// A value that is set and empty is as one not set. A value that is set and is not an
/// absolute path answers no place at all, rather than the next: the Hub's tool would
/// look elsewhere than whizbang could, and a sign-in read from another place than the
/// one the person named is not theirs. With no place, the sign-in is not there.
///
/// It reads nothing: not the environment, and not the disk.
pub fn token_file(
    hf_token_path: Option<&OsStr>,
    hf_home: Option<&OsStr>,
    xdg_cache_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<PathBuf> {
    // Set and not empty.
    fn set(value: Option<&OsStr>) -> Option<&Path> {
        value.filter(|value| !value.is_empty()).map(Path::new)
    }
    fn absolute(path: &Path) -> Option<PathBuf> {
        path.is_absolute().then(|| path.to_owned())
    }
    if let Some(path) = set(hf_token_path) {
        return absolute(path);
    }
    if let Some(folder) = set(hf_home) {
        return absolute(folder).map(|folder| folder.join("token"));
    }
    if let Some(folder) = set(xdg_cache_home) {
        return absolute(folder).map(|folder| folder.join("huggingface").join("token"));
    }
    let home = absolute(set(home)?)?;
    Some(home.join(".cache").join("huggingface").join("token"))
}

/// A token read from the sign-in. Its `{:?}` shows that it is one, and nothing of it.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// The token itself, for the one header that carries it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)")
    }
}

/// Why the sign-in's token was not read. Fixed words; nothing of the file, and no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInRefused {
    /// Where the sign-in is is not known: no variable names it and there is no home
    /// folder, or one names a place that is not an absolute path.
    NoPlace,
    /// Nothing is there.
    NotThere,
    /// What is there is not a regular file.
    NotAFile,
    /// Others than its owner can read or write it.
    Open,
    /// It is not the file of the user whizbang runs as.
    NotYours,
    /// Its folder can be written by others than its owner, who could put another
    /// token in its place.
    FolderOpen,
    /// It could not be opened or read.
    Unreadable,
    /// It is empty, or holds only a line break.
    Empty,
    /// It is longer than [`TOKEN_MAX_BYTES`].
    TooLong,
    /// It is shorter than a secret may be.
    TooShort,
    /// It holds something other than printable ASCII and one line break at its end.
    NotAToken,
}

impl std::fmt::Display for SignInRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let why = match self {
            SignInRefused::NoPlace => {
                "where the Hub's sign-in keeps its token is not known: no home folder, or \
                 a variable that names its place is not an absolute path"
            }
            SignInRefused::NotThere => "the Hub's sign-in has no token on this machine",
            SignInRefused::NotAFile => "the Hub's sign-in's token is not a regular file",
            SignInRefused::Open => {
                "the Hub's sign-in's token file can be read or written by others than its \
                 owner, so it is not used; make it its owner's alone (mode 600), or sign in \
                 again, which writes it so"
            }
            SignInRefused::NotYours => {
                "the Hub's sign-in's token file is not the file of the user whizbang runs \
                 as, so it is not used; sign in as that user"
            }
            SignInRefused::FolderOpen => {
                "the folder of the Hub's sign-in's token file can be written by others than \
                 its owner, who could put another token there, so it is not used; make the \
                 folder its owner's alone (mode 700), as the Hub's tool makes it"
            }
            SignInRefused::Unreadable => "the Hub's sign-in's token file could not be read",
            SignInRefused::Empty => "the Hub's sign-in's token file is empty",
            SignInRefused::TooLong => "the Hub's sign-in's token file is longer than a token",
            SignInRefused::TooShort => "the Hub's sign-in's token file is shorter than a token",
            SignInRefused::NotAToken => {
                "the Hub's sign-in's token file holds something that is not a token"
            }
        };
        write!(f, "{why}; {HOW_TO_SIGN_IN}")
    }
}

impl std::error::Error for SignInRefused {}

/// Reads the token at this place ([`token_file`]), as the Hub's tool writes it: the
/// token, and at most one line break after it (`\n`, or `\r\n`), which is taken off.
/// No more than [`TOKEN_MAX_BYTES`] and one byte is read.
///
/// - A link is followed, as the Hub's tool follows one; what it leads to must be a
///   regular file.
/// - **A file that others than its owner can read or write is refused**
///   ([`SignInRefused::Open`]), never read and warned of: the Hub's tool writes the
///   file for its owner alone (mode 600) and so does every sign-in since; one that
///   others can read is a token that may have been read already, and one that others
///   can write may be another's token.
/// - **A file that is not the user's own is refused** ([`SignInRefused::NotYours`]):
///   a link someone planted, or a file another user or the system made, may hold
///   another's token, which whizbang would send under this user's name. The user is
///   the one the store's file system learns (`store::fs::this_user`).
/// - **A file whose folder others can write is refused**
///   ([`SignInRefused::FolderOpen`]): whoever can write the folder can put another
///   file at the name. The folder is the one the name is in, as the Hub's tool makes
///   it, 700; a link at the name is followed for the file and not for this look.
/// - **Nothing that is not a regular file is opened.** The name is looked at first, a
///   link followed: a FIFO, a socket, a device or a folder is refused
///   ([`SignInRefused::NotAFile`]) before any open, since opening a FIFO waits for a
///   writer. The open itself does not wait (`O_NONBLOCK`), so a name swapped for a
///   FIFO between the look and the open is opened at once, and refused by the second
///   look, which is of the file that was opened: its kind, its owner, its mode and its
///   length are all asked of that.
pub fn read(place: &Path) -> Result<Token, SignInRefused> {
    match std::fs::metadata(place) {
        Ok(found) if found.is_file() => {}
        Ok(_) => return Err(SignInRefused::NotAFile),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(SignInRefused::NotThere);
        }
        Err(_) => return Err(SignInRefused::Unreadable),
    }
    let user = this_user().map_err(|_| SignInRefused::Unreadable)?;
    let folder = place.parent().ok_or(SignInRefused::NoPlace)?;
    let writers = RealFs
        .who_may_write(folder)
        .map_err(|_| SignInRefused::Unreadable)?;
    if writers.group || writers.anyone {
        return Err(SignInRefused::FolderOpen);
    }
    let file = open_without_waiting(place).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => SignInRefused::NotThere,
        _ => SignInRefused::Unreadable,
    })?;
    let metadata = file.metadata().map_err(|_| SignInRefused::Unreadable)?;
    judge(&Opened::of(&metadata), user)?;
    let mut bytes = Vec::with_capacity(usize::try_from(TOKEN_MAX_BYTES + 1).unwrap_or(0));
    let read = file
        .take(TOKEN_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| SignInRefused::Unreadable);
    let token = read.and_then(|_| token_of(&bytes));
    // The bytes are overwritten before anything else is done: what is kept is the
    // token, once.
    bytes.iter_mut().for_each(|byte| *byte = 0);
    token
}

/// The flag that makes an open not wait: for a FIFO, for a writer. The standard
/// library has no name for it and the workspace has no crate that does, so it is
/// written here for the two systems whizbang builds for (`SPEC.md` section 1).
#[cfg(target_os = "macos")]
const O_NONBLOCK: i32 = 0x0004;
#[cfg(target_os = "linux")]
const O_NONBLOCK: i32 = 0o4000;

/// Opens the file to read, without waiting.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn open_without_waiting(place: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NONBLOCK)
        .open(place)
}

/// What is asked of the file that was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Opened {
    regular: bool,
    owner: u32,
    mode: u32,
    length: u64,
}

impl Opened {
    fn of(metadata: &std::fs::Metadata) -> Opened {
        use std::os::unix::fs::MetadataExt;
        Opened {
            regular: metadata.is_file(),
            owner: metadata.uid(),
            mode: metadata.mode(),
            length: metadata.len(),
        }
    }
}

/// Whether the file that was opened may be read as this user's token: a regular
/// file, this user's, that nobody else may read or write, and not longer than a token
/// file is.
fn judge(opened: &Opened, user: u32) -> Result<(), SignInRefused> {
    if !opened.regular {
        return Err(SignInRefused::NotAFile);
    }
    if opened.owner != user {
        return Err(SignInRefused::NotYours);
    }
    if opened.mode & 0o077 != 0 {
        return Err(SignInRefused::Open);
    }
    if opened.length > TOKEN_MAX_BYTES {
        return Err(SignInRefused::TooLong);
    }
    Ok(())
}

/// The token these bytes of a token file hold.
fn token_of(bytes: &[u8]) -> Result<Token, SignInRefused> {
    if bytes.len() as u64 > TOKEN_MAX_BYTES {
        return Err(SignInRefused::TooLong);
    }
    let text = bytes
        .strip_suffix(b"\r\n")
        .or_else(|| bytes.strip_suffix(b"\n"))
        .unwrap_or(bytes);
    if text.is_empty() {
        return Err(SignInRefused::Empty);
    }
    if !text.iter().all(u8::is_ascii_graphic) {
        return Err(SignInRefused::NotAToken);
    }
    if text.len() < SECRET_MIN_BYTES {
        return Err(SignInRefused::TooShort);
    }
    // ASCII alone, so this cannot fail.
    let text = std::str::from_utf8(text).map_err(|_| SignInRefused::NotAToken)?;
    Ok(Token(text.to_owned()))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::PathBuf;

    use super::{Opened, SignInRefused, judge, token_file, token_of};

    #[test]
    fn a_token_file_that_is_not_this_users_or_that_others_may_read_or_write_is_refused() {
        // The decision over what was asked of the opened file, with made-up numbers:
        // no file of another user is needed, and nothing runs as another.
        let user = 501;
        let own = Opened {
            regular: true,
            owner: user,
            mode: 0o100_600,
            length: 40,
        };
        assert_eq!(judge(&own, user), Ok(()));
        let another = Opened {
            owner: user + 1,
            ..own
        };
        assert_eq!(judge(&another, user), Err(SignInRefused::NotYours));
        let root = Opened { owner: 0, ..own };
        assert_eq!(judge(&root, user), Err(SignInRefused::NotYours));
        for mode in [0o100_640, 0o100_604, 0o100_620, 0o100_602, 0o100_700] {
            let open = Opened { mode, ..own };
            let expected = if mode & 0o077 == 0 {
                Ok(())
            } else {
                Err(SignInRefused::Open)
            };
            assert_eq!(judge(&open, user), expected, "{mode:o}");
        }
        let fifo = Opened {
            regular: false,
            ..own
        };
        assert_eq!(judge(&fifo, user), Err(SignInRefused::NotAFile));
        let long = Opened { length: 513, ..own };
        assert_eq!(judge(&long, user), Err(SignInRefused::TooLong));
    }

    fn os(text: &str) -> Option<&OsStr> {
        Some(OsStr::new(text))
    }

    #[test]
    fn the_place_is_hf_token_path_then_hf_home_then_xdg_cache_home_then_the_home_folder() {
        // Made-up folders: nothing is read.
        let (path, hf, xdg, home) = (
            os("/made/up/token-file"),
            os("/made/up/hf"),
            os("/made/up/cache"),
            os("/made/up/dwelling"),
        );
        let place = |p: PathBuf| Some(p);
        assert_eq!(
            token_file(path, hf, xdg, home),
            place("/made/up/token-file".into())
        );
        assert_eq!(
            token_file(None, hf, xdg, home),
            place("/made/up/hf/token".into())
        );
        assert_eq!(
            token_file(None, None, xdg, home),
            place("/made/up/cache/huggingface/token".into())
        );
        assert_eq!(
            token_file(None, None, None, home),
            place("/made/up/dwelling/.cache/huggingface/token".into())
        );
        assert_eq!(token_file(None, None, None, None), None);
        // Set and empty is not set.
        assert_eq!(
            token_file(os(""), os(""), os(""), home),
            place("/made/up/dwelling/.cache/huggingface/token".into())
        );
        // Set and relative is no place, and never the next one.
        assert_eq!(token_file(os("token"), hf, xdg, home), None);
        assert_eq!(token_file(None, os("~/hf"), xdg, home), None);
        assert_eq!(token_file(None, None, os("cache"), home), None);
        assert_eq!(token_file(None, None, None, os("home")), None);
    }

    #[test]
    fn a_token_file_loses_one_line_break_and_is_held_to_printable_ascii() {
        let made_up = ["made", "up", "token", "0123456789"].join("-");
        for written in [
            made_up.clone(),
            format!("{made_up}\n"),
            format!("{made_up}\r\n"),
        ] {
            let token = token_of(written.as_bytes()).unwrap();
            assert_eq!(token.expose(), made_up);
            assert_eq!(format!("{token:?}"), "Token(..)");
        }
        for (written, refused) in [
            (String::new(), SignInRefused::Empty),
            ("\n".to_owned(), SignInRefused::Empty),
            (format!("{made_up}\n\n"), SignInRefused::NotAToken),
            (format!(" {made_up}"), SignInRefused::NotAToken),
            (format!("{made_up}\t"), SignInRefused::NotAToken),
            (format!("{made_up}\u{e9}"), SignInRefused::NotAToken),
            ("short".to_owned(), SignInRefused::TooShort),
            ("x".repeat(513), SignInRefused::TooLong),
        ] {
            assert_eq!(token_of(written.as_bytes()), Err(refused), "{written:?}");
        }
        assert!(token_of("x".repeat(512).as_bytes()).is_ok());
    }

    #[test]
    fn a_fifo_opened_by_the_open_that_does_not_wait_is_opened_at_once_and_judged_no_regular_file() {
        // Follow-up of the review of N4: the name may be swapped for a FIFO between the
        // look and the open. The open that does not wait opens it at once, with no
        // writer, and the opened file is judged no regular file. Were the flag lost,
        // the open would wait for a writer: it runs on one thread of the test kit's,
        // waited for ten seconds at most, after which a writer is opened to let it go
        // and the test fails.
        let scratch = whizbang_testkit::scratch::Scratch::new("sign-in-fifo");
        let place = scratch.path().join("token");
        let made = std::process::Command::new("mkfifo").arg(&place).status();
        if !made.is_ok_and(|status| status.success()) {
            eprintln!("skip: no FIFO could be made here (mkfifo)");
            return;
        }
        let (ended, has_ended) = std::sync::mpsc::channel();
        let opening = place.clone();
        let waiting = whizbang_testkit::threads::spawn(move || {
            let opened = super::open_without_waiting(&opening)
                .and_then(|file| file.metadata())
                .map(|metadata| Opened::of(&metadata));
            let _ = ended.send(opened.map_err(|error| error.kind()));
        });
        let Ok(opened) = has_ended.recv_timeout(std::time::Duration::from_secs(10)) else {
            let writer = std::fs::OpenOptions::new().write(true).open(&place);
            drop(writer);
            let _ = waiting.join();
            panic!("the open waited at the FIFO for a writer");
        };
        waiting.join().unwrap();
        let opened = opened.unwrap();
        assert!(!opened.regular);
        assert_eq!(judge(&opened, opened.owner), Err(SignInRefused::NotAFile));
    }
}
