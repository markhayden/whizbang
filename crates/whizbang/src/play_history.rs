//! The playground's history, its figures, and a run's report and archive (plan m5, task
//! W3, items 1 to 5): what [`Playground`] answers the host's [`Shelf`] with. History and
//! the figures choose their runs by [`stats::Filter`], so the figures the page shows are
//! the ones `runs stats` prints for the same filter ([`stats::Figures`]).

use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value as Json, json};
use whizbang_core::artifact::ArtifactType;
use whizbang_core::contract::RunId;
use whizbang_core::contract::record::{ArgumentValue, RunRecord};
use whizbang_serve::escaped;
use whizbang_serve::play::Refused;
use whizbang_serve::play::history::{Asked, Document, Shelf};

use crate::playing::Playground;
use crate::stats::{self, Figures, Filter, Loaded, Lookups, Runs};

/// The runs of the store's current generation, as the playground keeps them: read whole
/// once, and again only when the store has published since; and how many records were
/// read so, for a test.
#[derive(Default)]
pub struct Shelved {
    loaded: std::sync::Mutex<Option<Arc<Loaded>>>,
    records_read: std::sync::atomic::AtomicU64,
}

/// The filter the page asked for, as `runs stats` takes it: the page's `task` is the
/// action it names.
pub fn filter_of(asked: &Map<String, Json>) -> Result<Filter, Refused> {
    let field = |name: &str| asked.get(name).and_then(Json::as_str).map(str::to_owned);
    let action = match (field("action"), field("task")) {
        (Some(action), Some(task)) if action != task => {
            return Err(Refused::new(
                400,
                "task and action name the same thing: give one",
            ));
        }
        (action, task) => action.or(task),
    };
    Ok(Filter {
        action,
        model: field("model"),
        backend: field("backend"),
        use_case: field("use_case"),
        matrix: field("matrix"),
        since: field("since"),
        until: field("until"),
        kit: field("kit"),
        result: field("result"),
        words: field("words"),
    })
}

/// Millionths of a US dollar as dollars, for the page.
#[allow(clippy::cast_precision_loss)]
fn dollars(micros: u128) -> f64 {
    micros as f64 / 1_000_000.0
}

/// One row of history: the run, its task, what was asked (its words and its pictures'
/// names), its result, the models that served it, its time, what it and the runs under
/// it settled, when it started, and a picture to show beside it.
fn row(runs: &Runs, id: &str, record: &RunRecord, titles: &dyn Fn(&str) -> Option<String>) -> Json {
    let asked = record.asked();
    let action = asked.action.clone().unwrap_or_default();
    let words: Vec<String> = (asked.arguments.iter())
        .filter_map(|argument| match &argument.value {
            ArgumentValue::Text(text) if argument.name != "job" => Some(escaped(text)),
            _ => None,
        })
        .collect();
    let pictures: Vec<String> = runs
        .lookups
        .names_of(record)
        .iter()
        .map(|n| escaped(n))
        .collect();
    let mut models: Vec<String> = (record.models().iter())
        .chain(runs.children_of(id).flat_map(|child| child.models().iter()))
        .map(|model| escaped(&model.model))
        .collect();
    models.sort();
    models.dedup();
    let time = record.time();
    let thumb = (record.inputs().iter())
        .find(|input| input.artifact_type == ArtifactType::Image)
        .map(|input| input.artifact.to_string());
    json!({
        "run": id,
        "action": escaped(&action),
        "kit": runs.lookups.kits.get(&action),
        "title": titles(&action),
        "words": words,
        "pictures": pictures,
        "result": stats::result_of(record),
        "models": models,
        "time_ms": time.start_ms.map(|start| time.end_ms.saturating_sub(start)),
        "cost": dollars(runs.settled_under(id)),
        "at_ms": time.start_ms,
        "thumb": thumb,
    })
}

/// One page of history: the runs the filter admits that nothing nested (a run's steps
/// are its own), newest first, after the run `before` names when it names one, at most
/// `limit`; and whether there are more.
pub fn listed(
    loaded: Arc<Loaded>,
    asked: &Asked,
    lookups: Lookups,
    titles: &dyn Fn(&str) -> Option<String>,
) -> Result<Json, Refused> {
    let filter = filter_of(&asked.filter)?;
    let runs = Runs::within(loaded, &filter, lookups).map_err(|why| Refused::new(400, &why))?;
    let mut top: Vec<&(String, RunRecord)> = (runs.found.iter())
        .filter(|(_, record)| record.identity().parent.is_none())
        .collect();
    top.sort_by(|a, b| {
        let at = |r: &RunRecord| r.time().start_ms.unwrap_or(0);
        at(&b.1).cmp(&at(&a.1)).then_with(|| b.0.cmp(&a.0))
    });
    let start = match &asked.before {
        Some(before) => match top.iter().position(|(id, _)| id == before) {
            Some(at) => at + 1,
            None => return Err(Refused::new(400, "before names no run of this list")),
        },
        None => 0,
    };
    let page: Vec<Json> = (top.iter().skip(start).take(asked.limit))
        .map(|(id, record)| row(&runs, id, record, titles))
        .collect();
    let more = top.len() > start + page.len();
    Ok(json!({"runs": page, "more": more}))
}

/// The figures of `runs stats` over the filter the page asked for.
pub fn figures(loaded: Arc<Loaded>, asked: &Asked, lookups: Lookups) -> Result<Json, Refused> {
    let filter = filter_of(&asked.filter)?;
    let runs = Runs::within(loaded, &filter, lookups).map_err(|why| Refused::new(400, &why))?;
    let figures = Figures::of(&runs);
    let mut said = serde_json::to_value(&figures).unwrap_or(Json::Null);
    said["text"] = json!(figures.text());
    Ok(said)
}

/// A file name of letters, digits and `-` from a task's name and a run's id.
fn file_name(action: &str, run: &RunId, extension: &str) -> String {
    let plain: String = (action.chars())
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .take(64)
        .collect();
    format!("{plain}-{}.{extension}", run.as_str())
}

impl Shelf for Playground {
    fn runs(&self, asked: &Asked) -> Result<Json, Refused> {
        listed(self.loaded()?, asked, self.lookups(), &|action| {
            self.title_of(action)
        })
    }

    fn stats(&self, asked: &Asked) -> Result<Json, Refused> {
        figures(self.loaded()?, asked, self.lookups())
    }

    fn report(&self, run: &RunId) -> Result<Document, Refused> {
        let root = self.root_of()?;
        let action = self.action_of(&root, run)?;
        let html = crate::report::report(&root, run, self.title_of(&action).as_deref())
            .map_err(|why| Refused::new(404, &why))?;
        Ok(Document {
            name: file_name(&action, run, "html"),
            content_type: "text/html; charset=utf-8",
            bytes: html.into_bytes(),
        })
    }

    fn export(&self, run: &RunId) -> Result<Document, Refused> {
        let root = self.root_of()?;
        let action = self.action_of(&root, run)?;
        let bytes = crate::report::archive(&root, run, self.title_of(&action).as_deref())
            .map_err(|why| Refused::new(404, &why))?;
        Ok(Document {
            name: file_name(&action, run, "tar"),
            content_type: "application/x-tar",
            bytes,
        })
    }
}

impl Playground {
    /// The runs of the store's current generation: those read before when the store has
    /// not published since, else read again.
    fn loaded(&self) -> Result<Arc<Loaded>, Refused> {
        let root = self.root_of()?;
        let busy = || Refused::new(500, "the store could not answer");
        let reader =
            whizbang_core::store::Reader::open(&root, Arc::new(whizbang_core::store::fs::RealFs))
                .map_err(|_| busy())?;
        let number = reader.snapshot().map_err(|_| busy())?.number();
        let kept = &self.shelved;
        let mut held = kept
            .loaded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(loaded) = held.as_ref()
            && loaded.number == number
        {
            return Ok(loaded.clone());
        }
        let loaded = Arc::new(Loaded::load(&root).map_err(|why| Refused::new(500, &why))?);
        kept.records_read
            .fetch_add(loaded.all.len() as u64, std::sync::atomic::Ordering::SeqCst);
        *held = Some(loaded.clone());
        Ok(loaded)
    }

    /// How many run records history and the figures have read from the store: for a test.
    #[doc(hidden)]
    pub fn records_read(&self) -> u64 {
        self.shelved
            .records_read
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn root_of(&self) -> Result<std::path::PathBuf, Refused> {
        self.root()
            .map(Path::to_path_buf)
            .ok_or_else(|| Refused::new(503, "the playground keeps no store"))
    }

    /// The kits of the actions the playground offers.
    fn lookups(&self) -> Lookups {
        let mut lookups = Lookups::default();
        for descriptor in self.descriptors() {
            if let Some(kit) = descriptor.kit() {
                lookups
                    .kits
                    .insert(descriptor.name().to_owned(), kit.to_owned());
            }
        }
        lookups
    }

    fn title_of(&self, action: &str) -> Option<String> {
        (self.descriptors().iter())
            .find(|d| d.name() == action)
            .and_then(|d| d.title().map(str::to_owned))
    }

    /// The action of a run, from its record: 404 when the store holds none.
    fn action_of(&self, root: &Path, run: &RunId) -> Result<String, Refused> {
        let none = || Refused::new(404, "the store holds no record of that run");
        let reader = whizbang_core::store::Reader::open(
            root,
            std::sync::Arc::new(whizbang_core::store::fs::RealFs),
        )
        .map_err(|_| Refused::new(500, "the store could not answer"))?;
        let generation = reader
            .snapshot()
            .map_err(|_| Refused::new(500, "the store could not answer"))?;
        let stored =
            whizbang_core::contract::record::stored(&generation, run.as_str(), |e| reader.read(e));
        let record = stored.ok_or_else(none)?.map_err(|_| none())?;
        Ok(record.asked().action.clone().unwrap_or_default())
    }
}
