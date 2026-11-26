//! The command line of a run (plan m5, task W2, item 5): the `whizbang` command that
//! makes the same call again, from what its record asked and the settings it resolved.
//! Each action's subcommand and flags are its own (`whizbang rascal remove-checked`,
//! `whizbang squint check`), so the map from an action to its command lives here, beside
//! the commands, and the playground is handed it ([`words`]).
//!
//! - Every input is named by its identity in the store; the store, the config, the env
//!   file and the models directory by their paths, never a value of the env file.
//! - A choice the run resolved from the config's default (a backend, a judge) is written
//!   as an explicit flag, so a later change of default does not change what is called.
//! - An argument equal to its descriptor's default is left out: the command fills it in
//!   as the run did.
//! - A check of a use case is said whole, its requirements included, by `squint check
//!   --job-json` with each of its images by `--image`, in the job's order.
//! - A recipe the build does not ship, from a file the config lists, is `whizbang rascal
//!   run <file>` with its inputs as `NAME=VALUE`.
//! - Every flag that takes a value is written with it as one word, `--flag=value`, so a
//!   value that begins with `-` (a prompt, a path) is never read as a flag. Every command
//!   is parsed back by the command line's own parser before it is said.
//! - One of rascal's operations is said by its own flags, and held to its own parse: what
//!   the parse asks must be what the run asked, or no command is said.

use std::collections::BTreeMap;

use clap::Parser as _;
use whizbang::play_result::{Commanded, same_value};
use whizbang_core::contract::{Kind, Value};

/// What a recipe's run takes from its runner, and how a command says it.
const UNTIL: &str = whizbang_core::recipe::UNTIL;
const SET: &str = whizbang_core::recipe::SET;

/// One entry of the map: an action by name, its subcommand, the arguments it takes as
/// flags, and the ports it takes as flags, each by the flag's name.
pub(crate) struct Entry {
    pub action: &'static str,
    pub command: &'static [&'static str],
    pub arguments: &'static [(&'static str, &'static str)],
    pub ports: &'static [(&'static str, &'static str)],
    /// Whether it takes `--until` and `--set` (a recipe's runner's).
    pub steps: bool,
}

/// Every recipe and loop this build ships that a command runs, and the operations whose
/// commands are not of the common form, with their flags. A test holds the map to the
/// description and to the commands' own parser.
pub(crate) const ENTRIES: &[Entry] = &[
    Entry {
        action: "remove_prompt",
        command: &["rascal", "remove-prompt"],
        arguments: &[
            ("prompt", "--prompt"),
            ("backend", "--backend"),
            ("filler", "--filler"),
        ],
        ports: &[("image", "--image"), ("protect", "--protect")],
        steps: true,
    },
    Entry {
        action: "remove",
        command: &["rascal", "remove"],
        arguments: &[("backend", "--backend")],
        ports: &[
            ("image", "--image"),
            ("location", "--location"),
            ("protect", "--protect"),
        ],
        steps: true,
    },
    Entry {
        action: "repair_area",
        command: &["rascal", "repair-area"],
        arguments: &[("backend", "--backend")],
        ports: &[("image", "--image"), ("location", "--location")],
        steps: true,
    },
    Entry {
        action: "mask_rect",
        command: &["rascal", "mask-rect"],
        arguments: &[("box", "--box")],
        ports: &[("image", "--image")],
        steps: true,
    },
    Entry {
        action: "protect",
        command: &["rascal", "protect"],
        arguments: &[],
        ports: &[("image", "--image"), ("mask", "--mask")],
        steps: true,
    },
    Entry {
        action: "repair",
        command: &["rascal", "repair"],
        arguments: &[("backend", "--backend")],
        ports: &[("image", "--image"), ("stroke", "--stroke")],
        steps: true,
    },
    Entry {
        action: "seam",
        command: &["rascal", "seam"],
        arguments: &[("backend", "--backend")],
        ports: &[("image", "--image")],
        steps: true,
    },
    Entry {
        action: whizbang_rascal::checked::NAME,
        command: &["rascal", "remove-checked"],
        arguments: &[
            ("prompt", "--prompt"),
            ("attempts", "--attempts"),
            ("check", "--check"),
            ("backend", "--backend"),
            ("filler", "--filler"),
            ("judge", "--judge"),
        ],
        ports: &[("image", "--image"), ("protect", "--protect")],
        steps: false,
    },
    Entry {
        action: "fill",
        command: &["rascal", "fill"],
        arguments: &[("backend", "--backend")],
        ports: &[("image", "--image"), ("mask", "--mask")],
        steps: false,
    },
    Entry {
        action: "paste",
        command: &["rascal", "paste"],
        arguments: &[("mode", "--mode")],
        ports: &[("image", "--image"), ("fill", "--fill"), ("mask", "--mask")],
        steps: false,
    },
    Entry {
        action: "grow",
        command: &["rascal", "grow"],
        arguments: &[("px", "--px"), ("down_px", "--down-px")],
        ports: &[("mask", "--mask")],
        steps: false,
    },
];

/// A flag and its value as one word, `--flag=value`: a value that begins with `-` (a
/// prompt, a path) is then never read as a flag of its own.
fn flag_value(flag: impl AsRef<str>, value: impl AsRef<str>) -> String {
    format!("{}={}", flag.as_ref(), value.as_ref())
}

/// A value as a command line types it.
fn typed(value: &Value) -> String {
    match value {
        Value::Number(number) => format!("{number}"),
        Value::Integer(number) => number.to_string(),
        Value::Boolean(boolean) => boolean.to_string(),
        Value::Text(text) => text.clone(),
    }
}

fn path(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The flags every command that runs an action takes: the store, then the files.
fn common(asked: &Commanded) -> Vec<String> {
    let mut words = vec![flag_value("--store", path(&asked.store))];
    if let Some(config) = &asked.config {
        words.push(flag_value("--config", path(config)));
    }
    if let Some(env) = &asked.env_file {
        words.push(flag_value("--env-file", path(env)));
    }
    if let Some(models) = &asked.models_dir {
        words.push(flag_value("--models-dir", path(models)));
    }
    words
}

/// Every argument, and the choices written out, by name.
fn arguments_and_choices(asked: &Commanded) -> Vec<(String, Value)> {
    let mut all = asked.arguments.clone();
    for (name, backend) in &asked.chosen {
        all.push((name.clone(), Value::text(backend)));
    }
    all
}

/// The runner's `until` and `set`, as `--until` and `--set` once for each line: whether
/// the argument was one of them.
fn steps_of(name: &str, value: &Value, words: &mut Vec<String>) -> bool {
    match (name, value) {
        (UNTIL, Value::Text(step)) => words.push(flag_value("--until", step)),
        (SET, Value::Text(set)) => {
            for line in set.lines().filter(|line| !line.trim().is_empty()) {
                words.push(flag_value("--set", line));
            }
        }
        _ => return false,
    }
    true
}

fn not_said(action: &str, what: &str) -> String {
    format!("no command line of {action} says its {what}")
}

/// The words of an entry's command.
fn by_entry(entry: &Entry, asked: &Commanded) -> Result<Vec<String>, String> {
    let mut words: Vec<String> = entry
        .command
        .iter()
        .map(|word| (*word).to_owned())
        .collect();
    for (port, id) in &asked.inputs {
        let Some((_, flag)) = entry.ports.iter().find(|(name, _)| name == port) else {
            return Err(not_said(entry.action, &format!("input {port}")));
        };
        words.push(flag_value(*flag, id));
    }
    for (name, value) in arguments_and_choices(asked) {
        if entry.steps && steps_of(&name, &value, &mut words) {
            continue;
        }
        let Some((_, flag)) = entry
            .arguments
            .iter()
            .find(|(argument, _)| *argument == name)
        else {
            return Err(not_said(entry.action, &format!("argument {name}")));
        };
        words.push(flag_value(*flag, typed(&value)));
    }
    words.extend(common(asked));
    Ok(words)
}

/// `segment`'s query by its kind: `--phrase`, `--point` or `--box`.
fn segment(asked: &Commanded) -> Result<Vec<String>, String> {
    let mut words = vec!["rascal".to_owned(), "segment".to_owned()];
    for (port, id) in &asked.inputs {
        if port != "image" {
            return Err(not_said("segment", &format!("input {port}")));
        }
        words.push(flag_value("--image", id));
    }
    let all = arguments_and_choices(asked);
    let kind = all
        .iter()
        .chain(&asked.defaults)
        .find(|(name, _)| name == "kind")
        .map_or_else(|| "phrase".to_owned(), |(_, value)| typed(value));
    for (name, value) in &all {
        let flag = match name.as_str() {
            "kind" => continue,
            "query" => match kind.as_str() {
                "phrase" => "--phrase",
                "point" => "--point",
                "box" => "--box",
                _ => return Err(not_said("segment", "kind")),
            },
            "max_instances" => "--max-instances",
            "backend" => "--backend",
            _ => return Err(not_said("segment", &format!("argument {name}"))),
        };
        words.push(flag_value(flag, typed(value)));
    }
    words.extend(common(asked));
    Ok(words)
}

/// A check of a use case: its whole job, its images in order, its judge, and what it
/// measured first and offered the judge, as `squint check`'s flags say them.
fn use_case(asked: &Commanded) -> Result<Vec<String>, String> {
    let mut words = vec![
        "squint".to_owned(),
        "check".to_owned(),
        asked.action.clone(),
    ];
    let mut job = None;
    let mut set = None;
    for (name, value) in arguments_and_choices(asked) {
        match (name.as_str(), value) {
            ("job", Value::Text(text)) => job = Some(text),
            ("judge", value) => words.push(flag_value("--judge", typed(&value))),
            (SET, Value::Text(text)) => set = Some(text),
            _ => return Err(not_said(&asked.action, &format!("argument {name}"))),
        }
    }
    let job = job.ok_or_else(|| not_said(&asked.action, "job"))?;
    words.push(flag_value("--job-json", job));
    for (port, id) in &asked.inputs {
        if port != "images" {
            return Err(not_said(&asked.action, &format!("input {port}")));
        }
        words.push(flag_value("--image", id));
    }
    if let Some(set) = set {
        words.extend(measuring(&asked.action, &set)?);
    }
    words.extend(common(asked));
    Ok(words)
}

/// The flags of what a check measured first and offered its judge, from the settings
/// every door states (`door::check_call`): none where it is the default.
fn measuring(action: &str, set: &str) -> Result<Vec<String>, String> {
    use whizbang::door::Details;
    use whizbang_squint::recipes::{DETAILS_STEP, JUDGE_STEP};
    let mut said = BTreeMap::new();
    for line in set.lines().filter(|line| !line.trim().is_empty()) {
        let Some((name, value)) = line.split_once('=') else {
            return Err(not_said(action, "settings"));
        };
        said.insert(name.to_owned(), value.to_owned());
    }
    let mut take = |name: String| said.remove(&name);
    let mut words = Vec::new();
    let details_name = take(format!("{DETAILS_STEP}.details")).unwrap_or_else(|| "none".to_owned());
    let details =
        Details::stated(Some(&details_name)).ok_or_else(|| not_said(action, "details"))?;
    if details != Details::DEFAULT {
        words.push(flag_value("--details", details_name));
    }
    if let Some(detail) = take(format!("{DETAILS_STEP}.detail")) {
        words.push(flag_value("--detail", detail));
    }
    let offered = take(format!("{JUDGE_STEP}.tools")).unwrap_or_default();
    let (base, _) = details.tools_and_name();
    let extra = offered
        .strip_prefix(base)
        .ok_or_else(|| not_said(action, "tools"))?;
    for tool in extra.split(',').filter(|tool| !tool.is_empty()) {
        words.push(flag_value("--tool", tool));
    }
    if let Some(projection) = take(format!("{JUDGE_STEP}.projection"))
        && projection != details.projection()
    {
        words.push(flag_value("--projection", projection));
    }
    if let Some(second) = take(format!("{JUDGE_STEP}.second_look"))
        && second != "off"
    {
        words.push(flag_value("--second-look", second));
    }
    if let Some(submission) = take(format!("{JUDGE_STEP}.submission")) {
        words.push(flag_value("--submission", submission));
    }
    if let Some(name) = said.keys().next() {
        return Err(not_said(action, &format!("setting {name}")));
    }
    Ok(words)
}

/// A recipe from a file the config lists: `rascal run <file>`, its inputs and its text
/// arguments as `--input NAME=VALUE`.
fn recipe_file(asked: &Commanded, file: &std::path::Path) -> Result<Vec<String>, String> {
    let mut words = vec!["rascal".to_owned(), "run".to_owned(), path(file)];
    for (port, id) in &asked.inputs {
        words.push(flag_value("--input", format!("{port}={id}")));
    }
    for (name, value) in arguments_and_choices(asked) {
        if steps_of(&name, &value, &mut words) {
            continue;
        }
        match value {
            Value::Text(text) => words.push(flag_value("--input", format!("{name}={text}"))),
            _ => return Err(not_said(&asked.action, &format!("argument {name}"))),
        }
    }
    words.extend(common(asked));
    Ok(words)
}

/// One of rascal's operations by its own flags: each argument as `--<name>` (its `_` a
/// `-`), each input as `--<port>`; then held to the command's own parse.
fn operation(asked: &Commanded) -> Result<Vec<String>, String> {
    let mut words = vec!["rascal".to_owned(), asked.action.replace('_', "-")];
    for (port, id) in &asked.inputs {
        words.push(flag_value(
            format!("--{}", port.replace('_', "-")),
            id.clone(),
        ));
    }
    let wanted = arguments_and_choices(asked);
    for (name, value) in &wanted {
        let flag = format!("--{}", name.replace('_', "-"));
        match value {
            Value::Boolean(true) => words.push(flag),
            Value::Boolean(false) => {}
            value => words.push(flag_value(flag, typed(value))),
        }
    }
    words.extend(common(asked));
    // Parsed back by the command's own parser: what it asks must be what the run asked.
    let said = asked_by(&words)
        .ok_or_else(|| format!("no command line runs {} by itself", asked.action))?;
    let (action, arguments, inputs) = said;
    let held = |name: &str, value: &Value| {
        wanted
            .iter()
            .chain(&asked.defaults)
            .any(|(said, given)| said == name && same_value(given, value))
    };
    let same_arguments = arguments.iter().all(|(name, value)| held(name, value))
        && wanted.iter().all(|(name, value)| {
            arguments
                .iter()
                .any(|(said, given)| said == name && same_value(given, value))
        });
    let same_inputs = inputs.len() == asked.inputs.len()
        && inputs
            .iter()
            .zip(&asked.inputs)
            .all(|((port, id), (wanted_port, wanted_id))| port == wanted_port && id == wanted_id);
    if action != asked.action || !same_arguments || !same_inputs {
        return Err(format!(
            "no command line says this call of {}",
            asked.action
        ));
    }
    Ok(words)
}

/// What the words of a command of one of rascal's operations ask, as its own parse reads
/// them; none when they do not parse or are no operation's command.
pub(crate) fn asked_by(words: &[String]) -> Option<super::rascal::Said> {
    let parsed = super::Cli::try_parse_from(
        std::iter::once("whizbang".to_owned()).chain(words.iter().cloned()),
    )
    .ok()?;
    let super::Command::Rascal { command } = &parsed.command else {
        return None;
    };
    super::rascal::operation_of(command)
}

/// The words of the command that makes the run's call again, `whizbang` first; or why no
/// command line says it.
pub(crate) fn words(asked: &Commanded) -> Result<Vec<String>, String> {
    let mut words = if let Some(entry) = ENTRIES.iter().find(|entry| entry.action == asked.action) {
        by_entry(entry, asked)?
    } else if asked.action == "segment" {
        segment(asked)?
    } else if whizbang_squint::use_case::NAMES.contains(&asked.action.as_str()) {
        use_case(asked)?
    } else if let Some(file) = &asked.recipe_file {
        recipe_file(asked, file)?
    } else if asked.kind == Some(Kind::Primitive) {
        operation(asked)?
    } else {
        return Err(format!("no command line runs {}", asked.action));
    };
    words.insert(0, "whizbang".to_owned());
    // Every command, whatever made it, is held to the command line's own parser before it
    // is said: what it cannot read is not a command.
    if super::Cli::try_parse_from(&words).is_err() {
        return Err(format!(
            "no command line says this call of {}: its words do not parse",
            asked.action
        ));
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    //! The copied command makes the same call (plan m5, task W2, item 5): a run made
    //! through the playground's door, its command said by [`words`], then run by the
    //! binary's own parse and dispatch ([`crate::run`]) over the same fake backends (the
    //! test kit's `playkit`), against the same store, and the two runs held to the same
    //! call. Not the request fingerprint, which tells an omitted default from the same
    //! value written out.

    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::sync::Arc;

    use serde_json::{Map, Value as Json, json};
    use whizbang::playing::Playground;
    use whizbang::wiring::{self, Make};
    use whizbang_core::contract::record::RunRecord;
    use whizbang_core::contract::{Answer, Executor, Request, RunId};
    use whizbang_core::store::Reader;
    use whizbang_core::store::fs::RealFs;
    use whizbang_serve::play::Play;
    use whizbang_testkit::playkit::{KIND, PlayKit};
    use whizbang_testkit::scratch::Scratch;

    use super::words;

    /// The fixture host's backends, with a default for each capability; `fill_default`
    /// names the filler's default, and `recipe` a recipe file the config lists.
    fn config(fill_default: &str, recipe: &Path) -> String {
        format!(
            "[backends.segment]\ndefault = \"local-seg\"\n\n\
             [backends.segment.local-seg]\nkind = \"{KIND}\"\nmodel = \"local-seg\"\n\
             url = \"https://segment.example.org/v1\"\n\n\
             [backends.fill]\ndefault = \"{fill_default}\"\n\n\
             [backends.fill.cloud-fill]\nkind = \"{KIND}\"\nmodel = \"fill\"\n\
             url = \"https://fill.example.org/v1\"\n\n\
             [backends.fill.other-fill]\nkind = \"{KIND}\"\nmodel = \"other\"\n\
             url = \"https://fill.example.org/v2\"\n\n\
             [backends.judge]\ndefault = \"cloud-judge\"\n\n\
             [backends.judge.cloud-judge]\nkind = \"{KIND}\"\nmodel = \"judge\"\n\
             url = \"https://judge.example.org/v1\"\n\n\
             [egress.recipients.segment]\norigin = \"https://segment.example.org\"\n\
             classes = [\"prompts\", \"source_images\"]\n\n\
             [egress.recipients.fill]\norigin = \"https://fill.example.org\"\n\
             classes = [\"source_images\", \"derived_images\"]\n\n\
             [egress.recipients.judge]\norigin = \"https://judge.example.org\"\n\
             classes = [\"prompts\", \"source_images\", \"derived_images\", \"evidence\"]\n\n\
             [recipes]\npaths = [\"{}\"]\n",
            recipe.display()
        )
    }

    /// A user's recipe: a rectangle as a mask, under a name of its own.
    const MY_RECT: &str = "version = 1\nname = \"my_rect\"\n\
        purpose = \"Makes a box on a picture a mask, as a user's own recipe.\"\n\
        use = \"To test a recipe file the config lists.\"\nuse_case = \"custom\"\n\n\
        [inputs]\nimage = { type = \"Image\" }\nbox = { type = \"Text\" }\n\n\
        [[step]]\nid = \"rect\"\nop = \"rect\"\ninputs = { image = \"inputs.image\" }\n\
        box = \"inputs.box\"\n\n\
        [[step]]\nid = \"rasterize\"\nop = \"rasterize\"\n\
        inputs = { location = \"rect.location\", image = \"inputs.image\" }\n\n\
        [outputs]\nmask = \"rasterize.mask\"\n";

    /// A store with ten dollars, the config beside it, and the kit both doors run over.
    struct Bench {
        scratch: Scratch,
        kit: Arc<PlayKit>,
    }

    impl Bench {
        fn new(label: &str) -> Bench {
            let scratch = Scratch::new(label);
            let recipe = scratch.path().join("my_rect.toml");
            std::fs::write(&recipe, MY_RECT).unwrap();
            let bench = Bench {
                scratch,
                kit: PlayKit::new(),
            };
            bench.configure("cloud-fill");
            let opening =
                wiring::opening(&bench.root(), Make::TheRoot, wiring::recovery()).unwrap();
            let writer = opening.writer().unwrap();
            let ten = whizbang_core::spend::Amount::new(10_000_000).unwrap();
            whizbang_core::spend::reset(&writer, ten, 1).unwrap();
            let _ = super::super::rascal::MORE_KINDS.set(vec![bench.kit.kind()]);
            bench
        }

        /// A folder whose name begins with `-`, as a flag's would.
        fn root(&self) -> PathBuf {
            self.scratch.path().join("-store")
        }

        fn config(&self) -> PathBuf {
            self.scratch.path().join("whizbang.toml")
        }

        fn configure(&self, fill_default: &str) {
            let recipe = self.scratch.path().join("my_rect.toml");
            std::fs::write(self.config(), config(fill_default, &recipe)).unwrap();
        }

        /// The playground's door and its executor over the store's writer.
        fn door(&self) -> (Arc<dyn Executor>, Playground) {
            // Over the build's kinds and the kit's, as the copied command's are.
            let kinds = super::super::rascal::kinds_of_a_command();
            let over = whizbang::wiring::Over::nothing()
                .config(Some(&self.config()))
                .kinds(kinds);
            let opening = wiring::opening(&self.root(), Make::Nothing, wiring::recovery()).unwrap();
            let writer = opening.writer().unwrap();
            let registry = wiring::registry_over(&over, false).unwrap();
            let descriptors = registry.descriptors().cloned().collect();
            let executor = wiring::runner(registry, writer.clone(), &over).unwrap();
            let door = Playground::new(descriptors, Arc::new(over))
                .with_store(writer, &self.root())
                .with_commands(Arc::new(words));
            (Arc::new(executor), door)
        }

        /// A picture kept as the page's are.
        fn picture(&self, shade: u8) -> String {
            use image::ImageEncoder;
            let rgb: Vec<u8> = (0..48 * 32)
                .flat_map(|at: u32| [shade, (at % 251) as u8, 90])
                .collect();
            let mut png = Vec::new();
            image::codecs::png::PngEncoder::new(&mut png)
                .write_image(&rgb, 48, 32, image::ExtendedColorType::Rgb8)
                .unwrap();
            let (_, door) = self.door();
            let kept = door.picture(&png, "porch.png").unwrap();
            door.closing();
            kept["id"].as_str().unwrap().to_owned()
        }

        /// Runs the call through the door, and answers its run and its command.
        fn ran(&self, action: &str, arguments: Json) -> (RunId, Json) {
            let (executor, door) = self.door();
            let arguments: Map<String, Json> = arguments.as_object().unwrap().clone();
            let (call, _) = door.call(action, &arguments).unwrap();
            let run = match executor.start(&Request::new(call)).unwrap() {
                Answer::Started(run) => run,
                Answer::Ran(finished) => finished.run().clone(),
                Answer::Repeat(earlier) => earlier.run().clone(),
            };
            executor.wait(&run).unwrap();
            let command = door.command(&run).unwrap();
            drop(executor);
            door.closing();
            (run, command)
        }

        /// Runs the command's words through the binary's parse and dispatch, and answers
        /// the run it made: the newest top run of the action that is not `first`.
        fn again(&self, command: &Json, first: &RunId) -> RunId {
            let words: Vec<String> = serde_json::from_value(command["words"].clone()).unwrap();
            assert_eq!(words[0], "whizbang", "{command}");
            let code = crate::run(words);
            assert_eq!(
                format!("{code:?}"),
                format!("{:?}", ExitCode::SUCCESS),
                "{command}"
            );
            let action = self.record(first).asked().action.clone();
            let reader = Reader::open(&self.root(), Arc::new(RealFs)).unwrap();
            let generation = reader.snapshot().unwrap();
            let mut made: Vec<RunRecord> = generation
                .runs()
                .keys()
                .filter(|run| *run != first.as_str())
                .filter_map(|run| {
                    whizbang_core::contract::record::stored(&generation, run, |e| reader.read(e))?
                        .ok()
                })
                .filter(|record| {
                    record.identity().parent.is_none() && record.asked().action == action
                })
                .collect();
            made.sort_by_key(|record| record.time().end_ms);
            RunId::parse(&made.last().expect("the command made a run").identity().run).unwrap()
        }

        fn record(&self, run: &RunId) -> RunRecord {
            let reader = Reader::open(&self.root(), Arc::new(RealFs)).unwrap();
            let generation = reader.snapshot().unwrap();
            whizbang_core::contract::record::stored(&generation, run.as_str(), |e| reader.read(e))
                .unwrap()
                .unwrap()
        }

        /// Every capability's backend and model in the run and those it called.
        fn answered(&self, run: &RunId) -> Vec<(String, String, String)> {
            let reader = Reader::open(&self.root(), Arc::new(RealFs)).unwrap();
            let generation = reader.snapshot().unwrap();
            let mut seen = Vec::new();
            let mut runs = vec![run.as_str().to_owned()];
            while let Some(run) = runs.pop() {
                let Some(Ok(record)) =
                    whizbang_core::contract::record::stored(&generation, &run, |e| reader.read(e))
                else {
                    continue;
                };
                for model in record.models() {
                    seen.push((
                        model.capability.name().to_owned(),
                        model.backend.clone(),
                        model.model.clone(),
                    ));
                }
                runs.extend(record.steps().iter().map(|step| step.run().to_owned()));
            }
            seen.sort();
            seen.dedup();
            seen
        }

        /// The two runs made the same call: the action, its arguments once each default
        /// is filled in, its inputs, its resolved settings and config, and the backend
        /// and model of each need.
        fn same_call(&self, first: &RunId, second: &RunId) {
            self.same_call_under(first, second, true);
        }

        /// The same, the config's digest compared only when `config` says so: a command
        /// copied after the config changed runs under the config as it is.
        fn same_call_under(&self, first: &RunId, second: &RunId, config: bool) {
            let (a, b) = (self.record(first), self.record(second));
            assert_eq!(a.asked().action, b.asked().action);
            // The same arguments, once the choices the first resolved from a default are
            // filled in: the second names them, and is answered by the same backends.
            let mut filled = a.asked().arguments.clone();
            for argument in &b.asked().arguments {
                if !filled.iter().any(|given| given.name == argument.name) {
                    assert!(
                        ["backend", "filler", "judge"].contains(&argument.name.as_str()),
                        "{argument:?} was not asked the first time"
                    );
                    filled.push(argument.clone());
                }
            }
            let mut asked = b.asked().arguments.clone();
            filled.sort_by(|x, y| x.name.cmp(&y.name));
            asked.sort_by(|x, y| x.name.cmp(&y.name));
            assert_eq!(filled, asked);
            assert_eq!(a.asked().settings, b.asked().settings);
            if config {
                assert_eq!(a.asked().config, b.asked().config);
            } else {
                assert_ne!(a.asked().config, b.asked().config);
            }
            assert_eq!(a.inputs(), b.inputs());
            assert_eq!(self.answered(first), self.answered(second));
        }
    }

    #[test]
    fn the_command_of_a_run_makes_the_same_call() {
        let bench = Bench::new("command-same-call");
        let picture = bench.picture(40);
        // A primitive, a user's recipe file, a shipped recipe and a loop, each made again
        // by its command.
        let mut made = Vec::new();
        let mask = |run: &RunId| {
            let record = bench.record(run);
            record
                .outputs()
                .iter()
                .find(|o| o.port == "mask")
                .map(|o| o.artifact.to_string())
        };
        let (first, command) = bench.ran("rect", json!({"image": picture, "box": "4 4 20 20"}));
        made.push((first, command));
        let (first, command) = bench.ran("my_rect", json!({"image": picture, "box": "2 3 30 20"}));
        let words: Vec<String> = serde_json::from_value(command["words"].clone()).unwrap();
        assert_eq!(words[1..3], ["rascal", "run"], "{command}");
        let drawn = mask(&first).unwrap();
        made.push((first, command));
        made.push(bench.ran("protect", json!({"image": picture, "mask": drawn})));
        // No line break: the loop refuses a phrase with one (plan mE, task L2); the
        // quoting of a line break is held by the words of other runs.
        let phrase = "-a hose: the 'hose' by the $DOOR and `its` \"reel\"";
        let (removal, command) = bench.ran(
            "remove_checked",
            json!({"image": picture, "prompt": phrase, "attempts": 1}),
        );
        // A value that begins with `-` is said with its flag, as one word.
        let words: Vec<String> = serde_json::from_value(command["words"].clone()).unwrap();
        assert!(words.contains(&format!("--prompt={phrase}")), "{command}");
        let store = words
            .iter()
            .find(|word| word.starts_with("--store="))
            .unwrap();
        assert!(
            store.ends_with("/-store") && store.starts_with("--store=/"),
            "{store}"
        );
        made.push((removal.clone(), command));
        for (first, command) in &made {
            let second = bench.again(command, first);
            bench.same_call(first, &second);
        }
        // The user's recipe file edited since its run: the command says so, as it says a
        // changed config.
        let recipe = bench.scratch.path().join("my_rect.toml");
        std::fs::write(
            &recipe,
            MY_RECT.replace("as a user's own", "as a user's edited"),
        )
        .unwrap();
        let (_, door) = bench.door();
        let edited = door.command(&made[1].0).unwrap();
        door.closing();
        assert_eq!(edited["recipe_changed"], true, "{edited}");
        let text = edited["command"].as_str().unwrap();
        assert!(
            text.starts_with(": 'the recipe file has changed since this run"),
            "{text}"
        );
        assert_eq!(made[0].1["recipe_changed"], false);
        // A check of an edit sent from the removal: the job the loop writes, whole, with
        // its requirements.
        let record = bench.record(&removal);
        let edit = record.outputs().iter().find(|o| o.port == "image").unwrap();
        let arguments = json!({"candidate": edit.artifact.to_string(), "source": picture,
                               "job_of": removal.as_str()});
        let (first, command) = bench.ran("source-fidelity", arguments);
        let text = command["command"].as_str().unwrap();
        assert!(
            text.contains("--job-json") && text.contains("RQ1"),
            "{text}"
        );
        let second = bench.again(&command, &first);
        bench.same_call(&first, &second);
    }

    #[test]
    fn the_command_names_a_default_choice_explicitly_and_says_when_the_config_changed() {
        let bench = Bench::new("command-default");
        let picture = bench.picture(41);
        let (drawn, _) = bench.ran("mask_rect", json!({"image": picture, "box": "1 1 9 9"}));
        let mask = bench.record(&drawn).outputs()[0].artifact.to_string();
        // A fill that names no filler: the config's default answered, and is named.
        let (first, command) = bench.ran("fill", json!({"image": picture, "mask": mask}));
        let words: Vec<String> = serde_json::from_value(command["words"].clone()).unwrap();
        assert!(
            words.contains(&"--backend=cloud-fill".to_owned()),
            "{command}"
        );
        assert_eq!(command["config_changed"], false);
        assert!(!command["command"].as_str().unwrap().starts_with(':'));
        // The config's default changes: the command still calls the backend the run did,
        // and says the config is another than the run's.
        bench.configure("other-fill");
        let (_, door) = bench.door();
        let changed = door.command(&first).unwrap();
        door.closing();
        assert_eq!(changed["config_changed"], true, "{changed}");
        let text = changed["command"].as_str().unwrap();
        let (comment, rest) = text.split_once('\n').unwrap();
        // A line a shell runs as nothing, which an interactive zsh takes too.
        assert!(
            comment.starts_with(": 'the config has changed since this run"),
            "{comment}"
        );
        assert!(comment.contains(changed["recorded_config"].as_str().unwrap()));
        assert!(comment.contains(changed["config"].as_str().unwrap()));
        assert!(rest.contains("--backend=cloud-fill"), "{rest}");
        let second = bench.again(&changed, &first);
        bench.same_call_under(&first, &second, false);
        // A recipe file of the same name as the config's, and another text, is still
        // refused by `rascal run`: one recipe of a name is registered.
        let other = bench.scratch.path().join("other_rect.toml");
        std::fs::write(&other, MY_RECT.replace("Makes a box", "Makes another box")).unwrap();
        let refused = crate::run([
            "whizbang".to_owned(),
            "rascal".to_owned(),
            "run".to_owned(),
            other.display().to_string(),
            format!("--input=image={picture}"),
            "--input=box=1 1 4 4".to_owned(),
            format!("--store={}", bench.root().display()),
            format!("--config={}", bench.config().display()),
        ]);
        assert_eq!(format!("{refused:?}"), format!("{:?}", ExitCode::from(1)));
    }

    #[test]
    fn squint_check_job_json_takes_a_stored_job_with_images_by_identity() {
        let bench = Bench::new("command-job-json");
        let (candidate, source) = (bench.picture(42), bench.picture(43));
        let job = whizbang_rascal::checked::job("the bucket").unwrap();
        let store = bench.root();
        let config = bench.config();
        let line = |images: &[&str], use_case: &str| {
            let mut words: Vec<String> =
                ["whizbang", "squint", "check", use_case, "--job-json", &job]
                    .map(str::to_owned)
                    .to_vec();
            for image in images {
                words.extend(["--image".to_owned(), (*image).to_owned()]);
            }
            words.extend(["--store".to_owned(), store.display().to_string()]);
            words.extend(["--config".to_owned(), config.display().to_string()]);
            words
        };
        let mut measured = line(&[&candidate, &source], "source-fidelity");
        measured.push("--details=first".to_owned());
        let ok = crate::run(measured);
        assert_eq!(format!("{ok:?}"), format!("{:?}", ExitCode::SUCCESS));
        // The run holds the job whole, requirements and all, and its images in order.
        let reader = Reader::open(&store, Arc::new(RealFs)).unwrap();
        let generation = reader.snapshot().unwrap();
        let record = generation
            .runs()
            .keys()
            .filter_map(|run| {
                whizbang_core::contract::record::stored(&generation, run, |e| reader.read(e))?.ok()
            })
            .find(|record| record.identity().parent.is_none())
            .unwrap();
        let held = record
            .asked()
            .arguments
            .iter()
            .find(|a| a.name == "job")
            .unwrap();
        let whizbang_core::contract::record::ArgumentValue::Text(held) = &held.value else {
            panic!("the job is text");
        };
        let held = whizbang_squint::job::Job::from_json(held).unwrap();
        assert_eq!(held, whizbang_squint::job::Job::from_json(&job).unwrap());
        // Run again of it keeps what it measured first and offered its judge.
        let set = (record.asked().arguments.iter())
            .find(|a| a.name == "set")
            .unwrap();
        let whizbang_core::contract::record::ArgumentValue::Text(set) = &set.value else {
            panic!("set is text");
        };
        assert!(set.contains("details.details=first"), "{set}");
        let (_, door) = bench.door();
        let again = json!({"candidate": candidate, "source": source,
                           "job_of": record.identity().run});
        let (call, _) = door
            .call("source-fidelity", again.as_object().unwrap())
            .unwrap();
        door.closing();
        assert_eq!(
            call.arguments()["set"],
            whizbang_core::contract::Value::text(set)
        );
        // A job that gives a picture inline, and one whose pin is no file the store
        // imported the picture from, are each refused.
        let mut inline: Json = serde_json::from_str(&job).unwrap();
        inline["images"][0]["source"] = json!({"base64": "AAAA"});
        let mut pinned: Json = serde_json::from_str(&job).unwrap();
        pinned["images"][0]["source"] = json!({"sha256": "b".repeat(64)});
        for text in [inline.to_string(), pinned.to_string()] {
            let given = [candidate.clone(), source.clone()];
            let refused = whizbang::door::job_json(&text, &given, "source-fidelity", &store);
            assert!(refused.is_err(), "{text}");
        }
        let images: Vec<String> = record
            .inputs()
            .iter()
            .map(|i| i.artifact.to_string())
            .collect();
        assert_eq!(images, [candidate.clone(), source.clone()]);
        // One image too few, a job of another use case, and --image without --job-json:
        // each refused before anything runs.
        let failed = format!("{:?}", ExitCode::from(1));
        assert_eq!(
            format!("{:?}", crate::run(line(&[&candidate], "source-fidelity"))),
            failed
        );
        assert_eq!(
            format!(
                "{:?}",
                crate::run(line(&[&candidate, &source], "reality-checks"))
            ),
            failed
        );
        let parsed = super::super::Cli::try_parse_from([
            "whizbang",
            "squint",
            "check",
            "source-fidelity",
            "--image",
            &candidate,
            "--store",
            "s",
        ]);
        assert!(parsed.is_err());
    }

    #[test]
    fn every_recipe_and_loop_in_the_description_has_an_entry_whose_flags_parse() {
        let registry = wiring::registry().unwrap();
        let names: Vec<String> = registry
            .descriptors()
            .filter(|d| d.kind() != whizbang_core::contract::Kind::Primitive)
            .map(|d| d.name().to_owned())
            .collect();
        for name in &names {
            let covered = super::ENTRIES.iter().any(|entry| entry.action == name)
                || whizbang_squint::use_case::NAMES.contains(&name.as_str())
                || name == "check";
            assert!(covered, "{name} has no command entry");
        }
        // Each entry's flags, every one given, parse with the command's own parser.
        for entry in super::ENTRIES {
            let mut line: Vec<String> = vec!["whizbang".to_owned()];
            line.extend(entry.command.iter().map(|w| (*w).to_owned()));
            for (_, flag) in entry.ports {
                line.extend([(*flag).to_owned(), format!("sha256:{}", "a".repeat(64))]);
            }
            for (name, flag) in entry.arguments {
                let value = match *name {
                    "attempts" => "2",
                    "check" => "auto",
                    "px" | "down_px" => "3",
                    "mode" => "hard",
                    "box" => "1 2 3 4",
                    _ => "words",
                };
                line.extend([(*flag).to_owned(), value.to_owned()]);
            }
            if entry.steps {
                line.extend(["--until", "fill", "--set", "fill.x=1"].map(str::to_owned));
            }
            line.extend(
                [
                    "--store",
                    "s",
                    "--config",
                    "c",
                    "--env-file",
                    "e",
                    "--models-dir",
                    "/m",
                ]
                .map(str::to_owned),
            );
            let parsed = super::super::Cli::try_parse_from(&line);
            assert!(parsed.is_ok(), "{}: {:?}", entry.action, parsed.err());
        }
    }

    use clap::Parser as _;
}
