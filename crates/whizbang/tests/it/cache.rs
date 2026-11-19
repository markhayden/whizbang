//! `cache::…`: the decision cache of `check` (`SPEC.md` section 9; plan 2C, task M1,
//! item 4; ADR 0031). Keys are worked out over jobs made in the test; a hit and a miss
//! are shown through the binary's door with a judge scripted on loopback, whose server
//! answers only as many requests as the test says, so a run that asked once more than
//! it should fails.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use whizbang::door::{self, CheckAsked, Details};
use whizbang_core::artifact::ArtifactId;
use whizbang_core::contract::{Call, Engine, Executor, ModelIdentity, Registry, Value};
use whizbang_core::identity::{BindingId, Digest};
use whizbang_core::store::Store;
use whizbang_core::store::fs::RealFs;
use whizbang_squint::cache::{self, Key, Material};
use whizbang_squint::job::{Job, Role};

use crate::anthropic::{CHECK_CLASSES, Judged, pair, record_of, serving, submission};

// --- the key, over jobs made here ---------------------------------------------------------------

fn image(n: u8) -> (ArtifactId, BindingId) {
    (
        ArtifactId(Digest::of_bytes(&[n])),
        BindingId(Digest::of_bytes(&[n, n])),
    )
}

/// The key of a job, as `check` works it out, under the build's prompt for these tools.
fn key_of(
    job: &Job,
    images: &[(ArtifactId, BindingId)],
    tools: &[&str],
    judge: &ModelIdentity,
) -> Key {
    key_with(job, images, tools, &[], judge)
}

/// The same, with these tool files among the tools, each its name and revision.
fn key_with(
    job: &Job,
    images: &[(ArtifactId, BindingId)],
    tools: &[&str],
    files: &[(String, String)],
    judge: &ModelIdentity,
) -> Key {
    let (system, checklist, version) = cache::prompt_of(job, tools).unwrap();
    let arguments = vec![
        ("use_case".to_owned(), Value::text(&job.use_case)),
        ("tools".to_owned(), Value::text(&tools.join(","))),
        (
            "cell".to_owned(),
            Value::text("a cell of a matrix, which check does not read"),
        ),
    ];
    cache::key(&Material {
        job,
        arguments: &arguments,
        images,
        system: &system,
        checklist: &checklist,
        rules_version: version,
        tools,
        files,
        judge,
        config: Digest::of_bytes(b"the config"),
    })
}

#[test]
fn a_change_of_use_case_brief_role_order_rule_or_tool_misses() {
    let judge = ModelIdentity::new("anthropic", "claude-haiku-5-5", "1+fit1-jpeg90-1568");
    let pictures = [image(1), image(2), image(3)];
    let job = Job::shorthand("product-fidelity", false, 2, Some("A red box."));
    let base = key_of(&job, &pictures, &["view"], &judge);
    // The same job, made again, is the same key: the material is written one way.
    let again = Job::shorthand("product-fidelity", false, 2, Some("A red box."));
    assert_eq!(key_of(&again, &pictures, &["view"], &judge), base);
    assert_eq!(base.name().len(), 64);
    let mut changed: Vec<(&str, Key)> = Vec::new();
    // The use case.
    let other = Job::shorthand("reference-consistency", false, 2, Some("A red box."));
    changed.push(("use case", key_of(&other, &pictures, &["view"], &judge)));
    // The brief.
    let briefed = Job::shorthand("product-fidelity", false, 2, Some("A blue box."));
    changed.push(("brief", key_of(&briefed, &pictures, &["view"], &judge)));
    // An image's role: the same pictures in the same order, the candidate another.
    let mut swapped = job.clone();
    swapped.images[0].role = Role::Reference;
    swapped.images[1].role = Role::Candidate;
    changed.push(("role", key_of(&swapped, &pictures, &["view"], &judge)));
    // The order of the images.
    let reordered = [pictures[0], pictures[2], pictures[1]];
    changed.push(("order", key_of(&job, &reordered, &["view"], &judge)));
    // A picture's binding, the same identity: what is judged is the binding.
    let rebound = [pictures[0], pictures[1], (pictures[2].0, image(9).1)];
    changed.push(("binding", key_of(&job, &rebound, &["view"], &judge)));
    // A rule: a requirement of the job, which has the semantics of a rule.
    let mut ruled = job.clone();
    let rule = "[job]\nuse_case = \"product-fidelity\"\n\n[[job.requirements]]\nid = \"RQ1\"\n\
                kind = \"must_hold\"\nscope = [\"candidate\"]\ntext = \"The label reads RED.\"\n\n\
                [[job.images]]\nid = \"candidate\"\nrole = \"candidate\"\n";
    ruled.requirements = Job::from_toml(rule).unwrap().requirements;
    changed.push(("rule", key_of(&ruled, &pictures, &["view"], &judge)));
    // A tool: another set of tools, and the judge's identity.
    changed.push(("tools", key_of(&job, &pictures, &["view", "align"], &judge)));
    let newer = ModelIdentity::new("anthropic", "claude-haiku-5-5", "2+fit1-jpeg90-1568");
    changed.push(("judge", key_of(&job, &pictures, &["view"], &newer)));
    for (what, key) in &changed {
        assert_ne!(key, &base, "a change of the {what} hit");
        assert_ne!(key.name(), base.name(), "{what}");
    }
    // The cell of a matrix is not part of the key; the tools' revisions are.
    assert!(!base.material().contains("a cell of a matrix"));
    assert!(base.material().contains(&format!(
        "\"version\":\"{}\"",
        cache::tool_version("view").unwrap()
    )));
    // So are those of the tools that compare a pair (task X1): a run that measures one
    // first is keyed by squint's version of its code.
    let compared = key_of(&job, &pictures, &["view", "change_map"], &judge);
    assert!(compared.material().contains(&format!(
        "{{\"name\":\"change_map\",\"version\":\"{}\"}}",
        cache::tool_version("change_map").unwrap()
    )));
}

// --- through the door, under a scripted judge ---------------------------------------------------

/// The binary's runner over the world's store, `check` with its cache on or off.
fn runner(judged: &Judged, over: &whizbang::wiring::Over, cached: bool) -> Engine {
    let registry: Registry = whizbang::wiring::registry_with(cached).unwrap();
    let store = Arc::new(Store::open(&judged.world.root, Arc::new(RealFs)).unwrap());
    whizbang::wiring::runner(registry, store, over).unwrap()
}

/// A world whose config turns the cache on, its pair, and a judge on loopback that
/// answers `answers`: the world, the over, and the listener with its log.
fn cached_world(
    label: &str,
    answers: Vec<Vec<u8>>,
) -> (
    Judged,
    whizbang::wiring::Over,
    (String, String),
    whizbang_testkit::loopback::Listener,
) {
    let judged = Judged::new(label);
    let pictures = pair(&judged);
    let (listener, _) = serving(&judged.pki, answers);
    let origin = format!("https://127.0.0.1:{}", listener.port());
    let over = judged.over_with(
        &judged.address(listener.port()),
        Some(&origin),
        &CHECK_CLASSES,
        "[cache]\n",
    );
    (judged, over, pictures, listener)
}

/// `squint check source-fidelity` of the pair through the door: what it prints.
fn checked(executor: &Engine, judged: &Judged, (source, candidate): &(String, String)) -> String {
    let asked = CheckAsked {
        use_case: "source-fidelity",
        source: Some(source),
        candidate,
        references: &[],
        judge: None,
        brief: Some("Keep the band."),
        request_id: None,
        details: Details::None,
        detail: None,
        projection: "full",
    };
    let root = judged.world.root.clone();
    door::check(executor, &asked, &|run| door::verdict(&root, run)).unwrap()
}

/// The run that judged, of the run a door's first line names: the judge's step of a use
/// case's recipe (plan m3, task Q1), whose record holds the judge's turns.
fn judged_of(root: &std::path::Path, printed: &str) -> String {
    let run = whizbang_core::contract::RunId::parse(&printed_run(printed)).unwrap();
    whizbang::door::judged_run(root, &run).as_str().to_owned()
}

/// The run a door's first line names.
fn printed_run(printed: &str) -> String {
    let first = printed.lines().next().unwrap();
    first
        .strip_prefix("run ")
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .to_owned()
}

/// A setting of a run's record, by its name.
fn setting(record: &Json, name: &str) -> Option<Json> {
    let settings = record["asked"]["settings"].as_array()?;
    let found = settings.iter().find(|s| s["name"] == name)?;
    Some(found["value"].clone())
}

#[test]
fn an_unchanged_job_hits_and_records_a_run_naming_the_earlier_one() {
    // One answer only: a second request would find no server to answer it.
    let answers = vec![submission("held", "Nothing protected moved.")];
    let (judged, over, pictures, listener) = cached_world("cache-hit", answers);
    let executor = runner(&judged, &over, true);
    let first = checked(&executor, &judged, &pictures);
    let second = checked(&executor, &judged, &pictures);
    drop(executor);
    assert_eq!(listener.finish(), 1);
    let root = &judged.world.root;
    let (earlier, later) = (judged_of(root, &first), judged_of(root, &second));
    assert_ne!(earlier, later);
    // The same verdict and reasons, from a run of its own.
    let after = |printed: &str| {
        printed
            .lines()
            .skip(1)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(after(&first), after(&second));
    assert!(second.contains("verdict: pass"), "{second}");
    let root = &judged.world.root;
    let record = record_of(root, &earlier);
    assert_eq!(
        setting(&record, cache::SETTING),
        Some(json!({"text": "kept"})),
        "{record}"
    );
    let record = record_of(root, &later);
    assert_eq!(
        setting(&record, cache::SETTING),
        Some(json!({"text": "hit"})),
        "{record}"
    );
    assert_eq!(
        setting(&record, cache::ANSWERED_BY),
        Some(json!({"text": earlier}))
    );
    // The hit sent nothing and paid for nothing, and was admitted as any run is: its
    // reservation was taken, and settled at nothing.
    let judged_turns = record["judge"].as_array().unwrap();
    assert_eq!(judged_turns.len(), 1, "{record}");
    assert_eq!(judged_turns[0]["judge"], "verdict");
    assert_eq!(record["models"], json!([]));
    assert!(
        record["spend"]["reserved_micros"].as_u64().unwrap() > 0,
        "{record}"
    );
    assert_eq!(record["spend"]["requests"], json!([]));
    assert_eq!(record["egress"], json!([]));
}

#[test]
fn evaluation_never_reads_the_cache() {
    // Three answers: the run that keeps its key, then two that must ask again.
    let answers = vec![
        submission("held", "Kept."),
        submission("held", "Asked again."),
        submission("held", "And again."),
    ];
    let (judged, over, pictures, listener) = cached_world("cache-evaluation", answers);
    let caching = runner(&judged, &over, true);
    let kept = checked(&caching, &judged, &pictures);
    let root = &judged.world.root;
    assert_eq!(
        setting(&record_of(root, &judged_of(root, &kept)), cache::SETTING),
        Some(json!({"text": "kept"}))
    );
    // A run that names its cell of a matrix, as every run of an evaluation does, never
    // looks, under the same config and the same registry.
    let (source, candidate) = &pictures;
    let asked = CheckAsked {
        use_case: "source-fidelity",
        source: Some(source),
        candidate,
        references: &[],
        judge: None,
        brief: Some("Keep the band."),
        request_id: None,
        details: Details::None,
        detail: None,
        projection: "full",
    };
    let (job, images) = door::shorthand(&asked).unwrap();
    // An evaluation's cell is a call of `check` itself, as `whizbang eval` makes it.
    let mut call = Call::new(whizbang_squint::check::NAME)
        .argument("use_case", Value::text(&job.use_case))
        .argument("job", Value::text(&job.to_json()))
        .argument("tools", Value::text("view,align"))
        .argument(
            "max_output_tokens",
            Value::Integer(door::CHECK_OUTPUT_TOKENS),
        )
        .argument("cell", Value::text("{\"matrix\":\"m\",\"repetition\":1}"));
    for image in &images {
        call = call.input("images", *image);
    }
    let ran = caching.submit(&call).unwrap();
    assert_eq!(ran.outcome().name(), "completed");
    assert_eq!(
        setting(&record_of(root, ran.run().as_str()), cache::SETTING),
        None
    );
    // And the registry evaluation builds (`wiring::registry`) has the cache off,
    // whatever the config says.
    drop(caching);
    let evaluating = runner(&judged, &over, false);
    let again = checked(&evaluating, &judged, &pictures);
    assert_eq!(
        setting(&record_of(root, &judged_of(root, &again)), cache::SETTING),
        None
    );
    drop(evaluating);
    assert_eq!(listener.finish(), 3);
}

#[test]
fn a_backend_that_cannot_report_its_identity_is_not_cached() {
    // The provider's answers name no model.
    let unnamed = || {
        let answer = submission("held", "Nothing protected moved.");
        let text = String::from_utf8(answer).unwrap();
        let (_, body) = text.split_once("\r\n\r\n").unwrap();
        let mut body: Json = serde_json::from_str(body).unwrap();
        body.as_object_mut().unwrap().remove("model");
        let body = body.to_string();
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    };
    let (judged, over, pictures, listener) =
        cached_world("cache-unreported", vec![unnamed(), unnamed()]);
    let executor = runner(&judged, &over, true);
    let first = checked(&executor, &judged, &pictures);
    let second = checked(&executor, &judged, &pictures);
    drop(executor);
    // Both asked the judge: the first kept no key.
    assert_eq!(listener.finish(), 2);
    let record = record_of(&judged.world.root, &judged_of(&judged.world.root, &first));
    let said = "not kept: the judge did not report its identity";
    assert_eq!(
        setting(&record, cache::SETTING),
        Some(json!({"text": said})),
        "{record}"
    );
    let record = record_of(&judged.world.root, &judged_of(&judged.world.root, &second));
    assert_eq!(setting(&record, cache::ANSWERED_BY), None);
}

#[test]
fn a_tool_files_edit_misses() {
    use whizbang_squint::files::FileTools;
    // A tool file the run names is keyed by the digest of its bytes as they are now
    // (plan 2C, task X1): an edit misses, and the same bytes hit.
    let dir = whizbang_testkit::scratch::Scratch::new("cache-tool-file");
    let code = "def run(context):\n    return {'readings': []}\n";
    let text = crate::tool_file::tool_text("probe", crate::tool_file::BOUNDS, code);
    let path = crate::tool_file::write_tool(dir.path(), "probe.py", &text);
    let files =
        whizbang::tool_file::Files::read([path.as_path()], whizbang::pack::OneWorker::installed())
            .unwrap();
    let judge = ModelIdentity::new("anthropic", "claude-haiku-5-5", "1+fit1-jpeg90-1568");
    let pictures = [image(1), image(2)];
    let job = Job::shorthand("source-fidelity", true, 0, None);
    let tools = ["view", "probe"];
    let key = |files: &whizbang::tool_file::Files| {
        let revision = files.revision("probe").unwrap();
        key_with(
            &job,
            &pictures,
            &tools,
            &[("probe".to_owned(), revision)],
            &judge,
        )
    };
    let base = key(&files);
    assert_eq!(key(&files), base);
    assert!(
        base.material()
            .contains(&Digest::of_bytes(text.as_bytes()).to_string())
    );
    let edited = text.replace("[]", "[ ]");
    crate::tool_file::write_tool(dir.path(), "probe.py", &edited);
    assert_ne!(key(&files), base);
    crate::tool_file::write_tool(dir.path(), "probe.py", &text);
    assert_eq!(key(&files), base);
}
