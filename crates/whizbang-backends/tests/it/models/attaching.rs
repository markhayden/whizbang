//! Attaching to an install, and cancelling one (ADR 0017, "Attaching and
//! cancelling"). A request for a model that is being installed waits for that install
//! and ends as it ended; only the request that started an install cancels it. The
//! installer and the one that attaches are two requests of one process, or two
//! processes: the second process is the test's own program run once more, for one
//! test that is otherwise skipped, and the two talk a line at a time through its
//! input and its output.

use std::io::{BufRead, BufReader, Lines, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use whizbang_backends::models::{
    Current, InstallRefused, InstallationRecord, ModelsDir, State, Status, install,
    install_or_attach,
};
use whizbang_core::capability::Fault;
use whizbang_core::contract::Cancellation;
use whizbang_core::egress::{DownloadRequest, Downloaded, Downloader};
use whizbang_core::store::fs::{Fs, RealFs};
use whizbang_testkit::barrier::Barriers;
use whizbang_testkit::downloads::FakeDownloader;
use whizbang_testkit::fs_shim::{Event, ShimFs};
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::threads;

use super::none;
use super::publishing::{
    adapter, events_after, first, index_of, installations, installed, installing, is_whole,
    reading, record_of, said, second,
};
use super::staging::{TERMS, address, at, graph, incoming, serving_both};

/// A fake that serves the licence, and the graph up to 1,024 of its 2,500 bytes, where
/// it stops at the barrier `mid`.
fn stopping_mid_file(barriers: &Barriers) -> Arc<FakeDownloader> {
    Arc::new(
        FakeDownloader::new()
            .serving(&at("LICENSE.txt"), TERMS.to_vec())
            .stopping_after(&at("graph.onnx"), graph(), 1024, barriers.clone(), "mid"),
    )
}

/// What a status says, as the tests compare it.
fn of(status: &Status) -> (State, u64, u64) {
    (status.state(), status.files_done(), status.files())
}

/// A request for the second entry over a real disk: it installs, or attaches.
fn asking(
    root: &Path,
    downloader: &dyn Downloader,
    cancellation: &Cancellation,
    watch: &mut dyn FnMut(Option<Status>),
) -> Result<InstallationRecord, InstallRefused> {
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    install_or_attach(
        &dir,
        &second(),
        &adapter(),
        downloader,
        cancellation,
        &none(),
        watch,
    )
}

/// An install of the second entry on a thread, stopped inside its second file: the
/// status says one file of two.
fn stopped_mid_file(
    root: &Path,
    barriers: &Barriers,
    cancellation: &Cancellation,
) -> (
    Arc<FakeDownloader>,
    std::thread::JoinHandle<Result<InstallationRecord, InstallRefused>>,
) {
    let fake = stopping_mid_file(barriers);
    let installer = {
        let (fake, root, cancellation) = (fake.clone(), root.to_path_buf(), cancellation.clone());
        threads::spawn(move || asking(&root, &*fake, &cancellation, &mut |_| {}))
    };
    barriers.arrived("mid", 1);
    assert_eq!(said(root), Some((State::Installing, 1, 2)));
    (fake, installer)
}

/// Each file was asked for once of this fake, and handed over whole.
fn served_each_file_once(fake: &FakeDownloader) {
    assert_eq!(fake.requests().len(), 2);
    assert_eq!(fake.asked(&at("LICENSE.txt")), 1);
    assert_eq!(fake.asked(&at("graph.onnx")), 1);
    assert_eq!(fake.handed(&at("graph.onnx")), 2500);
}

// --- a second request attaches -----------------------------------------------------------------

#[test]
fn two_requests_in_one_process_cause_one_download_of_each_file_and_end_with_one_result() {
    let scratch = Scratch::new("attach-one-process");
    let root = scratch.path();
    installed(root, &first());
    let barriers = Barriers::new();
    let (fake, installer) = stopped_mid_file(root, &barriers, &Cancellation::new());
    // The second request, while the first is stopped inside a file: it downloads
    // nothing, is shown how far the install is, and waits. The first is let go once
    // the second has been shown that.
    let other = serving_both();
    let mut seen = Vec::new();
    let attached = asking(root, &other, &Cancellation::new(), &mut |status| {
        if seen.is_empty() {
            barriers.release("mid");
        }
        seen.push(status.as_ref().map(of));
        std::thread::yield_now();
    });
    let published = installer.join().unwrap();
    assert_eq!(seen[0], Some((State::Installing, 1, 2)));
    // What is shown between two tries is an install that is running, or nothing.
    assert!(
        seen.iter()
            .flatten()
            .all(|said| said.0 == State::Installing)
    );
    assert_eq!(published, Ok(record_of(&second())));
    assert_eq!(attached, published);
    served_each_file_once(&fake);
    assert!(other.requests().is_empty());
    let record = published.unwrap();
    assert_eq!(reading(root), Current::Installed(record.clone()));
    is_whole(root, &record);
    assert_eq!(installations(root), [record.identity().hex()]);
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
}

#[test]
fn a_request_that_attaches_ends_as_the_install_ended_failed_or_cancelled() {
    // The install the second request waits for is refused a file, or is cancelled:
    // the second is told so, and downloads nothing itself.
    for cancelled in [false, true] {
        let scratch = Scratch::new("attach-ended");
        let root = scratch.path();
        let before = installed(root, &first());
        let barriers = Barriers::new();
        let cancellation = Cancellation::new();
        // The graph it serves is a byte short when it is not cancelled.
        let fake = Arc::new(
            FakeDownloader::new()
                .serving(&at("LICENSE.txt"), TERMS.to_vec())
                .stopping_after(
                    &at("graph.onnx"),
                    graph()[..2499].to_vec(),
                    1024,
                    barriers.clone(),
                    "mid",
                ),
        );
        let installer = {
            let (fake, root, cancellation) =
                (fake.clone(), root.to_path_buf(), cancellation.clone());
            threads::spawn(move || asking(&root, &*fake, &cancellation, &mut |_| {}))
        };
        barriers.arrived("mid", 1);
        let other = serving_both();
        let mut shown = false;
        let attached = asking(root, &other, &Cancellation::new(), &mut |_| {
            if !shown {
                shown = true;
                if cancelled {
                    cancellation.cancel();
                }
                barriers.release("mid");
            }
            std::thread::yield_now();
        });
        let ended = installer.join().unwrap().unwrap_err();
        if cancelled {
            assert_eq!(ended, InstallRefused::Cancelled);
            assert_eq!(attached, Err(InstallRefused::Cancelled));
            assert_eq!(said(root), Some((State::Cancelled, 1, 2)));
        } else {
            assert!(matches!(ended, InstallRefused::File { .. }), "{ended:?}");
            assert_eq!(attached, Err(InstallRefused::Failed));
            assert_eq!(said(root), Some((State::Failed, 1, 2)));
        }
        assert!(other.requests().is_empty());
        assert_eq!(reading(root), Current::Installed(before.clone()));
        assert!(!incoming(root).exists());
    }
}

#[test]
fn a_request_that_attached_to_an_install_of_another_identity_is_busy() {
    // The install that is running is of another revision of the model. It ends
    // installed, and what it installed is not what the second request asked for:
    // the second is told that the model was busy, and asks again when it will.
    let scratch = Scratch::new("attach-another");
    let root = scratch.path();
    let barriers = Barriers::new();
    let (_, installer) = stopped_mid_file(root, &barriers, &Cancellation::new());
    let other = serving_both();
    let mut shown = false;
    let dir = ModelsDir::at(Arc::new(RealFs), root);
    let attached = install_or_attach(
        &dir,
        &first(),
        &adapter(),
        &other,
        &Cancellation::new(),
        &none(),
        &mut |_| {
            if !shown {
                shown = true;
                barriers.release("mid");
            }
            std::thread::yield_now();
        },
    );
    let published = installer.join().unwrap().unwrap();
    assert_eq!(attached, Err(InstallRefused::Busy));
    assert!(other.requests().is_empty());
    assert_eq!(reading(root), Current::Installed(published));
    // Asked again, with nothing running, it installs.
    let again = install_or_attach(
        &dir,
        &first(),
        &adapter(),
        &other,
        &Cancellation::new(),
        &none(),
        &mut |_| {},
    );
    assert_eq!(again, Ok(record_of(&first())));
    assert_eq!(reading(root), Current::Installed(record_of(&first())));
}

#[test]
fn while_the_lock_is_held_and_the_status_says_no_install_is_running_nothing_is_shown() {
    // The model's lock is held by one who installs nothing, as a removal will hold
    // it (task M5b), over a status that says how the last install ended. What is
    // shown between two tries is nothing; and once the lock is let go, a status
    // that says installed with a pointer that names this installation is the answer.
    let scratch = Scratch::new("attach-no-install");
    let root = scratch.path();
    let record = installed(root, &second());
    let mut held = RealFs
        .hold_lock_file(&root.join("models/tiny/LOCK"), false)
        .unwrap();
    assert!(held.is_some());
    let fake = serving_both();
    let mut seen = Vec::new();
    let attached = asking(root, &fake, &Cancellation::new(), &mut |status| {
        seen.push(status.as_ref().map(of));
        held = None;
    });
    assert_eq!(seen, [None]);
    assert_eq!(attached, Ok(record));
    assert!(fake.requests().is_empty());
}

#[test]
fn an_attached_request_answers_from_the_status_and_never_from_a_pointer_alone() {
    // The install the second request waits for is refused the sync that makes its
    // pointer durable: it is refused, its status says failed, and the pointer names
    // the new installation all the same (ADR 0017, "Limits"). The second request
    // ends as that install ended, failed; asked again, it is answered installed,
    // once a sync has passed.
    let clean = events_after(&[&first()], &second());
    let pointed = index_of(
        &clean,
        |event| matches!(event, Event::Rename(_, to) if to.ends_with("CURRENT")),
    );
    let scratch = Scratch::new("attach-status");
    let root = scratch.path().to_path_buf();
    installed(&root, &first());
    let barriers = Barriers::new();
    let fs = Arc::new(ShimFs::new());
    fs.refuse_before(pointed + 1);
    let stop = barriers.clone();
    fs.pause_before(pointed, move || stop.stop_at("pointing"));
    let installer = {
        let (fs, root) = (fs.clone(), root.clone());
        threads::spawn(move || installing(&fs, &root, &second(), &serving_both()))
    };
    barriers.arrived("pointing", 1);
    let other = serving_both();
    let mut shown = false;
    let attached = asking(&root, &other, &Cancellation::new(), &mut |_| {
        if !shown {
            shown = true;
            barriers.release("pointing");
        }
        std::thread::yield_now();
    });
    assert!(matches!(
        installer.join().unwrap(),
        Err(InstallRefused::Disk(_))
    ));
    assert_eq!(reading(&root), Current::Installed(record_of(&second())));
    assert_eq!(said(&root), Some((State::Failed, 2, 2)));
    assert_eq!(attached, Err(InstallRefused::Failed));
    assert!(other.requests().is_empty());
    // Asked again: nothing is downloaded, the pointer is made durable, and the
    // answer is the installation.
    let again = asking(&root, &other, &Cancellation::new(), &mut |_| {});
    assert_eq!(again, Ok(record_of(&second())));
    assert!(other.requests().is_empty());
    assert_eq!(said(&root), Some((State::Installed, 2, 2)));
}

#[test]
fn an_install_says_installing_before_anything_else_whether_it_downloads_or_finds_its_identity_whole()
 {
    // An identity that is on the disk whole is read, every file of it, and nothing
    // is downloaded: one who attaches meanwhile is shown an install that is running,
    // not how the one before it ended.
    for adopted in [true, false] {
        let entry = if adopted { second() } else { first() };
        let before = |root: &Path| {
            installed(root, &second());
            assert_eq!(said(root), Some((State::Installed, 2, 2)));
        };
        let clean = {
            let scratch = Scratch::new("attach-says-events");
            before(scratch.path());
            let fs = Arc::new(ShimFs::new());
            let fake = serving_both();
            installing(&fs, scratch.path(), &entry, &fake).unwrap();
            assert_eq!(fake.requests().is_empty(), adopted);
            fs.events()
        };
        // The first thing written or removed under the model's lock is the status.
        let named = |event: &Event, name: &str| match event {
            Event::Write(path) | Event::Remove(path) => path.ends_with(name),
            _ => false,
        };
        let changed = index_of(&clean, |event| {
            matches!(event, Event::Write(_) | Event::Remove(_))
        });
        assert!(named(&clean[changed], "STATUS.new"), "adopted {adopted}");
        let said_first = index_of(
            &clean,
            |event| matches!(event, Event::Rename(_, to) if to.ends_with("STATUS")),
        );
        // What the status says straight after it was first written, when the first
        // file of the installation is opened to be read, and when the directory's
        // lock is taken to point.
        let opened = clean
            .iter()
            .position(|event| matches!(event, Event::Open(p) if p.ends_with("files/LICENSE.txt")));
        let pointing = index_of(
            &clean,
            |event| matches!(event, Event::Lock(p) if !p.ends_with("tiny/LOCK")),
        );
        let moments = [
            (Some(said_first + 1), 0),
            (opened, if adopted { 0 } else { 2 }),
            (Some(pointing), 2),
        ];
        for (moment, done) in moments {
            let moment = moment.unwrap();
            let scratch = Scratch::new("attach-says");
            let root = scratch.path().to_path_buf();
            before(&root);
            let fs = Arc::new(ShimFs::new());
            let seen = Arc::new(Mutex::new(None));
            let (see, at_root) = (seen.clone(), root.clone());
            fs.pause_before(moment, move || {
                *see.lock().unwrap() = said(&at_root);
            });
            installing(&fs, &root, &entry, &serving_both()).unwrap();
            assert_eq!(
                *seen.lock().unwrap(),
                Some((State::Installing, done, 2)),
                "adopted {adopted}, event {moment}"
            );
            assert_eq!(said(&root), Some((State::Installed, 2, 2)));
        }
    }
}

// --- a second process --------------------------------------------------------------------------

/// The variable that names the models directory to a test run in a process of its
/// own.
const ROOT_FOR_CHILD: &str = "WHIZBANG_TEST_MODELS_ROOT";

fn root_from_parent() -> PathBuf {
    PathBuf::from(std::env::var_os(ROOT_FOR_CHILD).unwrap())
}

/// Says one line to the process that started this one.
fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "\n{line}").unwrap();
    out.flush().unwrap();
}

/// The test's own program, run once more for one test of this file that is
/// otherwise skipped: a second process over the same models directory.
struct Other {
    child: Child,
    lines: Lines<BufReader<ChildStdout>>,
}

impl Other {
    fn running(test: &str, root: &Path) -> Other {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("models::attaching::{test}"),
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(ROOT_FOR_CHILD, root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the test binary runs");
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        Other { child, lines }
    }

    /// Waits for the line the other process says that starts with this word, and
    /// gives what follows the word.
    fn says(&mut self, word: &str) -> String {
        let prefix = format!("{word} ");
        let line = self
            .lines
            .by_ref()
            .map_while(Result::ok)
            .find(|line| line.starts_with(&prefix));
        let line = line.unwrap_or_else(|| panic!("the other process ended before `{word}`"));
        line[prefix.len()..].to_owned()
    }

    /// Kills the other process, and waits until it is gone: its locks are let go.
    fn killed(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    fn ended_well(mut self) {
        assert!(self.child.wait().unwrap().success());
    }
}

/// Run only in a process of its own: asks for the second entry in the directory it
/// is told of, says what the status shows the first time it shows an install, and
/// says how it ended and how many requests its downloader was handed.
#[test]
#[ignore = "run in a process of its own by the tests of a second process that attaches"]
fn child_asks_for_the_model_and_says_what_it_sees() {
    let root = root_from_parent();
    let fake = serving_both();
    let mut told = false;
    let ended = asking(&root, &fake, &Cancellation::new(), &mut |status| {
        if let (Some(status), false) = (status, told) {
            told = true;
            let (state, done, files) = of(&status);
            say(&format!("sees {state:?} {done} {files}"));
        }
        std::thread::yield_now();
    });
    match ended {
        Ok(record) => say(&format!("ended {}", record.identity())),
        Err(refused) => say(&format!("ended {refused:?}")),
    }
    say(&format!("asked {}", fake.requests().len()));
}

/// Run only in a process of its own: installs the second entry in the directory it
/// is told of, and stops inside its second file, every byte of it appended and none
/// synced, until a line arrives on its input.
#[test]
#[ignore = "run in a process of its own by the test of an installing process that is killed"]
fn child_installs_and_stops_inside_its_second_file() {
    let root = root_from_parent();
    let clean = events_after(&[&first()], &second());
    let inside = index_of(
        &clean,
        |event| matches!(event, Event::SyncFile(p) if p.ends_with("graph.onnx")),
    );
    let fs = Arc::new(ShimFs::new());
    fs.pause_before(inside, || {
        say("stopped inside");
        let mut go = String::new();
        std::io::stdin().lock().read_line(&mut go).unwrap();
    });
    let ended = installing(&fs, &root, &second(), &serving_both());
    say(&format!("ended {}", ended.is_ok()));
}

#[test]
fn a_second_process_that_asks_while_the_first_is_stopped_sees_progress_and_ends_with_the_same_installation()
 {
    let scratch = Scratch::new("attach-process");
    let root = scratch.path();
    installed(root, &first());
    let barriers = Barriers::new();
    let (fake, installer) = stopped_mid_file(root, &barriers, &Cancellation::new());
    let mut other = Other::running("child_asks_for_the_model_and_says_what_it_sees", root);
    assert_eq!(other.says("sees"), "Installing 1 2");
    barriers.release("mid");
    let published = installer.join().unwrap().unwrap();
    assert_eq!(other.says("ended"), published.identity().to_string());
    // The other process's downloader was asked for nothing, and this one's for each
    // file once.
    assert_eq!(other.says("asked"), "0");
    other.ended_well();
    served_each_file_once(&fake);
    assert_eq!(reading(root), Current::Installed(published.clone()));
    is_whole(root, &published);
    assert_eq!(installations(root), [published.identity().hex()]);
}

#[test]
fn when_the_installing_process_is_killed_the_attached_one_reports_the_install_as_failed() {
    let scratch = Scratch::new("attach-killed");
    let root = scratch.path();
    let before = installed(root, &first());
    let mut other = Other::running("child_installs_and_stops_inside_its_second_file", root);
    assert_eq!(other.says("stopped"), "inside");
    // This process attaches, is shown the install, and the other is killed: the
    // model's lock is free, and the status still says that an install is running.
    let fake = serving_both();
    let mut seen = Vec::new();
    let attached = asking(root, &fake, &Cancellation::new(), &mut |status| {
        if seen.is_empty() {
            other.killed();
        }
        seen.push(status.as_ref().map(of));
        std::thread::yield_now();
    });
    assert_eq!(seen[0], Some((State::Installing, 1, 2)));
    assert_eq!(attached, Err(InstallRefused::Failed));
    assert!(fake.requests().is_empty());
    assert_eq!(said(root), Some((State::Installing, 1, 2)));
    assert_eq!(reading(root), Current::Installed(before.clone()));
    is_whole(root, &before);
    // What the dead process staged is still there, and is removed by the next to
    // take the model's lock: asked again, the model is installed.
    assert!(incoming(root).exists());
    let again = asking(root, &fake, &Cancellation::new(), &mut |_| {});
    assert_eq!(again, Ok(record_of(&second())));
    served_each_file_once(&fake);
    assert!(!incoming(root).exists());
    assert_eq!(installations(root), [record_of(&second()).identity().hex()]);
}

// --- a cancel ----------------------------------------------------------------------------------

#[test]
fn a_cancel_while_the_download_is_stopped_inside_a_file_ends_the_install_as_cancelled() {
    let scratch = Scratch::new("cancel-mid-file");
    let root = scratch.path();
    let before = installed(root, &first());
    let barriers = Barriers::new();
    let cancellation = Cancellation::new();
    let (fake, installer) = stopped_mid_file(root, &barriers, &cancellation);
    assert!(incoming(root).join("files/graph.onnx").exists());
    cancellation.cancel();
    barriers.release("mid");
    assert_eq!(installer.join().unwrap(), Err(InstallRefused::Cancelled));
    // Its staging is removed, the previous installation is named and whole, and
    // nothing was asked or handed over after the cancel.
    assert!(!incoming(root).exists());
    assert_eq!(reading(root), Current::Installed(before.clone()));
    is_whole(root, &before);
    assert_eq!(installations(root), [before.identity().hex()]);
    assert_eq!(said(root), Some((State::Cancelled, 1, 2)));
    assert_eq!(fake.requests().len(), 2);
    assert_eq!(fake.handed(&at("graph.onnx")), 1024);
}

/// A downloader that looks at no cancellation and at no answer of its sink: it hands
/// each file over in two pieces, come what may, and runs what it was given once,
/// after the first piece of the graph.
struct Deaf {
    asked: Mutex<Vec<String>>,
    between: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Deaf {
    fn doing(between: impl FnOnce() + Send + 'static) -> Deaf {
        Deaf {
            asked: Mutex::new(Vec::new()),
            between: Mutex::new(Some(Box::new(between))),
        }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Downloader for Deaf {
    fn download(
        &self,
        request: &DownloadRequest,
        _: &Cancellation,
        sink: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        let is_graph = *request.from() == at("graph.onnx");
        let bytes = if is_graph { graph() } else { TERMS.to_vec() };
        let name = if is_graph {
            "graph.onnx"
        } else {
            "LICENSE.txt"
        };
        self.asked.lock().unwrap().push(name.to_owned());
        let (one, two) = bytes.split_at(bytes.len() / 2);
        let _ = sink(one);
        if is_graph && let Some(between) = self.between.lock().unwrap().take() {
            between();
        }
        let _ = sink(two);
        Ok(Downloaded::Bytes(bytes.len() as u64))
    }
}

#[test]
fn a_cancel_is_asked_between_files_and_inside_one_whatever_the_downloader_does() {
    // Inside a file, of a downloader that does not ask: the piece after the cancel
    // is not kept, and the install ends as cancelled.
    let scratch = Scratch::new("cancel-inside");
    let root = scratch.path();
    let before = installed(root, &first());
    let cancellation = Cancellation::new();
    let cancel = cancellation.clone();
    let deaf = Deaf::doing(move || cancel.cancel());
    let fs = Arc::new(ShimFs::new());
    let dir = ModelsDir::at(fs.clone(), root);
    let ended = install(&dir, &second(), &adapter(), &deaf, &cancellation, &none());
    assert_eq!(ended, Err(InstallRefused::Cancelled));
    let appended = fs
        .events()
        .iter()
        .filter(|event| matches!(event, Event::Append(p) if p.ends_with("graph.onnx")))
        .count();
    assert_eq!(appended, 1);
    assert_eq!(said(root), Some((State::Cancelled, 1, 2)));
    assert!(!incoming(root).exists());
    assert_eq!(reading(root), Current::Installed(before.clone()));

    // Between files: cancelled when the first file is staged, the second is never
    // asked for.
    let clean = {
        let scratch = Scratch::new("cancel-between-events");
        installed(scratch.path(), &first());
        let fs = Arc::new(ShimFs::new());
        let dir = ModelsDir::at(fs.clone(), scratch.path());
        let deaf = Deaf::doing(|| {});
        install(
            &dir,
            &second(),
            &adapter(),
            &deaf,
            &Cancellation::new(),
            &none(),
        )
        .unwrap();
        assert_eq!(deaf.asked(), ["LICENSE.txt", "graph.onnx"]);
        fs.events()
    };
    let between = index_of(
        &clean,
        |event| matches!(event, Event::Begin(p) if p.ends_with("graph.onnx")),
    );
    // The status is written between the two files: the cancel comes with its first
    // step, after the first file is finished.
    let staged = clean[..between]
        .iter()
        .rposition(|event| matches!(event, Event::Finish(_)))
        .unwrap();
    let cancellation = Cancellation::new();
    let cancel = cancellation.clone();
    let deaf = Deaf::doing(|| {});
    let fs = Arc::new(ShimFs::new());
    fs.pause_before(staged + 1, move || cancel.cancel());
    let dir = ModelsDir::at(fs.clone(), root);
    let ended = install(&dir, &second(), &adapter(), &deaf, &cancellation, &none());
    assert_eq!(ended, Err(InstallRefused::Cancelled));
    assert_eq!(deaf.asked(), ["LICENSE.txt"]);
    assert_eq!(said(root), Some((State::Cancelled, 1, 2)));
    assert!(!incoming(root).exists());
    assert_eq!(reading(root), Current::Installed(before));
}

/// A downloader that answers every request by saying the file moved, within its origin,
/// and runs what it was given once, before its first answer.
struct Moving {
    asked: Mutex<u64>,
    before: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl Downloader for Moving {
    fn download(
        &self,
        _: &DownloadRequest,
        _: &Cancellation,
        _: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<Downloaded, Fault> {
        *self.asked.lock().unwrap() += 1;
        if let Some(before) = self.before.lock().unwrap().take() {
            before();
        }
        Ok(Downloaded::Moved(address(
            "https://weights.example.org/files/elsewhere",
        )))
    }
}

#[test]
fn a_cancel_before_a_move_is_followed_asks_for_nothing_more() {
    let scratch = Scratch::new("cancel-move");
    let root = scratch.path();
    let before = installed(root, &first());
    // Not cancelled, the move is followed, again and again, to the limit of moves.
    let moving = |cancel: Option<Cancellation>| Moving {
        asked: Mutex::new(0),
        before: Mutex::new(cancel.map(|cancel| {
            let cancel: Box<dyn FnOnce() + Send> = Box::new(move || cancel.cancel());
            cancel
        })),
    };
    let dir = ModelsDir::at(Arc::new(ShimFs::new()), root);
    let followed = moving(None);
    let cancellation = Cancellation::new();
    let ended = install(
        &dir,
        &second(),
        &adapter(),
        &followed,
        &cancellation,
        &none(),
    );
    assert!(
        matches!(ended, Err(InstallRefused::File { .. })),
        "{ended:?}"
    );
    assert!(*followed.asked.lock().unwrap() > 1);
    // Cancelled while the downloader answers that the file moved: one request.
    let cancelled = moving(Some(cancellation.clone()));
    let ended = install(
        &dir,
        &second(),
        &adapter(),
        &cancelled,
        &cancellation,
        &none(),
    );
    assert_eq!(ended, Err(InstallRefused::Cancelled));
    assert_eq!(*cancelled.asked.lock().unwrap(), 1);
    assert_eq!(said(root), Some((State::Cancelled, 0, 2)));
    assert!(!incoming(root).exists());
    assert_eq!(reading(root), Current::Installed(before));
}

#[test]
fn a_cancel_after_the_pointer_was_replaced_returns_the_installed_result() {
    let scratch = Scratch::new("cancel-after");
    let root = scratch.path();
    installed(root, &first());
    let cancellation = Cancellation::new();
    let cancel = cancellation.clone();
    let fs = Arc::new(ShimFs::new());
    fs.pause_after_rename_onto("CURRENT", move || cancel.cancel());
    let dir = ModelsDir::at(fs.clone(), root);
    let ended = install_or_attach(
        &dir,
        &second(),
        &adapter(),
        &serving_both(),
        &cancellation,
        &none(),
        &mut |_| {},
    );
    assert!(cancellation.is_cancelled());
    assert_eq!(ended, Ok(record_of(&second())));
    assert_eq!(reading(root), Current::Installed(record_of(&second())));
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
}

// --- an attached request that goes away --------------------------------------------------------

#[test]
fn an_attached_request_that_goes_away_does_not_cancel_the_install() {
    // In one process: the attached request is cancelled while it waits. It is told
    // that it left; the install goes on, and ends installed.
    let scratch = Scratch::new("attach-left");
    let root = scratch.path();
    installed(root, &first());
    let barriers = Barriers::new();
    let (fake, installer) = stopped_mid_file(root, &barriers, &Cancellation::new());
    let other = serving_both();
    let leaving = Cancellation::new();
    let mut rounds = 0;
    let attached = asking(root, &other, &leaving, &mut |_| {
        rounds += 1;
        leaving.cancel();
    });
    assert_eq!(attached, Err(InstallRefused::Left));
    assert_eq!(rounds, 1);
    assert_eq!(said(root), Some((State::Installing, 1, 2)));
    barriers.release("mid");
    let published = installer.join().unwrap();
    assert_eq!(published, Ok(record_of(&second())));
    served_each_file_once(&fake);
    assert!(other.requests().is_empty());
    assert_eq!(said(root), Some((State::Installed, 2, 2)));

    // In two: the process that attached is killed once it was shown the install.
    let scratch = Scratch::new("attach-left-process");
    let root = scratch.path();
    installed(root, &first());
    let barriers = Barriers::new();
    let (fake, installer) = stopped_mid_file(root, &barriers, &Cancellation::new());
    let mut other = Other::running("child_asks_for_the_model_and_says_what_it_sees", root);
    assert_eq!(other.says("sees"), "Installing 1 2");
    other.killed();
    barriers.release("mid");
    let published = installer.join().unwrap().unwrap();
    served_each_file_once(&fake);
    assert_eq!(reading(root), Current::Installed(published.clone()));
    is_whole(root, &published);
    assert_eq!(said(root), Some((State::Installed, 2, 2)));
}

#[test]
fn a_request_that_finds_no_install_running_installs_and_one_already_cancelled_does_nothing() {
    let scratch = Scratch::new("attach-none");
    let root = scratch.path();
    let fake = serving_both();
    let mut rounds = 0;
    let ended = asking(root, &fake, &Cancellation::new(), &mut |_| rounds += 1);
    assert_eq!(ended, Ok(record_of(&second())));
    assert_eq!(rounds, 0);
    served_each_file_once(&fake);
    // Cancelled before it asked, with an install running: it does not wait.
    let barriers = Barriers::new();
    let scratch = Scratch::new("attach-cancelled");
    let root = scratch.path();
    let (_, installer) = stopped_mid_file(root, &barriers, &Cancellation::new());
    let gone = Cancellation::new();
    gone.cancel();
    let other = serving_both();
    let ended = asking(root, &other, &gone, &mut |_| rounds += 1);
    assert_eq!(ended, Err(InstallRefused::Cancelled));
    assert_eq!(rounds, 0);
    assert!(other.requests().is_empty());
    barriers.release("mid");
    assert_eq!(installer.join().unwrap(), Ok(record_of(&second())));
}
