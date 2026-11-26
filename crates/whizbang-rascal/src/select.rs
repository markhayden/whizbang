//! `select`: the masks a segmenter found for the instances to keep, as one mask: the union
//! of their cores, minus any exclude boxes (plan 2A, task R2).
//!
//! Ported from rascal `crates/rascal-ops/src/select.rs` and the box of `rasterize.rs`
//! (`bbox_pixels`), at the commit `docs/sources.toml` pins, with three differences, each
//! on purpose:
//!
//! 1. **The kept instances are the masks given.** Rascal took every instance plane at a
//!    port of many and chose among them by `keep` and `drop` indices. Here `segment`
//!    commits one mask for each instance, so the call names the ones it keeps, and
//!    there is nothing to index.
//! 2. **A box is in pixels of the image,** as `segment` prints it (left, top, right,
//!    bottom), and not in fractions of the frame. A pixel is excluded when its centre
//!    is at or right of the left edge and left of the right one, and at or under the top
//!    edge and above the bottom one: rascal's rule, compared exactly.
//! 3. **No pixel left is a refusal of the call** (the run ends as invalid), where rascal
//!    said its op did not apply.
//!
//! **A pixel to keep by** (plan 1D, task Q1): given `at`, `X Y` in pixels of the image, only
//! the masks whose core holds the pixel there are kept, so one instance of the several a
//! request answered is picked by pointing at it ([`holding`]). None that holds it refuses
//! the call, naming how many masks were given.
//!
//! The new mask is on the masks' one binding, with nothing protected and no blend: feather
//! it afterwards. Its source is every instance and location the masks name, once each, in
//! the order given; its origin is one step, this action, its arguments and every mask.
//!
//! **One mask at a time** (plan mE, task L2, item 3): a run reads the masks one after the
//! other into a [`Union`], which holds the union's core and what the origin and the source
//! need of each, and lets each mask go before it reads the next. So a run holds one mask
//! and the union, where it held every mask it was given at once: with the 33 a located
//! retry may give it, on a picture of 12 megapixels, about 1.2 GB.

use whizbang_core::artifact::{ArtifactError, ArtifactType, Mask, Origin, Source, Step, UseCase};
use whizbang_core::contract::{
    Action, ActionDescriptor, Argument, Context, Count, Failure, InputPort, Kind, OutputPort,
    PreparedCall, Produced, Value, ValueType,
};

use crate::operation;

/// The action's name.
pub const NAME: &str = "select";

/// The action's version, which a mask it made records in its origin.
pub const VERSION: &str = "1.0.0";

/// The most masks one call unions: the most instances `segment` finds.
pub const MASKS_MAX: u32 = 64;

/// The use cases a call may record, by name.
pub(crate) const USE_CASES: [(&str, UseCase); 5] = [
    ("seam", UseCase::Seam),
    ("remove", UseCase::Remove),
    ("repair", UseCase::Repair),
    ("protect", UseCase::Protect),
    ("custom", UseCase::Custom),
];

/// A box to clear from the core: left, top, right, bottom, in pixels of the image.
pub type Exclude = [f64; 4];

/// Reads the boxes of the argument `exclude`: boxes apart by `;`, each four numbers
/// apart by spaces or commas. An empty text is no box.
pub fn boxes(text: &str) -> Result<Vec<Exclude>, String> {
    let mut boxes = Vec::new();
    for part in text.split(';').filter(|part| !part.trim().is_empty()) {
        let numbers: Vec<&str> = part
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|word| !word.is_empty())
            .collect();
        let read: Option<Vec<f64>> = numbers
            .iter()
            .map(|word| word.parse::<f64>().ok().filter(|n| n.is_finite()))
            .collect();
        match read.as_deref() {
            Some(&[left, top, right, bottom]) => boxes.push([left, top, right, bottom]),
            _ => {
                return Err(
                    "an exclude box is four finite numbers: left, top, right and bottom".to_owned(),
                );
            }
        }
    }
    Ok(boxes)
}

/// The pixels, counted from 0, whose centres are at or past `low` and before `high`.
fn centres_within(low: f64, high: f64, size: u32) -> impl Iterator<Item = usize> {
    (0..size)
        .filter(move |at| {
            let centre = f64::from(*at) + 0.5;
            centre >= low && centre < high
        })
        .map(|at| at as usize)
}

/// Reads the argument `at`: two finite numbers, X and Y, apart by spaces or commas.
pub fn point(text: &str) -> Result<[f64; 2], String> {
    let read: Option<Vec<f64>> = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| word.parse::<f64>().ok().filter(|n| n.is_finite()))
        .collect();
    match read.as_deref() {
        Some(&[x, y]) => Ok([x, y]),
        _ => Err("at is two finite numbers: X and Y, in pixels of the image".to_owned()),
    }
}

/// The masks whose core holds the pixel at `[x, y]`, in the order given: the pixel
/// whose left edge is at or under `x` and whose right edge is past it, and the same of
/// `y`. A place outside the image, or one that no mask holds, is refused, naming how many
/// masks there were.
pub fn holding(masks: Vec<Mask>, [x, y]: [f64; 2]) -> Result<Vec<Mask>, SelectError> {
    let count = masks.len();
    let Some(first) = masks.first() else {
        return Err(SelectError::Invalid(
            "select takes at least one mask".to_owned(),
        ));
    };
    let space = first.binding().source_space();
    let (width, height) = (f64::from(space.width()), f64::from(space.height()));
    if !(x >= 0.0 && y >= 0.0 && x < width && y < height) {
        return Err(SelectError::Invalid(format!(
            "the pixel at {} {} is outside the image of the {count} masks, {width} by {height}",
            crate::grow::text_of(x),
            crate::grow::text_of(y)
        )));
    }
    let at = y.floor() as usize * space.width() as usize + x.floor() as usize;
    let kept: Vec<Mask> = masks
        .into_iter()
        .filter(|mask| mask.core().get(at).is_some_and(|sample| *sample != 0))
        .collect();
    if kept.is_empty() {
        return Err(SelectError::Invalid(format!(
            "none of the {count} masks holds the pixel at {} {}",
            crate::grow::text_of(x),
            crate::grow::text_of(y)
        )));
    }
    Ok(kept)
}

/// The union of masks given one at a time: what [`select`] makes of them, without
/// holding more than the one being added.
pub struct Union {
    binding: Option<whizbang_core::identity::Binding>,
    first: Option<whizbang_core::identity::PixelDigest>,
    core: Vec<u8>,
    source: Source,
    ids: Vec<whizbang_core::identity::Digest>,
    /// The pixel a mask must hold to be kept, as an index into the core, when one is
    /// asked ([`holding`]); and how many masks were given.
    at: Option<[f64; 2]>,
    given: usize,
    /// The image's size, when the pixel to keep by is outside it.
    outside: Option<(f64, f64)>,
    /// The space of the first mask given, which the pixel to keep by is read in.
    first_space: Option<whizbang_core::identity::Space>,
}

impl Union {
    /// An empty union; with `at`, of only the masks whose core holds that pixel.
    pub fn new(at: Option<[f64; 2]>) -> Union {
        Union {
            binding: None,
            first: None,
            core: Vec::new(),
            source: Source::default(),
            ids: Vec::new(),
            at,
            given: 0,
            outside: None,
            first_space: None,
        }
    }

    /// Adds one mask, which may then be let go. Refused: a mask on another image than
    /// the first's. A pixel to keep by that is outside the image is refused when the
    /// union is finished, naming how many masks were given.
    pub fn add(&mut self, mask: &Mask) -> Result<(), SelectError> {
        self.given += 1;
        // Kept by a pixel first, as `holding` keeps them: the pixel's place in the
        // image of the first mask given, and only a mask that holds it is joined, or
        // compared with the others' image.
        if let Some([x, y]) = self.at {
            let space = *self
                .first_space
                .get_or_insert_with(|| mask.binding().source_space());
            let (width, height) = (f64::from(space.width()), f64::from(space.height()));
            if !(x >= 0.0 && y >= 0.0 && x < width && y < height) {
                self.outside = Some((width, height));
                return Ok(());
            }
            let at = y.floor() as usize * space.width() as usize + x.floor() as usize;
            if mask.core().get(at).is_none_or(|sample| *sample == 0) {
                return Ok(());
            }
        }
        match &self.binding {
            None => {
                self.binding = Some(mask.binding().clone());
                self.first = Some(mask.image());
                self.core = vec![0u8; mask.core().len()];
            }
            Some(binding) if binding != mask.binding() => {
                return Err(SelectError::Invalid(format!(
                    "the masks are on two images: {} and {}",
                    self.first.map(|d| d.to_string()).unwrap_or_default(),
                    mask.image()
                )));
            }
            Some(_) => {}
        }
        for (out, sample) in self.core.iter_mut().zip(mask.core()) {
            if *sample != 0 {
                *out = 255;
            }
        }
        for location in &mask.source().locations {
            if !self.source.locations.contains(location) {
                self.source.locations.push(*location);
            }
        }
        for instance in &mask.source().instances {
            if !self.source.instances.contains(instance) {
                self.source.instances.push(*instance);
            }
        }
        self.ids.push(mask.id().0);
        Ok(())
    }

    /// The selection, every box of `exclude` cleared: refused when no mask was given,
    /// when none held the pixel to keep by, and when no pixel is left.
    pub fn finish(mut self, exclude: &[Exclude], use_case: UseCase) -> Result<Mask, SelectError> {
        if self.given == 0 {
            return Err(SelectError::Invalid(
                "select takes at least one mask".to_owned(),
            ));
        }
        if let (Some((width, height)), Some([x, y])) = (self.outside, self.at) {
            return Err(SelectError::Invalid(format!(
                "the pixel at {} {} is outside the image of the {} masks, {width} by {height}",
                crate::grow::text_of(x),
                crate::grow::text_of(y),
                self.given
            )));
        }
        let Some(binding) = self.binding.take() else {
            let [x, y] = self.at.unwrap_or_default();
            return Err(SelectError::Invalid(format!(
                "none of the {} masks holds the pixel at {} {}",
                self.given,
                crate::grow::text_of(x),
                crate::grow::text_of(y)
            )));
        };
        let mut core = self.core;
        let space = binding.source_space();
        let width = space.width() as usize;
        for [left, top, right, bottom] in exclude {
            let columns: Vec<usize> = centres_within(*left, *right, space.width()).collect();
            for y in centres_within(*top, *bottom, space.height()) {
                for x in &columns {
                    core[y * width + x] = 0;
                }
            }
        }
        if operation::set(&core) == 0 {
            return Err(SelectError::Invalid(format!(
                "no pixel is left of the {} masks after the {} exclude boxes",
                self.ids.len(),
                exclude.len()
            )));
        }
        let name = USE_CASES.iter().find(|(_, case)| *case == use_case);
        let mut step = Step::new(NAME, VERSION)
            .argument("exclude", &exclude_text(exclude))
            .argument("use_case", name.map_or("custom", |(name, _)| name));
        for id in &self.ids {
            step = step.input("mask", *id);
        }
        let origin = Origin::new(step).map_err(SelectError::Mask)?;
        Mask::from_core(binding, use_case, core, self.source, origin).map_err(SelectError::Mask)
    }
}

/// Why a selection was not made.
#[derive(Debug)]
pub enum SelectError {
    /// The call cannot be worked on: no mask, masks on two images, or nothing left.
    Invalid(String),
    /// The mask could not be made.
    Mask(ArtifactError),
}

/// The selection: one mask whose core is every pixel set in a core of `masks`, cleared
/// inside every box of `exclude`.
pub fn select(masks: &[Mask], exclude: &[Exclude], use_case: UseCase) -> Result<Mask, SelectError> {
    let mut union = Union::new(None);
    for mask in masks {
        union.add(mask)?;
    }
    union.finish(exclude, use_case)
}

/// The boxes as an origin writes them: each number the shortest decimal that reads back
/// as the same double.
fn exclude_text(exclude: &[Exclude]) -> String {
    let each = exclude.iter().map(|edges| {
        let edges: Vec<String> = edges.iter().map(|e| crate::grow::text_of(*e)).collect();
        edges.join(" ")
    });
    each.collect::<Vec<_>>().join(";")
}

/// The action `select`.
pub struct Select {
    descriptor: ActionDescriptor,
}

impl Select {
    pub fn new() -> Select {
        let names: Vec<&str> = USE_CASES.iter().map(|(name, _)| *name).collect();
        let draft = ActionDescriptor::draft(
            NAME,
            VERSION,
            Kind::Primitive,
            "One mask from the instance masks to keep: the union of their cores, minus exclude boxes.",
        )
        .argument(Argument::optional(
            "exclude",
            "Boxes cleared from the core, in pixels: left top right bottom, boxes apart by ;.",
            ValueType::Text,
        ))
        .argument(Argument::optional(
            "at",
            "Keeps only the masks whose core holds the pixel at X Y, in pixels of the image.",
            ValueType::Text,
        ))
        .argument(Argument::with_default(
            "use_case",
            "What the mask is for, recorded on it.",
            ValueType::choice(&names),
            Value::text("remove"),
        ))
        .input(InputPort::variadic(
            "mask",
            ArtifactType::Mask,
            "The instance masks to keep, all on one image.",
            Count::between(1, MASKS_MAX),
        ))
        .output(OutputPort::one(
            "mask",
            ArtifactType::Mask,
            "The selection: the union of the cores, no blend and nothing protected.",
        ))
        .concurrent(true);
        match draft.build() {
            Ok(descriptor) => Select { descriptor },
            // The draft is written above, in full, and a test of this crate builds it.
            Err(error) => panic!("{error}"),
        }
    }
}

impl Default for Select {
    fn default() -> Select {
        Select::new()
    }
}

impl Action for Select {
    fn descriptor(&self) -> &ActionDescriptor {
        &self.descriptor
    }

    fn run(&self, call: &PreparedCall, context: &Context) -> Result<Produced, Failure> {
        let exclude = boxes(operation::text(call, "exclude").unwrap_or(""))
            .map_err(|why| Failure::invalid(&why))?;
        let named = operation::text(call, "use_case").unwrap_or("remove");
        let use_case = USE_CASES.iter().find(|(name, _)| *name == named);
        let use_case = use_case.map_or(UseCase::Remove, |(_, case)| *case);
        let failed = |error| match error {
            SelectError::Invalid(why) => Failure::invalid(&why),
            SelectError::Mask(error) => Failure::error("mask", &error.without_paths()),
        };
        let at = match operation::text(call, "at") {
            Some(at) => Some(point(at).map_err(|why| Failure::invalid(&why))?),
            None => None,
        };
        // One mask at a time: each is read, added to the union, and let go.
        let mut union = Union::new(at);
        for id in operation::given(call, "mask") {
            union.add(&operation::mask(context, id)?).map_err(failed)?;
            if operation::stopped(context) {
                return Err(operation::stopped_failure(context, NAME));
            }
        }
        let kept = union.ids.len();
        let mask = union.finish(&exclude, use_case).map_err(failed)?;
        context.events().progress(&format!(
            "select: a core of {} pixels from {kept} masks",
            operation::set(mask.core())
        ));
        Ok(Produced::nothing().with("mask", mask))
    }
}

/// Registers the action.
pub fn register(
    registry: &mut whizbang_core::contract::Registry,
) -> Result<(), whizbang_core::contract::RegistrationError> {
    registry.register(std::sync::Arc::new(Select::new()))
}
