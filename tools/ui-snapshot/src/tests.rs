use crate::{carriers, fixtures, render};

#[test]
fn every_registered_state_renders_identical_png_bytes() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/assets/compiled");
    let mut names = std::collections::HashSet::new();
    for state in fixtures::STATES {
        assert!(names.insert(state.name), "duplicate fixture {}", state.name);
        let first = match render::png(state, &dir) {
            Ok(png) => png,
            Err(carriers::Error::Missing(message)) => {
                eprintln!("skipping {}: missing fixture {message}", state.name);
                continue;
            }
            Err(error) => panic!("{}: {error:?}", state.name),
        };
        let second = render::png(state, &dir).unwrap();
        assert_eq!(first, second, "{} PNG bytes changed", state.name);
        let image = image::load_from_memory(&first).unwrap().to_rgba8();
        assert_eq!([image.width(), image.height()], state.size);
        assert!(
            image.pixels().any(|pixel| pixel[3] != 0),
            "{} is blank",
            state.name
        );
    }
}

#[test]
fn absent_and_broken_carriers_have_different_outcomes() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        carriers::load(dir.path(), false),
        Err(carriers::Error::Missing(_))
    ));
    std::fs::write(dir.path().join(assets::carriers::FONT.output), b"invalid").unwrap();
    assert!(matches!(
        carriers::load(dir.path(), false),
        Err(carriers::Error::Invalid(_))
    ));
}
