//! The six types: what each is made of, what each refuses, and what its identity depends
//! on.

use whizbang_core::artifact::{
    Artifact, ArtifactError, ArtifactType, BandProfile, Image, ImageRole, Line, Location, Origin,
    Plan, Series, Shape, Step, Windows,
};
use whizbang_core::identity::{
    Binding, Direction, IdentityError, Mapping, Point, Projection, Space, Topology, identify,
};

use super::support::*;

fn invalid<T: std::fmt::Debug>(result: Result<T, ArtifactError>, artifact: ArtifactType) -> bool {
    matches!(&result, Err(ArtifactError::Invalid { artifact: found, .. }) if *found == artifact)
}

#[test]
fn there_are_six_types_and_each_artifact_knows_its_own() {
    let names: Vec<&str> = ArtifactType::ALL.iter().map(|t| t.name()).collect();
    assert_eq!(
        names,
        [
            "mask",
            "location",
            "image",
            "band_profile",
            "series",
            "plan"
        ]
    );
    let artifacts = one_of_each();
    let types: Vec<ArtifactType> = artifacts.iter().map(Artifact::artifact_type).collect();
    assert_eq!(types, ArtifactType::ALL);
    // Six different identities, and each artifact says what produced it.
    let mut ids: Vec<_> = artifacts.iter().map(Artifact::id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 6);
    for artifact in &artifacts {
        assert!(!artifact.origin().steps().is_empty());
    }
}

#[test]
fn an_artifact_always_records_what_produced_it() {
    // There is no origin without a step, and no step without an action.
    assert!(matches!(
        Origin::new(Step::new("", "1")),
        Err(ArtifactError::Format(_))
    ));
    let origin = made_by("find")
        .then(Step::new("grow", "2").argument("by", "3"))
        .unwrap();
    assert_eq!(origin.steps().len(), 2);
    assert_eq!(origin.steps()[1].arguments, [("by".into(), "3".into())]);
    assert!(origin.then(Step::new("", "1")).is_err());
}

#[test]
fn the_origin_is_not_part_of_an_identity_and_the_content_is() {
    let binding = on("photo");
    let shape = Shape::Box {
        left: 1.0,
        top: 1.0,
        right: 4.0,
        bottom: 3.0,
    };
    let location = Location::new(binding.clone(), shape.clone(), made_by("find")).unwrap();
    // The same content from another action is the same artifact.
    let again = Location::new(
        binding.clone(),
        shape.clone(),
        Origin::new(
            Step::new("draw", "7")
                .argument("by", "hand")
                .input("image", picture("photo").0),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(location.id(), again.id());
    assert_ne!(location.to_bytes(), again.to_bytes());
    // The same shape on another image of the same size is another artifact.
    let elsewhere = Location::new(on("another photo"), shape, made_by("find")).unwrap();
    assert_ne!(location.id(), elsewhere.id());
    // Another shape on the same image is another artifact.
    let other = Location::new(binding, Shape::WholeImage, made_by("find")).unwrap();
    assert_ne!(location.id(), other.id());
}

#[test]
fn a_location_holds_each_shape_inside_its_image() {
    let binding = on("photo");
    let path = vec![Point::new(1.0, 1.0), Point::new(7.5, 5.0)];
    for shape in [
        Shape::Box {
            left: 0.0,
            top: 0.0,
            right: 8.0,
            bottom: 6.0,
        },
        Shape::Polygon {
            points: vec![
                Point::new(1.0, 1.0),
                Point::new(6.0, 1.5),
                Point::new(3.0, 5.0),
            ],
        },
        Shape::WholeImage,
        Shape::Absent,
        Shape::Line {
            line: Line::Column(8.0),
            width: 2.0,
        },
        Shape::Line {
            line: Line::Path(path.clone()),
            width: 10.0,
        },
        Shape::Stroke {
            path: path.clone(),
            radius: 0.5,
        },
    ] {
        let location = Location::new(binding.clone(), shape.clone(), made_by("find"))
            .unwrap_or_else(|error| panic!("{shape:?}: {error}"));
        assert_eq!(*location.shape(), shape);
        assert_eq!(*location.binding(), binding);
    }
    let many = (0..65)
        .map(|n| Point::new(f64::from(n) / 10.0, 1.0))
        .collect();
    for shape in [
        // No area, out of order, outside, and not finite.
        Shape::Box {
            left: 2.0,
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
        },
        Shape::Box {
            left: 5.0,
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
        },
        Shape::Box {
            left: 2.0,
            top: 1.0,
            right: 8.5,
            bottom: 3.0,
        },
        Shape::Box {
            left: f64::NAN,
            top: 1.0,
            right: 4.0,
            bottom: 3.0,
        },
        Shape::Polygon {
            points: path.clone(),
        },
        Shape::Polygon { points: many },
        Shape::Polygon {
            points: vec![
                Point::new(1.0, 1.0),
                Point::new(6.0, 1.5),
                Point::new(3.0, 6.5),
            ],
        },
        // A view needs a panorama, and this image is flat.
        Shape::View {
            yaw: 0.0,
            pitch: 0.0,
            fov: 60.0,
        },
        Shape::Line {
            line: Line::Column(8.5),
            width: 2.0,
        },
        Shape::Line {
            line: Line::Column(4.0),
            width: 0.0,
        },
        Shape::Line {
            line: Line::Column(4.0),
            width: 10.5,
        },
        Shape::Line {
            line: Line::Path(vec![Point::new(1.0, 1.0)]),
            width: 2.0,
        },
        Shape::Stroke {
            path: path.clone(),
            radius: f64::INFINITY,
        },
        Shape::Stroke {
            path: vec![Point::new(1.0, 1.0), Point::new(-0.5, 2.0)],
            radius: 1.0,
        },
    ] {
        assert!(
            invalid(
                Location::new(binding.clone(), shape.clone(), made_by("find")),
                ArtifactType::Location
            ),
            "{shape:?}"
        );
    }
    // A view on a complete panorama, inside its ranges and outside them.
    let panorama = Binding::root(picture("panorama"), Space::panorama(400, 200).unwrap());
    let view = |yaw, pitch, fov| {
        Location::new(
            panorama.clone(),
            Shape::View { yaw, pitch, fov },
            made_by("find"),
        )
    };
    view(-180.0, 90.0, 120.0).unwrap();
    for (yaw, pitch, fov) in [(180.5, 0.0, 60.0), (0.0, -90.5, 60.0), (0.0, 0.0, 9.0)] {
        assert!(invalid(view(yaw, pitch, fov), ArtifactType::Location));
    }
}

#[test]
fn an_image_holds_its_file_whole_and_takes_its_identity_from_it() {
    let file = photo();
    let image = image_of(file.clone(), ImageRole::Picture);
    assert_eq!(image.file(), file);
    let identified = identify(&file, &limits()).unwrap();
    assert_eq!(image.identity(), identified.identity);
    assert_eq!(image.space(), space());
    assert_eq!(image.pixels(&limits()).unwrap(), identified.pixels);
    // The binding a mask made on the image belongs to.
    assert_eq!(
        image.binding(),
        Binding::root(identified.identity.pixels, space())
    );
    // The same picture stored another way is another artifact with the same pixels.
    let other = png(|x, y| [(x * 30) as u8, (y * 40) as u8, (x * y) as u8, 255]);
    let mut recompressed = Vec::new();
    {
        use image::ImageEncoder;
        let decoded = image::load_from_memory(&other).unwrap().into_rgba8();
        image::codecs::png::PngEncoder::new_with_quality(
            &mut recompressed,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        )
        .write_image(
            decoded.as_raw(),
            WIDTH,
            HEIGHT,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    }
    assert_ne!(recompressed, file);
    let copy = image_of(recompressed, ImageRole::Picture);
    assert_eq!(copy.identity().pixels, image.identity().pixels);
    assert_ne!(copy.identity().file, image.identity().file);
    assert_ne!(copy.id(), image.id());
    // What it is for is part of what it is, and so is its space.
    assert_ne!(
        image_of(file.clone(), ImageRole::Rendering).id(),
        image.id()
    );
    let wide = png_of(400, 200);
    let as_panorama = |projection, topology| {
        Image::new(
            wide.clone(),
            projection,
            topology,
            ImageRole::Picture,
            made_by("import"),
            &limits(),
        )
        .unwrap()
    };
    let flat = as_panorama(Projection::Flat, Topology::Bounded);
    let panorama = as_panorama(Projection::Equirectangular, Topology::Wrap);
    assert_ne!(flat.id(), panorama.id());
    assert_eq!(panorama.space(), Space::panorama(400, 200).unwrap());
    // Bytes that are no picture are refused, and so is a flat space that wraps.
    assert!(matches!(
        Image::new(
            b"not a picture".to_vec(),
            Projection::Flat,
            Topology::Bounded,
            ImageRole::Picture,
            made_by("import"),
            &limits()
        ),
        Err(ArtifactError::Identity(IdentityError::Image(_)))
    ));
    assert!(matches!(
        Image::new(
            file,
            Projection::Flat,
            Topology::Wrap,
            ImageRole::Picture,
            made_by("import"),
            &limits()
        ),
        Err(ArtifactError::Identity(IdentityError::Space(_)))
    ));
}

fn png_of(width: u32, height: u32) -> Vec<u8> {
    let picture = image::RgbaImage::from_fn(width, height, |x, y| {
        image::Rgba([(x % 251) as u8, (y % 241) as u8, 7, 255])
    });
    let mut bytes = Vec::new();
    picture
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

#[test]
fn a_band_profile_has_a_half_width_for_every_row_and_none_under_its_least() {
    let binding = on("photo");
    let line = line(binding.clone()).id();
    let windows = Windows {
        gap: 1,
        step_columns: 3,
        texture_columns: 8,
    };
    let make = |column: u32, thin: f64, half: Vec<f64>| {
        BandProfile::new(
            binding.clone(),
            line,
            column,
            thin,
            half,
            windows,
            "fan-v1",
            made_by("fan"),
        )
    };
    let band = make(4, 1.5, vec![1.5, 2.0, 2.5, 3.0, 2.0, 1.5]).unwrap();
    assert_eq!(band.half().len(), HEIGHT as usize);
    assert_eq!((band.column(), band.thin(), band.line()), (4, 1.5, line));
    assert_eq!((band.windows(), band.constants()), (windows, "fan-v1"));
    for (column, thin, half) in [
        (4, 1.5, vec![1.5; 5]),
        (4, 1.5, vec![1.5, 2.0, 1.25, 3.0, 2.0, 1.5]),
        (4, 1.5, vec![1.5, 2.0, f64::NAN, 3.0, 2.0, 1.5]),
        (4, 0.0, vec![1.5; 6]),
        (4, f64::INFINITY, vec![1.5; 6]),
        (9, 1.5, vec![1.5; 6]),
    ] {
        assert!(
            invalid(make(column, thin, half.clone()), ArtifactType::BandProfile),
            "{column} {thin} {half:?}"
        );
    }
    // Every part of the content is part of the identity.
    let other = make(4, 1.5, vec![1.5, 2.0, 2.5, 3.0, 2.0, 1.75]).unwrap();
    assert_ne!(band.id(), other.id());
}

#[test]
fn a_series_has_a_value_for_every_channel_of_every_row() {
    let binding = on("photo");
    let series = Series::new(
        binding.clone(),
        "tone_ramp.step",
        "levels",
        3,
        (0..18).map(f64::from).collect(),
        made_by("ramp"),
    )
    .unwrap();
    assert_eq!(series.rows(), [0, 1, 2, 3, 4, 5]);
    assert_eq!(series.row(2), Some(&[6.0, 7.0, 8.0][..]));
    assert_eq!(series.row(6), None);
    assert_eq!((series.name(), series.unit()), ("tone_ramp.step", "levels"));
    let sparse = Series::with_rows(
        binding.clone(),
        "sigma_px",
        "px",
        1,
        vec![1, 4],
        vec![0.5, 0.75],
        made_by("feather"),
    )
    .unwrap();
    assert_eq!(sparse.row(4), Some(&[0.75][..]));
    assert_eq!(sparse.row(2), None);
    let make = |channels: u32, rows: Vec<u32>, values: Vec<f64>| {
        Series::with_rows(
            binding.clone(),
            "sigma_px",
            "px",
            channels,
            rows,
            values,
            made_by("feather"),
        )
    };
    for (channels, rows, values) in [
        (0, vec![], vec![]),
        (2, vec![0, 1], vec![1.0, 2.0, 3.0]),
        (1, vec![1, 1], vec![1.0, 2.0]),
        (1, vec![2, 1], vec![1.0, 2.0]),
        (1, vec![0, 6], vec![1.0, 2.0]),
        (1, vec![0, 1], vec![1.0, f64::NAN]),
    ] {
        assert!(
            invalid(make(channels, rows.clone(), values), ArtifactType::Series),
            "{channels} {rows:?}"
        );
    }
}

#[test]
fn a_plan_cuts_a_complete_panorama_by_an_even_strip_that_fits() {
    let panorama = Binding::root(picture("panorama"), Space::panorama(400, 200).unwrap());
    let plan = Plan::new(panorama.clone(), 40, made_by("cut")).unwrap();
    assert_eq!(plan.strip(), 40);
    assert_eq!(plan.left_span(), [360, 400]);
    assert_eq!(plan.right_span(), [0, 40]);
    // The mapping the plan describes is ADR 0001's wrap, under the same rule.
    let canvas = Space::flat(80, 200).unwrap();
    let cut = |strip| {
        Binding::new(
            picture("canvas"),
            canvas,
            picture("panorama"),
            Space::panorama(400, 200).unwrap(),
            Direction::SourceToDestination,
            Mapping::Wrap { strip },
        )
    };
    cut(40).unwrap();
    Plan::new(panorama.clone(), 200, made_by("cut")).unwrap();
    for strip in [0, 1, 41, 202] {
        assert!(
            invalid(
                Plan::new(panorama.clone(), strip, made_by("cut")),
                ArtifactType::Plan
            ),
            "{strip}"
        );
    }
    // A flat image, and a part of a panorama, have no seam to cut.
    for space in [
        Space::flat(400, 200).unwrap(),
        Space::new(400, 200, Projection::Equirectangular, Topology::Bounded).unwrap(),
    ] {
        let binding = Binding::root(picture("panorama"), space);
        assert!(invalid(
            Plan::new(binding, 40, made_by("cut")),
            ArtifactType::Plan
        ));
    }
    assert_ne!(
        plan.id(),
        Plan::new(panorama, 42, made_by("cut")).unwrap().id()
    );
}
