//! What `whizbang play` offers through the playground host (`whizbang_serve::play`; plan
//! m5, task H1; ADR 0041): each action of the registry, a call of one made from the
//! page's arguments as the stdio server makes one from a tool's; the backends' readiness
//! for `/api/hello`; and the file a running host keeps in its store, `play.json`, by which
//! a second `whizbang play` finds it. Nothing here runs anything: a call goes through the
//! executor's entry point, as every door's does.
//!
//! The fixture host of the playground's Chrome tier (task U1) builds the same
//! [`Playground`] over the test kit's fakes.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json, json};
use whizbang_core::contract::{ActionDescriptor, Call};
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_serve::play::{Play, Token};
use whizbang_serve::tools;

use crate::backend_status;
use crate::pack::OneWorker;
use crate::wiring::Over;

/// The name of the file a running host keeps in its store's folder.
pub const PLAY_FILE: &str = "play.json";

/// The actions of a registry, offered to the page, over what the executor was built
/// over.
pub struct Playground {
    descriptors: Vec<ActionDescriptor>,
    over: Arc<Over>,
    worker: Option<Arc<OneWorker>>,
}

impl Playground {
    /// The actions these descriptors describe, the backends `over` offers, and the
    /// pack's one worker of the process, stopped when the host ends.
    pub fn new(descriptors: Vec<ActionDescriptor>, over: Arc<Over>) -> Playground {
        let worker = over.worker().cloned();
        Playground {
            descriptors,
            over,
            worker,
        }
    }
}

impl Play for Playground {
    fn call(
        &self,
        action: &str,
        arguments: &Map<String, Json>,
    ) -> Result<(Call, Option<String>), String> {
        let Some(descriptor) = self.descriptors.iter().find(|d| d.name() == action) else {
            return Err("this build has no action of that name".to_owned());
        };
        tools::call_of(descriptor, arguments)
    }

    fn hello(&self) -> Map<String, Json> {
        let mut said = Map::new();
        let backends = match backend_status::readiness(&self.over) {
            Ok(listed) => json!(listed),
            Err(why) => json!({"refused": crate::door::printable(&why)}),
        };
        said.insert("backends".to_owned(), backends);
        said
    }

    fn closing(&self) {
        if let Some(worker) = &self.worker {
            worker.kill();
        }
    }
}

/// What a running host keeps in its store's folder: its port, its process and its
/// token, readable by its owner only (ADR 0041, "Reconnecting").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hosting {
    pub port: u16,
    pub process: u32,
    pub token: String,
}

/// The file of the store at `root`.
pub fn hosting_file(root: &Path) -> PathBuf {
    root.join(PLAY_FILE)
}

/// Writes the file of a running host, for its owner alone, in place of one a host that
/// ended without removing it left.
pub fn write_hosting(root: &Path, hosting: &Hosting) -> io::Result<()> {
    let path = hosting_file(root);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    let text = serde_json::to_vec(hosting).map_err(io::Error::other)?;
    file.write_all(&text)?;
    file.sync_all()
}

/// The file of a host of the store at `root`, when one is there, reads as one, and can
/// be trusted: the store's folder is this user's and nobody else may write it, and the
/// file is a regular file of this user's that nobody else may read or write. Anything
/// else is not read, and a launch goes on as if no host were there.
pub fn read_hosting(root: &Path) -> Option<Hosting> {
    let writers = RealFs.who_may_write(root).ok()?;
    if !writers.owner_is_this_user || writers.group || writers.anyone {
        return None;
    }
    let path = hosting_file(root);
    let found = std::fs::symlink_metadata(&path).ok()?;
    if !found.is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let user = whizbang_core::store::fs::this_user().ok()?;
        if found.uid() != user || found.mode() & 0o077 != 0 {
            return None;
        }
    }
    let mut text = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(4096)
        .read_to_end(&mut text)
        .ok()?;
    serde_json::from_slice(&text).ok()
}

/// The name of the page that sends Chrome on to the address with its token.
pub const OPENER_FILE: &str = "play-open.html";

/// Writes, for its owner alone, the page in the store's folder that sends a browser on
/// to `address`, the address with its token: what Chrome is given in place of the
/// address, so that the token is never on a command line, which other users of the
/// machine can read. The path of the page.
pub fn write_opener(root: &Path, address: &str) -> io::Result<PathBuf> {
    let path = root.join(OPENER_FILE);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    let page = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\">\n\
         <meta http-equiv=\"refresh\" content=\"0;url={address}\">\n\
         <title>whizbang playground</title></head>\n\
         <body><p><a href=\"{address}\">Open the playground</a></p></body></html>\n"
    );
    file.write_all(page.as_bytes())?;
    file.sync_all()?;
    Ok(path)
}

/// Removes the opening page of the store at `root`; one that is not there is nothing
/// to remove.
pub fn remove_opener(root: &Path) {
    let _ = std::fs::remove_file(root.join(OPENER_FILE));
}

/// The command that opens the page `opener` in Google Chrome: `open -a "Google Chrome"`
/// on the Mac, `google-chrome` elsewhere, in a process group of its own so that a
/// signal meant for whizbang does not reach the browser. Its arguments name the page's
/// path and nothing else of it.
pub fn chrome_command(opener: &Path, mac: bool) -> std::process::Command {
    use std::process::{Command, Stdio};
    let mut command = if mac {
        let mut command = Command::new("open");
        command.args(["-a", "Google Chrome"]).arg(opener);
        command
    } else {
        let mut command = Command::new("google-chrome");
        command.arg(opener);
        command
    };
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Removes the file of the host of the store at `root`; one that is not there is
/// nothing to remove.
pub fn remove_hosting(root: &Path) {
    let _ = std::fs::remove_file(hosting_file(root));
}

/// Whether the host this file names answers, at its port on loopback, to its token:
/// the one `/api/hello` it asks, within a few seconds. A host that answers so is the
/// host of the store whose file named it, since only that host has the token.
pub fn answers(hosting: &Hosting) -> bool {
    let Some(token) = Token::parse(&hosting.token) else {
        return false;
    };
    let address = SocketAddr::from(([127, 0, 0, 1], hosting.port));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(2)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let request = format!(
        "GET /api/hello HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n\r\n",
        hosting.port,
        token.hex()
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut answer = Vec::new();
    let _ = stream.take(64 * 1024).read_to_end(&mut answer);
    answer.starts_with(b"HTTP/1.1 200 ")
}

/// The address a person opens: the host's page, with the token after `#k=` when it is
/// to be handed over, which the page takes out of the address bar at once.
pub fn address(port: u16, token: Option<&Token>) -> String {
    match token {
        Some(token) => format!("http://127.0.0.1:{port}/#k={}", token.hex()),
        None => format!("http://127.0.0.1:{port}/"),
    }
}
