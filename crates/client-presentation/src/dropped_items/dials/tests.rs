use std::sync::Arc;

use super::*;
use crate::dropped_items::DroppedItemPublisher;
use assets::icon::dial::{DIAL_STRIPS, DialKind};
use render_model::DroppedItemModel;

/// Creates distinct sprites for every dial frame and one unrelated item without carriers.
fn icons() -> UiPresentationRuntime {
    let mut sprites = vec![assets::IconSprite {
        width: 1,
        height: 1,
        rgba8: vec![0, 71, 121, 255].into(),
    }];
    let mut entries = vec![assets::IconEntry {
        identifier: "test:ordinary".into(),
        metadata: 0,
        sprite: 0,
    }];
    for strip in DIAL_STRIPS {
        entries.push(assets::IconEntry {
            identifier: strip.item.into(),
            metadata: 0,
            sprite: 0,
        });
        let count = match strip.kind {
            DialKind::Compass | DialKind::RecoveryCompass => inventory::COMPASS_FRAMES,
            DialKind::Clock => inventory::CLOCK_FRAMES,
        };
        for frame in 0..count {
            let sprite = sprites.len() as u32;
            entries.push(assets::IconEntry {
                identifier: strip.frames_identifier.into(),
                metadata: frame,
                sprite,
            });
            sprites.push(assets::IconSprite {
                width: 1,
                height: 1,
                rgba8: vec![sprite as u8, 71, 121, 255].into(),
            });
        }
    }
    entries.sort_by(|a, b| (&a.identifier, a.metadata).cmp(&(&b.identifier, b.metadata)));
    let catalog = assets::RuntimeIconCatalog::decode(
        &assets::encode_icon_catalog([5; 32], &sprites, &entries).unwrap(),
    )
    .unwrap();
    UiPresentationRuntime::with_hud_and_icons(
        ui_engine::test_support::fixture_font(),
        client_ui::test_support::fixture_hud(),
        Arc::new(catalog),
    )
    .unwrap()
}

/// Resolves current models before warming all active strips, as the publisher does.
fn publish(cache: &mut ModelCache, icons: &UiPresentationRuntime, dials: bool) -> Vec<u32> {
    let mut animation = DialModels::default();
    let mut current = vec![("test:ordinary", 0)];
    if dials {
        for strip in DIAL_STRIPS {
            animation.observe(icons, strip.item, 0);
            current.push(icons.dial_icon_key(strip.item, 0).unwrap());
        }
    }
    cache.begin_frame();
    for &(identifier, metadata) in &current {
        cache.request(&ModelKey::icon(
            icons.item_icon(identifier, metadata).unwrap(),
            true,
        ));
    }
    cache.finish_demand();
    let handles = current
        .into_iter()
        .map(|(identifier, metadata)| {
            DroppedItemPublisher::icon_model(cache, icons, identifier, metadata, true).unwrap()
        })
        .collect();
    animation.warm(cache, icons);
    cache.finish_frame();
    handles
}

#[test]
fn every_dial_frame_changes_handles_without_republishing_scene_models() {
    let mut icons = icons();
    let mut cache = ModelCache::default();
    icons.set_dial_frames(inventory::DialFrames {
        compass: Some(0),
        recovery_compass: Some(0),
        clock: Some(0),
    });
    let first = publish(&mut cache, &icons, true);
    let revision = cache.revision;
    let shared = Arc::clone(&cache.shared);
    let resident_count = cache.models.len();
    let mut previous = first;
    for frame in 1..=inventory::CLOCK_FRAMES * 2 {
        icons.set_dial_frames(inventory::DialFrames {
            compass: Some(frame % inventory::COMPASS_FRAMES),
            recovery_compass: Some(frame % inventory::COMPASS_FRAMES),
            clock: Some(frame % inventory::CLOCK_FRAMES),
        });
        let current = publish(&mut cache, &icons, true);
        assert_eq!(current[0], previous[0], "unrelated item keeps its handle");
        for (&handle, &old) in current[1..].iter().zip(&previous[1..]) {
            assert_ne!(handle, old, "dial selects a different model");
            assert!(matches!(
                cache.shared[handle as usize],
                DroppedItemModel::NativeSprite(_)
            ));
        }
        assert_eq!(
            cache.revision, revision,
            "animation must not rebuild GPU models"
        );
        assert!(Arc::ptr_eq(&shared, &cache.shared));
        assert_eq!(cache.models.len(), resident_count);
        previous = current;
    }
}

#[test]
fn removed_dials_release_published_frames_and_reuse_cached_models_on_return() {
    let mut icons = icons();
    let mut cache = ModelCache::default();
    icons.set_dial_frames(inventory::DialFrames {
        compass: Some(0),
        recovery_compass: Some(0),
        clock: Some(0),
    });
    let handles = publish(&mut cache, &icons, true);
    let resident_count = cache.models.len();
    let ordinary = publish(&mut cache, &icons, false);
    assert_eq!(ordinary, handles[..1]);
    for (slot, model) in cache.shared.iter().enumerate() {
        assert_eq!(
            matches!(model, DroppedItemModel::Vacant),
            slot != ordinary[0] as usize
        );
    }
    let inactive_revision = cache.revision;
    publish(&mut cache, &icons, false);
    assert_eq!(cache.revision, inactive_revision);
    assert_eq!(publish(&mut cache, &icons, true), handles);
    assert_eq!(cache.models.len(), resident_count);
}
