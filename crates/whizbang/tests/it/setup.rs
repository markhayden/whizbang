//! `whizbang setup` (plan 1C, task Y1; ADR 0021): the pack installed by `uv`, locked,
//! from the lock file this build carries, into an environment in the models directory,
//! and a check that the worker it made says hello.
//!
//! Every test here is of the fast tier: none installs a package and every `uv` is
//! offline. The real `uv` and the Python the pack pins are the test kit's (found as
//! `just test-pack` finds them) and are handed to the binary by its flags, since the
//! binary is started with a cleared environment. A stand-in for `uv` is a short script
//! of the test's own: one that notes what it was given and runs the real `uv`, and one
//! that never answers and ends itself after 120 seconds and not before, so that
//! nothing is left if the test is killed and nobody kills it.

use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

use whizbang::pack_setup::{self, Asked, Host, Install, Limits, Machine, Pack};
use whizbang_testkit::scratch::Scratch;
use whizbang_testkit::worker::{processes, python, uv};

/// The repository's `pack` folder.
fn pack_folder() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../pack")).to_path_buf()
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("output is UTF-8")
}

/// Runs `whizbang setup` with the real `uv` (or `with_uv`), the pinned Python and this
/// models directory, and these variables of the test's own.
fn setup(models: &Path, with_uv: Option<&Path>, vars: &[(&str, &str)]) -> Output {
    let mut binary = whizbang_testkit::binary::command(env!("CARGO_BIN_EXE_whizbang"));
    binary
        .arg("setup")
        .arg("--uv")
        .arg(with_uv.unwrap_or(uv()))
        .arg("--python")
        .arg(python())
        .arg("--models-dir")
        .arg(models)
        .envs(vars.iter().copied());
    binary.output().expect("the binary runs")
}

/// A models directory for one test: a folder in a scratch folder, made by `setup`.
fn models_in(scratch: &Scratch) -> PathBuf {
    scratch.path().join("models")
}

/// Writes an executable script.
fn script(path: &Path, body: &str) {
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

/// A stand-in for `uv` that notes, for each time it is started, its arguments and its
/// environment in a file of `notes` of its own, and then runs the real `uv` with them.
fn noting_uv(scratch: &Scratch) -> (PathBuf, PathBuf) {
    let notes = scratch.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let stand_in = scratch.path().join("uv");
    script(
        &stand_in,
        &format!(
            "#!/bin/bash\nnote='{}'/call.$$\nprintf '%s\\n' \"$@\" >\"$note.args\"\n/usr/bin/env >\"$note.vars\"\nexec '{}' \"$@\"\n",
            notes.display(),
            uv().display()
        ),
    );
    (stand_in, notes)
}

/// One start of the noting stand-in: its arguments, and its environment by name.
struct Call {
    args: Vec<String>,
    vars: BTreeMap<String, String>,
}

/// The variables bash sets of itself in a script's children, whatever it was given.
const BASHS_OWN: [&str; 3] = ["PWD", "SHLVL", "_"];

fn calls(notes: &Path) -> Vec<Call> {
    let mut names: Vec<PathBuf> = std::fs::read_dir(notes)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|end| end == "args"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|args| {
            let vars = args.with_extension("vars");
            Call {
                args: std::fs::read_to_string(&args)
                    .unwrap()
                    .lines()
                    .map(str::to_owned)
                    .collect(),
                vars: std::fs::read_to_string(vars)
                    .unwrap()
                    .lines()
                    .filter_map(|line| line.split_once('='))
                    .filter(|(name, _)| !BASHS_OWN.contains(name))
                    .map(|(name, value)| (name.to_owned(), value.to_owned()))
                    .collect(),
            }
        })
        .collect()
}

/// What is under a folder: for each path, its kind, size, mode, time of change and
/// where a link points. Bounded: a test's folder holds a few hundred entries.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut seen = BTreeMap::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(path) = todo.pop() {
        assert!(
            seen.len() < 20_000,
            "more under the folder than a test makes"
        );
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let link = if meta.file_type().is_symlink() {
            std::fs::read_link(&path).unwrap().display().to_string()
        } else {
            String::new()
        };
        seen.insert(
            path.clone(),
            format!(
                "{:?} {} {:o} {}.{} {link}",
                meta.file_type(),
                meta.len(),
                meta.mode(),
                meta.mtime(),
                meta.mtime_nsec()
            ),
        );
        if meta.is_dir() {
            for entry in std::fs::read_dir(&path).unwrap() {
                todo.push(entry.unwrap().path());
            }
        }
    }
    seen
}

/// What `install` is given in a test: the real `uv` and the pinned Python, the pack
/// this build carries, the limits of the binary and no variable of anybody's.
fn install_into(root: &Path) -> Install {
    Install {
        root: root.to_path_buf(),
        uv: uv().to_path_buf(),
        python: Some(python().to_path_buf()),
        extra: None,
        host: Host::this(),
        pack: Pack::carried(),
        limits: Limits::shipped(),
        parent: Vec::new(),
    }
}

/// A stand-in for `uv` that notes each start, as `noting_uv` does, and fails, so that
/// nothing could be downloaded even if a refusal before it were missing.
fn failing_uv(scratch: &Scratch) -> (PathBuf, PathBuf) {
    let notes = scratch.path().join("notes");
    std::fs::create_dir(&notes).unwrap();
    let stand_in = scratch.path().join("uv");
    script(
        &stand_in,
        &format!(
            "#!/bin/bash\nnote='{}'/call.$$\nprintf '%s\\n' \"$@\" >\"$note.args\"\n/usr/bin/env >\"$note.vars\"\nexit 1\n",
            notes.display()
        ),
    );
    (stand_in, notes)
}

#[test]
fn an_extra_on_a_platform_the_lock_does_not_cover_is_refused_before_anything_runs() {
    // The lock covers macOS on Apple silicon alone (ADR 0021, "Amended"). Anywhere else
    // an extra is refused in a sentence that says so, before any `uv` is started, so
    // nothing is downloaded, and before the models directory is made.
    let scratch = Scratch::new("setup-extra-host");
    let models = models_in(&scratch);
    let (stand_in, notes) = failing_uv(&scratch);
    for (os, arch) in [
        ("linux", "x86_64"),
        ("linux", "aarch64"),
        ("macos", "x86_64"),
        ("windows", "x86_64"),
    ] {
        let mut install = install_into(&models);
        install.uv = stand_in.clone();
        install.extra = Some("torch".to_owned());
        install.host = Host { os, arch };
        let refused = pack_setup::install(&install).expect_err("an extra off the lock");
        assert!(
            refused.contains("is locked for macOS on Apple silicon only in this build"),
            "{refused}"
        );
        assert!(refused.contains(&format!("{os} on {arch}")), "{refused}");
        assert!(
            refused.contains("nothing was downloaded and nothing was changed"),
            "{refused}"
        );
        assert!(calls(&notes).is_empty(), "uv was started on {os} {arch}");
        assert!(!models.exists(), "the models directory was made");
    }
    // The one platform an extra is not refused on. The core, which downloads nothing,
    // is refused on none: the other tests here install it with no extra, on both runners.
    assert_eq!(
        pack_setup::EXTRA_HOST,
        Host {
            os: "macos",
            arch: "aarch64"
        }
    );
}

#[test]
fn the_command_refuses_an_extra_off_the_mac_before_it_makes_the_models_directory() {
    // Through `run`, as the binary runs it: the models directory named by the flag is
    // missing, and so is the data folder the platform would give; neither is made, no
    // `uv` is started, and the sentence is the one `install` gives.
    let scratch = Scratch::new("setup-extra-run");
    let named = scratch.path().join("named").join("models");
    let data = scratch.path().join("data");
    let (stand_in, notes) = failing_uv(&scratch);
    let linux = Host {
        os: "linux",
        arch: "x86_64",
    };
    for (models_dir, extra, sentence) in [
        (
            Some(named.as_path()),
            "torch",
            "is locked for macOS on Apple silicon only in this build, and this is linux on x86_64; nothing was downloaded and nothing was changed",
        ),
        (
            None,
            "torch",
            "is locked for macOS on Apple silicon only in this build, and this is linux on x86_64; nothing was downloaded and nothing was changed",
        ),
        (
            Some(named.as_path()),
            "Not-A-Name",
            "an extra's name is lower-case letters, digits and '-', 64 at most, starting with a letter; nothing was changed",
        ),
    ] {
        let platform_dir = data.join("whizbang").join("models");
        let platform: whizbang::wiring::Platform =
            std::sync::Arc::new(move || Some(platform_dir.clone()));
        let refused = pack_setup::run(
            Asked {
                config: None,
                env_file: None,
                models_dir,
                extra: Some(extra),
                uv: Some(&stand_in),
                python: Some(python()),
            },
            Machine {
                platform,
                path: None,
                parent: Vec::new(),
                host: linux,
            },
        )
        .expect_err("refused");
        assert!(refused.contains(sentence), "{refused}");
        assert!(!scratch.path().join("named").exists(), "{refused}");
        assert!(!data.exists(), "{refused}");
        assert!(calls(&notes).is_empty(), "uv was started");
    }
}

/// The marker every package of an extra carries in the pack's file: the one platform
/// `setup` installs an extra on.
const MAC: &str = "sys_platform == 'darwin' and platform_machine == 'arm64'";

#[test]
fn the_extra_is_for_the_mac_alone_and_the_lock_names_no_nvidia_or_cuda_package() {
    // Each package of each extra is for the Mac alone, so on Linux, which the lock still
    // covers for the core (`uv sync` refuses a platform the lock is not made for), the
    // extra resolves to nothing.
    let pyproject = std::fs::read_to_string(pack_folder().join("pyproject.toml")).unwrap();
    let all = extras();
    assert!(!all.is_empty());
    for (extra, listed) in &all {
        assert!(!listed.is_empty(), "{extra}");
        for item in listed {
            assert!(
                pyproject.contains(&format!("\"{item}; {MAC}\"")),
                "{item} of {extra} is not for the Mac alone"
            );
        }
    }
    // The lock holds none of the packages Linux's CUDA build of `torch` pulls in, and
    // no file for Linux.
    let blocks = lock_blocks();
    assert!(blocks.len() > 3);
    for block in blocks {
        let name = field(&block, "name").unwrap();
        for start in ["nvidia-", "cuda-"] {
            assert!(!name.starts_with(start), "{name}");
        }
        assert_ne!(name, "triton");
        for line in &block {
            assert!(!line.contains("linux_x86_64"), "{name}: {line}");
            assert!(!line.contains("manylinux"), "{name}: {line}");
        }
    }
}

#[test]
fn setup_with_no_extra_makes_an_environment_offline_whose_worker_says_hello() {
    let scratch = Scratch::new("setup-hello");
    let models = models_in(&scratch);
    let (stand_in, notes) = noting_uv(&scratch);
    let out = setup(&models, Some(&stand_in), &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let said = text(&out.stdout);
    assert!(said.contains("the worker it made said hello"), "{said}");
    assert!(said.contains("extra: none"), "{said}");
    // The environment is in the models directory, and the worker's Python is in it.
    let place = pack_setup::at(&models);
    assert!(place.environment.starts_with(&models));
    assert!(place.python.is_file(), "{}", place.python.display());
    assert!(place.source.join("whizbang_pack").is_dir());
    // Every `uv` it started was offline: told so on its command line and by its
    // environment, with no Python to be downloaded.
    let calls = calls(&notes);
    assert!(
        calls.len() >= 3,
        "the version, the check of the lock, the sync"
    );
    for call in &calls {
        assert!(
            call.args.iter().any(|arg| arg == "--offline"),
            "{:?}",
            call.args
        );
        assert_eq!(call.vars.get("UV_OFFLINE").map(String::as_str), Some("1"));
        assert_eq!(
            call.vars.get("UV_PYTHON_DOWNLOADS").map(String::as_str),
            Some("never")
        );
    }
    let sync = calls
        .iter()
        .find(|call| call.args.iter().any(|arg| arg == "sync"))
        .expect("a sync");
    for flag in ["--locked", "--no-build"] {
        assert!(sync.args.iter().any(|arg| arg == flag), "{:?}", sync.args);
    }
    assert!(!sync.args.iter().any(|arg| arg.starts_with("--extra")));
}

#[test]
fn setup_refuses_a_lock_file_that_does_not_match_the_pack_and_changes_nothing() {
    let scratch = Scratch::new("setup-mismatch");
    let models = models_in(&scratch);
    let out = setup(&models, None, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let before = snapshot(&models);
    // The pack's own file asks for a version of a package that the lock does not pin.
    let pyproject = std::fs::read_to_string(pack_folder().join("pyproject.toml")).unwrap();
    let changed = pyproject.replace("transformers==", "transformers>=1,!=");
    assert_ne!(
        changed, pyproject,
        "the pack's file names transformers by its pin"
    );
    let mut install = install_into(&models);
    install.pack = Pack::carried().with_file("pyproject.toml", changed);
    let refused = pack_setup::install(&install).expect_err("a lock that does not match");
    assert!(
        refused.contains("the lock file does not match the pack"),
        "{refused}"
    );
    assert!(refused.contains("nothing was changed"), "{refused}");
    assert_eq!(snapshot(&models), before);
    // And into a models directory that holds nothing yet, it makes nothing.
    let empty = scratch.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    std::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0o700)).unwrap();
    let before = snapshot(&empty);
    install.root = empty.clone();
    pack_setup::install(&install).expect_err("a lock that does not match");
    assert_eq!(snapshot(&empty), before);
}

#[test]
fn a_second_setup_changes_nothing() {
    let scratch = Scratch::new("setup-twice");
    let models = models_in(&scratch);
    let first = setup(&models, None, &[]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let before = snapshot(&models);
    assert!(before.len() > 10, "the environment is there");
    let second = setup(&models, None, &[]);
    assert_eq!(second.status.code(), Some(0), "{}", text(&second.stderr));
    assert_eq!(snapshot(&models), before);
}

#[test]
fn setup_again_writes_the_packs_new_adapter_and_keeps_the_runtime() {
    // The upgrade of task Q1 of checkpoint 1D: a models directory whose pack holds the
    // adapter of revision 1 and a runtime installed by an extra; `setup` again, with no
    // extra, writes this build's adapter and leaves the runtime as it was.
    let scratch = Scratch::new("setup-upgrade");
    let models = models_in(&scratch);
    let first = setup(&models, None, &[]);
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    let place = pack_setup::at(&models);
    let adapter = place.source.join("whizbang_pack").join("sam3.py");
    let carried = std::fs::read_to_string(&adapter).unwrap();
    assert!(carried.contains("REVISION = \"2\""));
    std::fs::write(
        &adapter,
        carried.replace("REVISION = \"2\"", "REVISION = \"1\""),
    )
    .unwrap();
    // A runtime an extra installed: a package the core's lock does not list, as the
    // environment's installer records one.
    let packages = std::fs::read_dir(place.environment.join("lib"))
        .unwrap()
        .map(|found| found.unwrap().path().join("site-packages"))
        .find(|path| path.is_dir())
        .expect("the environment's packages");
    let module = packages.join("runtime_of_an_extra");
    std::fs::create_dir(&module).unwrap();
    std::fs::write(module.join("__init__.py"), "").unwrap();
    let info = packages.join("runtime_of_an_extra-1.0.dist-info");
    std::fs::create_dir(&info).unwrap();
    std::fs::write(
        info.join("METADATA"),
        "Metadata-Version: 2.1\nName: runtime-of-an-extra\nVersion: 1.0\n",
    )
    .unwrap();
    std::fs::write(info.join("INSTALLER"), "uv\n").unwrap();
    std::fs::write(
        info.join("RECORD"),
        "runtime_of_an_extra/__init__.py,,\nruntime_of_an_extra-1.0.dist-info/METADATA,,\n\
         runtime_of_an_extra-1.0.dist-info/INSTALLER,,\nruntime_of_an_extra-1.0.dist-info/RECORD,,\n",
    )
    .unwrap();
    let environment = |snap: BTreeMap<PathBuf, String>| -> BTreeMap<PathBuf, String> {
        snap.into_iter()
            .filter(|(path, _)| path.starts_with(&place.environment))
            .collect()
    };
    let before = environment(snapshot(&models));
    let (stand_in, notes) = noting_uv(&scratch);
    let again = setup(&models, Some(&stand_in), &[]);
    assert_eq!(again.status.code(), Some(0), "{}", text(&again.stderr));
    // The pack's copy is this build's: the adapter that takes points and boxes.
    assert_eq!(std::fs::read_to_string(&adapter).unwrap(), carried);
    // The runtime is kept: nothing of the environment changed, and the sync was told
    // to keep what it does not list.
    assert!(module.join("__init__.py").is_file());
    assert_eq!(environment(snapshot(&models)), before);
    let sync = calls(&notes)
        .into_iter()
        .find(|call| call.args.iter().any(|arg| arg == "sync"))
        .expect("a sync");
    assert!(
        sync.args.iter().any(|arg| arg == "--inexact"),
        "{:?}",
        sync.args
    );
}

/// The lock file, as blocks of `[[package]]`: each block's lines.
fn lock_blocks() -> Vec<Vec<String>> {
    let lock = std::fs::read_to_string(pack_folder().join("uv.lock")).unwrap();
    let mut blocks: Vec<Vec<String>> = Vec::new();
    for line in lock.lines() {
        if line == "[[package]]" {
            blocks.push(Vec::new());
        } else if let Some(block) = blocks.last_mut() {
            block.push(line.to_owned());
        }
    }
    blocks
}

/// The value of `key = "value"` among a block's lines.
fn field<'a>(block: &'a [String], key: &str) -> Option<&'a str> {
    let start = format!("{key} = \"");
    block
        .iter()
        .find_map(|line| line.strip_prefix(&start)?.strip_suffix('"'))
}

/// The value of `"key": "…"` inside a line of the lock, as `url = "…"` or `hash = "…"`.
fn quoted<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = format!("{key} = \"");
    let at = line.find(&start)? + start.len();
    let rest = &line[at..];
    Some(&rest[..rest.find('"')?])
}

/// The pack's extras, from its own file: each name, and what it lists, each without the
/// marker that says which platform it is for.
fn extras() -> Vec<(String, Vec<String>)> {
    let pyproject = std::fs::read_to_string(pack_folder().join("pyproject.toml")).unwrap();
    let mut inside = false;
    let mut found = Vec::new();
    for line in pyproject.lines() {
        if line.starts_with('[') {
            inside = line == "[project.optional-dependencies]";
            continue;
        }
        if !inside || line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let (name, list) = line.split_once(" = ").expect("one extra a line");
        let listed = list
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|item| {
                let item = item.trim().trim_matches('"');
                let item = item.split_once(';').map_or(item, |(wanted, _)| wanted);
                item.trim().to_owned()
            })
            .filter(|item| !item.is_empty())
            .collect();
        found.push((name.to_owned(), listed));
    }
    found
}

#[test]
fn the_lock_file_pins_every_package_of_every_extra_by_version_and_hash() {
    let extras = extras();
    assert!(!extras.is_empty(), "the pack has the runtime's extra");
    let blocks = lock_blocks();
    let names: Vec<&str> = blocks
        .iter()
        .map(|block| field(block, "name").expect("a name"))
        .collect();
    // The pack's own file pins each package an extra names to one version, and the
    // lock holds it at that version.
    for (extra, listed) in &extras {
        assert!(!listed.is_empty(), "{extra}");
        for item in listed {
            let (name, version) = item.split_once("==").unwrap_or_else(|| {
                panic!("{item} of the extra {extra} is not pinned to one version")
            });
            let at = names
                .iter()
                .position(|locked| *locked == name)
                .unwrap_or_else(|| panic!("{name} is not in the lock"));
            assert_eq!(field(&blocks[at], "version"), Some(version), "{name}");
        }
    }
    // The lock was made from this file: the extras it records are the file's.
    let project = blocks
        .iter()
        .find(|block| field(block, "name") == Some("whizbang-pack"))
        .expect("the pack itself");
    let provided = project
        .iter()
        .find_map(|line| line.strip_prefix("provides-extras = "))
        .expect("the extras the lock was made with");
    let wanted: Vec<String> = extras
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect();
    assert_eq!(provided, format!("[{}]", wanted.join(", ")));
    // Every package but the pack itself: a version, and each of its files with its
    // SHA-256; and every package any package depends on is in the lock.
    for (block, name) in blocks.iter().zip(&names) {
        if *name == "whizbang-pack" {
            assert!(
                block
                    .iter()
                    .any(|line| line == "source = { virtual = \".\" }")
            );
            continue;
        }
        let version = field(block, "version").unwrap_or_else(|| panic!("{name}: no version"));
        assert!(!version.is_empty() && !version.contains('*'), "{name}");
        let files: Vec<&String> = block
            .iter()
            .filter(|line| line.contains("url = \""))
            .collect();
        assert!(!files.is_empty(), "{name}: no file");
        for file in files {
            let hash = quoted(file, "hash").unwrap_or_else(|| panic!("{name}: {file}"));
            let hex = hash.strip_prefix("sha256:").expect("a SHA-256");
            assert_eq!(hex.len(), 64, "{name}");
            assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()), "{name}");
        }
        for line in block {
            if let Some(dep) = line.trim().strip_prefix("{ name = \"") {
                let dep = &dep[..dep.find('"').unwrap()];
                assert!(
                    names.contains(&dep),
                    "{name} needs {dep}, which is not locked"
                );
            }
        }
    }
}

/// The only index the owner approved (plan 1C, "Origins"), and where it serves files.
const INDEX: &str = "https://pypi.org/simple";
const FILES: &str = "https://files.pythonhosted.org/";

#[test]
fn the_lock_file_names_no_index_but_the_approved_one() {
    let blocks = lock_blocks();
    assert!(blocks.len() > 3);
    for block in &blocks {
        let name = field(block, "name").unwrap();
        let source = block
            .iter()
            .find_map(|line| line.strip_prefix("source = "))
            .unwrap_or_else(|| panic!("{name}: no source"));
        if name == "whizbang-pack" {
            assert_eq!(source, "{ virtual = \".\" }");
        } else {
            assert_eq!(source, format!("{{ registry = \"{INDEX}\" }}"), "{name}");
        }
        for line in block {
            for key in ["url", "registry", "git", "path", "editable", "directory"] {
                let Some(value) = quoted(line, key) else {
                    continue;
                };
                match key {
                    "url" => assert!(value.starts_with(FILES), "{name}: {value}"),
                    "registry" => assert_eq!(value, INDEX, "{name}"),
                    _ => panic!("{name}: a source by {key}: {line}"),
                }
            }
        }
    }
    // And the pack's own file names no index, no other place to find a package, and
    // no source of its own for one.
    let pyproject = std::fs::read_to_string(pack_folder().join("pyproject.toml")).unwrap();
    for word in [
        "index",
        "find-links",
        "sources",
        "://",
        "no-binary",
        "no-build-isolation",
    ] {
        let code = pyproject
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .find(|line| line.contains(word));
        assert_eq!(code, None, "{word}");
    }
}

#[test]
fn a_uv_that_does_not_end_within_its_limit_is_killed_and_waited_for() {
    let scratch = Scratch::new("setup-deaf-uv");
    let models = models_in(&scratch);
    std::fs::create_dir(&models).unwrap();
    std::fs::set_permissions(&models, std::fs::Permissions::from_mode(0o700)).unwrap();
    // It notes its process id, and becomes a sleep of 120 seconds under that id: it
    // ends itself then, and not before.
    let noted = scratch.path().join("pid");
    let stand_in = scratch.path().join("uv");
    script(
        &stand_in,
        &format!(
            "#!/bin/bash\necho $$ >'{}'\nexec /bin/sleep 120\n",
            noted.display()
        ),
    );
    let mut install = install_into(&models);
    install.uv = stand_in;
    install.limits = Limits {
        quick: Duration::from_secs(1),
        extra: Duration::from_secs(1),
    };
    let started = Instant::now();
    let refused = pack_setup::install(&install).expect_err("uv never answers");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(
        refused.contains("uv did not end within 1 seconds; it was killed and waited for"),
        "{refused}"
    );
    let pid: u32 = std::fs::read_to_string(&noted)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    // Gone: not running, and not an ended process nobody waited for.
    assert!(
        processes().iter().all(|listed| listed.pid != pid),
        "the stand-in is still listed"
    );
    // And nothing was made.
    assert_eq!(std::fs::read_dir(&models).unwrap().count(), 0);
}

#[test]
fn uv_is_given_the_allow_list_and_no_users_config() {
    let scratch = Scratch::new("setup-allow-list");
    let models = models_in(&scratch);
    let (stand_in, notes) = noting_uv(&scratch);
    let planted = [
        ("LANG", "C"),
        ("OPENAI_API_KEY", "made-up-key-for-a-test"),
        ("HF_TOKEN", "hf_madeUpTokenForATest"),
        ("UV_INDEX_URL", "https://index.example.invalid/simple"),
        ("UV_EXTRA_INDEX_URL", "https://extra.example.invalid/simple"),
        ("UV_CONFIG_FILE", "/nowhere/uv.toml"),
        ("PIP_INDEX_URL", "https://pip.example.invalid/simple"),
        ("PYTHONPATH", "/nowhere"),
        ("VIRTUAL_ENV", "/nowhere"),
        ("UV_PYTHON_DOWNLOADS", "automatic"),
    ];
    let out = setup(&models, Some(&stand_in), &planted);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let calls = calls(&notes);
    assert!(!calls.is_empty());
    for call in &calls {
        // No user's configuration: neither a file nor the variables.
        assert!(
            call.args.iter().any(|arg| arg == "--no-config"),
            "{:?}",
            call.args
        );
        assert!(call.args.iter().any(|arg| arg == "--no-python-downloads"));
        assert!(call.args.iter().any(|arg| arg == "--no-cache"));
        for (name, value) in [
            ("UV_NO_CONFIG", "1"),
            ("UV_NO_CACHE", "1"),
            ("UV_PYTHON_DOWNLOADS", "never"),
            ("UV_CONCURRENT_DOWNLOADS", "4"),
            ("UV_CONCURRENT_INSTALLS", "4"),
            ("UV_CONCURRENT_BUILDS", "1"),
            ("PATH", "/usr/bin:/bin"),
        ] {
            assert_eq!(
                call.vars.get(name).map(String::as_str),
                Some(value),
                "{name}"
            );
        }
        // The locale passes; nothing else of the caller's does.
        assert_eq!(call.vars.get("LANG").map(String::as_str), Some("C"));
        for (name, _) in &planted[1..] {
            if *name != "UV_PYTHON_DOWNLOADS" {
                assert!(!call.vars.contains_key(*name), "{name} was handed on");
            }
        }
        // Every name is one setup sets, and none is a key's.
        for name in call.vars.keys() {
            assert!(
                pack_setup::UV_NAMES.contains(&name.as_str()),
                "{name} is not on the list"
            );
            for end in ["_KEY", "_TOKEN", "_SECRET"] {
                assert!(!name.ends_with(end), "{name}");
            }
        }
    }
}

#[test]
fn the_worker_setup_starts_for_its_hello_is_given_a_lifetime_of_120_seconds_and_is_gone_when_setup_ends()
 {
    let scratch = Scratch::new("setup-lifetime");
    let models = models_in(&scratch);
    // What `setup` starts for its hello: the worker itself, from the environment it
    // made, with a lifetime of its own, 120 seconds, whatever any config says.
    let place = pack_setup::at(&models);
    let launch = pack_setup::hello_launch(&place, &models);
    assert_eq!(launch.lifetime_s, 120);
    assert_eq!(
        launch.module(),
        whizbang_backends::pack::worker::WORKER_MODULE
    );
    assert_eq!(launch.python, place.python);
    assert_eq!(launch.source, place.source);
    assert!(launch.more().is_empty(), "no fault of the fakes");
    let out = setup(&models, None, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("lifetime 120 s"));
    // Once `setup` has ended, no process names its models directory: the worker was
    // stopped and waited for.
    let named = models.display().to_string();
    let left: Vec<_> = processes()
        .into_iter()
        .filter(|listed| listed.command.contains(&named))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn the_pack_this_build_carries_is_the_repositorys() {
    // What `setup` installs is what the repository holds: its file, its lock, its
    // pinned Python, every module of its source and its tool file (plan 2C, task X1);
    // and nothing else.
    let carried = Pack::carried();
    let mut wanted: Vec<String> = [
        "pyproject.toml",
        "uv.lock",
        ".python-version",
        "tools/outline.py",
        "tools/per_subject.py",
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect();
    let package = pack_folder().join("src").join("whizbang_pack");
    for entry in std::fs::read_dir(&package).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        if name.ends_with(".py") {
            wanted.push(format!("src/whizbang_pack/{name}"));
        }
    }
    wanted.sort();
    let mut have: Vec<String> = carried
        .files()
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    have.sort();
    assert_eq!(have, wanted);
    for (name, bytes) in carried.files() {
        assert_eq!(
            &std::fs::read(pack_folder().join(name)).unwrap(),
            bytes,
            "{name}"
        );
    }
}

/// The packages ADR 0022 names as the runtime it chose, each with its version: the rows
/// of the table under its heading "The runtime's packages".
fn the_runtime_the_adr_names() -> (String, Vec<(String, String)>) {
    let adr = std::fs::read_to_string(pack_folder().join("../docs/adr/0022-the-first-runtime.md"))
        .expect("ADR 0022 is there");
    let start = adr
        .find("\n### The runtime's packages\n")
        .expect("the ADR has its table of the runtime's packages");
    let section = &adr[start + 1..];
    let section = &section[..section[4..].find("\n#").map_or(section.len(), |at| at + 4)];
    let extra = section
        .lines()
        .find_map(|line| {
            let at = line.find("the pack's extra `")? + "the pack's extra `".len();
            Some(line[at..line[at..].find('`')? + at].to_owned())
        })
        .expect("the ADR names the pack's extra");
    let mut packages = Vec::new();
    for line in section.lines().filter(|line| line.starts_with("| `")) {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        packages.push((cells[1].trim_matches('`').to_owned(), cells[2].to_owned()));
    }
    (extra, packages)
}

#[test]
fn the_runtime_the_adr_names_is_the_one_the_lock_pins() {
    // ADR 0022 names the runtime by the pack's extra and its packages at their versions;
    // the pack's file pins the extra to exactly those, and the lock holds each at it.
    let (extra, named) = the_runtime_the_adr_names();
    assert!(!named.is_empty(), "the ADR names the runtime's packages");
    let (_, listed) = extras()
        .into_iter()
        .find(|(name, _)| *name == extra)
        .unwrap_or_else(|| panic!("the pack has no extra {extra}"));
    let mut pinned: Vec<String> = listed;
    pinned.sort();
    let mut wanted: Vec<String> = named
        .iter()
        .map(|(name, version)| format!("{name}=={version}"))
        .collect();
    wanted.sort();
    assert_eq!(pinned, wanted, "the extra {extra} pins what the ADR names");
    let blocks = lock_blocks();
    for (name, version) in &named {
        let block = blocks
            .iter()
            .find(|block| field(block, "name") == Some(name.as_str()))
            .unwrap_or_else(|| panic!("{name} is not in the lock"));
        assert_eq!(field(block, "version"), Some(version.as_str()), "{name}");
    }
}
