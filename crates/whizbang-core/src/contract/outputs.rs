//! What is handed back is held to what the descriptor says of each output (plan m3, task
//! F1, item 5; ADR 0008, "What is handed back is held"), after it is held to the ports
//! (`engine::held_to_ports`) and before anything of it is published. Nothing of it is
//! left trusted:
//!
//! - **Within the decode limits:** each artifact is written as its canonical file and
//!   read back under the executor's limits.
//! - **Its origin names the action:** the last of its steps that is not a rebind names
//!   the action that handed it back. An artifact the store already holds is not made
//!   here: the store keeps the file it has (ADR 0008). It is taken as it is when it is
//!   an input of the call (handed back as it was for an unchanged result) or an output
//!   a run the call made committed; any other the store holds is taken only when the
//!   file the store keeps names this action, as a run made again does, so a record
//!   never lists another action's artifact as this one's output (plan m3, F1's second
//!   pull request, from the review of the first).
//! - **On its image** (`OutputPort::on`): an artifact other than an image belongs to the
//!   image of the port it names, an input of the call or an output.
//! - **Rebound** (`OutputPort::rebound_onto`): a mask is on the image it is rebound onto,
//!   and its binding places that image onto the image its port names, by the identity;
//!   or that image is the one its port names, unchanged, and the mask keeps its binding.
//! - **Derived** (`OutputPort::derived_from`): an image is the size of the one its port
//!   names, and is that picture, or differs from it only where a mask weighs over 0: the
//!   masks handed back rebound onto it, or else the masks the call gave on that image.
//!   With neither, it is that picture or it is refused.
//!
//! What comes back is why not, in fixed words with the port's name, never a path.

use crate::artifact::{self, Artifact, ArtifactId, ArtifactType, Contents, Mask};
use crate::identity::{Binding, CanonicalPixels, DecodeLimits, Destination, Mapping, PixelDigest};

use super::descriptor::{ActionDescriptor, OutputPort, PortRef};
use super::prepare::PreparedCall;
use super::run::Committed;

/// The name of the step a rebind adds to a mask's origin.
const REBIND: &str = "rebind";

/// An image a relation names: its pixel digest and its pixels.
struct Pictured {
    digest: PixelDigest,
    pixels: CanonicalPixels,
}

/// The binding an artifact other than an image belongs to.
fn binding_of(artifact: &Artifact) -> Option<&Binding> {
    match artifact {
        Artifact::Mask(mask) => Some(mask.binding()),
        Artifact::Location(location) => Some(location.binding()),
        Artifact::BandProfile(band) => Some(band.binding()),
        Artifact::Series(series) => Some(series.binding()),
        Artifact::Plan(plan) => Some(plan.binding()),
        Artifact::Image(_) => None,
    }
}

/// The action an artifact's origin names: the last of its steps that is not a rebind.
fn named(artifact: &Artifact) -> Option<&str> {
    let steps = artifact.origin().steps().iter().rev();
    let mut made = steps.filter(|step| step.action != REBIND);
    made.next().map(|step| step.action.as_str())
}

/// Holds what was handed back, each output at its port, to everything the descriptor
/// says of its port. `store` is the run's, which holds the call's inputs; `called`
/// says whether a run the call made committed an artifact of this identity.
pub(super) fn held_to_relations(
    prepared: &PreparedCall,
    descriptor: &ActionDescriptor,
    held: &[(Committed, Artifact)],
    store: &(dyn Contents + Send + Sync),
    limits: &DecodeLimits,
    called: &dyn Fn(ArtifactId) -> bool,
) -> Result<(), String> {
    let generation = store
        .snapshot()
        .map_err(|_| "the store could not be read to hold what was handed back".to_owned())?;
    for (output, artifact) in held {
        let port = output.port();
        if Artifact::from_bytes(&artifact.to_bytes(), limits).is_err() {
            return Err(format!(
                "the action handed back at {port} an artifact that is not read within the \
                 executor's decode limits"
            ));
        }
        let (kind, id) = (artifact.artifact_type(), artifact.id());
        let given = prepared.inputs().iter().any(|input| input.id() == id);
        if artifact::holds(&generation, kind, id) {
            if given || called(id) {
                continue;
            }
            // The file the store keeps is what the record lists.
            let kept = artifact::get(store, kind, id, limits).map_err(|_| {
                format!("the action handed back at {port} an artifact the store could not read")
            })?;
            if named(&kept) != Some(descriptor.name()) {
                return Err(format!(
                    "the action handed back at {port} an artifact the store holds already, \
                     made by another action, which the call neither gave nor made"
                ));
            }
        } else if named(artifact) != Some(descriptor.name()) {
            return Err(format!(
                "the action handed back at {port} an artifact whose origin does not name it"
            ));
        }
    }
    let image = |at: &PortRef| -> Result<Pictured, String> {
        let read = |artifact: &Artifact| match artifact {
            Artifact::Image(image) => image
                .pixels(limits)
                .map(|pixels| Pictured {
                    digest: image.identity().pixels,
                    pixels,
                })
                .map_err(|_| "is not read within the executor's decode limits".to_owned()),
            _ => Err("is not an image".to_owned()),
        };
        match at {
            PortRef::Output(name) => match held.iter().find(|(o, _)| o.port() == name) {
                Some((_, artifact)) => read(artifact),
                None => Err("was not handed back".to_owned()),
            },
            PortRef::Input(name) => {
                let Some(input) = prepared.inputs().iter().find(|i| i.port() == name) else {
                    return Err("was not given".to_owned());
                };
                let got = artifact::get(store, ArtifactType::Image, input.id(), limits)
                    .map_err(|_| "could not be read".to_owned())?;
                read(&got)
            }
        }
    };
    for port in descriptor.outputs() {
        let at_port = held.iter().filter(|(o, _)| o.port() == port.name());
        for (_, artifact) in at_port {
            held_to_port(
                port, artifact, &image, prepared, descriptor, held, store, limits,
            )?;
        }
    }
    Ok(())
}

/// One output held to what its port says.
#[allow(clippy::too_many_arguments)]
fn held_to_port(
    port: &OutputPort,
    artifact: &Artifact,
    image: &dyn Fn(&PortRef) -> Result<Pictured, String>,
    prepared: &PreparedCall,
    descriptor: &ActionDescriptor,
    held: &[(Committed, Artifact)],
    store: &(dyn Contents + Send + Sync),
    limits: &DecodeLimits,
) -> Result<(), String> {
    let name = port.name();
    let said = |what: &str, why: String| format!("the output {name} {what}: {why}");
    if let (Some(on), Some(binding)) = (port.belongs_to(), binding_of(artifact)) {
        let on_image = image(on).map_err(|why| said("belongs to an image that", why))?;
        match port.rebinds_onto() {
            Some(onto) => {
                let onto = image(&PortRef::Output(onto.to_owned()))
                    .map_err(|why| said("is rebound onto an image that", why))?;
                if binding.source() != onto.digest {
                    return Err(said(
                        "is not on the image it is rebound onto",
                        format!("it is on {}", binding.source()),
                    ));
                }
                let placed = binding.destination() == Destination::Image(on_image.digest)
                    && *binding.mapping() == Mapping::Identity;
                if onto.digest != on_image.digest && !placed {
                    return Err(said(
                        "was not rebound",
                        format!("its binding does not place its image onto {on}"),
                    ));
                }
            }
            None if binding.source() != on_image.digest => {
                return Err(said(
                    "is on another image than its port names",
                    format!(
                        "it is on {}, and {on} is {}",
                        binding.source(),
                        on_image.digest
                    ),
                ));
            }
            None => {}
        }
    }
    if let (Some(parent), Artifact::Image(_)) = (port.parent(), artifact) {
        let parent_image =
            image(parent).map_err(|why| said("is derived from an image that", why))?;
        let derived = image(&PortRef::Output(name.to_owned()))?;
        let size = |p: &CanonicalPixels| (p.width(), p.height());
        if size(&derived.pixels) != size(&parent_image.pixels) {
            return Err(said(
                "is not derived from the image its port names",
                "it is of another size".to_owned(),
            ));
        }
        if derived.digest == parent_image.digest {
            return Ok(());
        }
        // The masks that say where it may differ: those handed back rebound onto it, or
        // else those the call gave on the image it is derived from.
        let mut masks: Vec<Mask> = descriptor
            .outputs()
            .iter()
            .filter(|other| other.rebinds_onto() == Some(name))
            .flat_map(|other| held.iter().filter(move |(o, _)| o.port() == other.name()))
            .filter_map(|(_, artifact)| match artifact {
                Artifact::Mask(mask) => Some(mask.clone()),
                _ => None,
            })
            .collect();
        if masks.is_empty() {
            for input in prepared.inputs() {
                if input.artifact() != ArtifactType::Mask {
                    continue;
                }
                let mask = artifact::get(store, ArtifactType::Mask, input.id(), limits)
                    .and_then(Artifact::into_mask)
                    .map_err(|_| {
                        said(
                            "could not be held",
                            "a mask the call gave could not be read".to_owned(),
                        )
                    })?;
                if mask.image() == parent_image.digest {
                    masks.push(mask);
                }
            }
        }
        // A mask of another size says nothing of these pixels.
        let count = parent_image.pixels.rgba().len() / 4;
        masks.retain(|mask| mask.core().len() == count);
        let weighs = |mask: &Mask, at: usize| {
            mask.protect()[at] == 0 && (mask.core()[at] != 0 || mask.blend()[at] != 0)
        };
        let pixels = parent_image
            .pixels
            .rgba()
            .chunks(4)
            .zip(derived.pixels.rgba().chunks(4));
        for (at, (before, after)) in pixels.enumerate() {
            if before != after && !masks.iter().any(|mask| weighs(mask, at)) {
                return Err(said(
                    "is not derived from the image its port names",
                    "it differs where no mask says it may".to_owned(),
                ));
            }
        }
    }
    Ok(())
}
