//! Gameplay touch controls: each finger keeps the control it pressed until it lifts.

use bevy::{
    input::touch::Touches,
    math::Vec2,
    prelude::{Local, Query, ResMut, With},
    window::{PrimaryWindow, Window},
};
use semantic_input::{TouchContact, TouchControlState, touch};

use crate::semantic_controls::SemanticTouchTargets;
use client_shell::{
    menu::MenuRuntime,
    ui_resources::{UiPresentationResource, UiRuntimeResource},
};

/// Look strength per viewport fraction dragged, wider across for a landscape screen.
/// Provisional until measured against the native touch look curve.
const LOOK_GAIN: [f32; 2] = [4.0, 2.0];

#[allow(
    clippy::too_many_arguments,
    reason = "UI, menu and player authority are borrowed separately."
)]
pub(crate) fn drive_gameplay_touch_targets(
    mut touches: ResMut<Touches>,
    mut ui: ResMut<UiRuntimeResource>,
    mut targets: ResMut<SemanticTouchTargets>,
    player: Option<ResMut<client_state::PlayerRuntime>>,
    mut menu: Option<ResMut<MenuRuntime>>,
    presentation: Option<ResMut<UiPresentationResource>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut state: Local<TouchControlState>,
) {
    targets.release_all();
    let Some(mut presentation) = presentation.filter(|view| view.touch_controls_enabled()) else {
        state.clear();
        return;
    };
    let gameplay = match (player.as_deref(), windows.single()) {
        (Some(player), Ok(window)) => (window.focused
            && !ui.ui_focused(player)
            && !menu.as_deref().is_some_and(MenuRuntime::is_visible))
        .then_some(window),
        _ => None,
    };
    let (Some(window), Some(mut player)) = (gameplay, player) else {
        state.clear();
        presentation.set_touch_contacts(Vec::new());
        return;
    };
    let size = [window.width().max(1.0), window.height().max(1.0)];
    let normalized = |position: Vec2| {
        [
            (position.x / size[0]).clamp(0.0, 1.0),
            (position.y / size[1]).clamp(0.0, 1.0),
        ]
    };
    for contact in touches.iter_just_canceled() {
        state.release(contact.id());
    }
    let started: Vec<_> = touches
        .iter_just_pressed()
        .filter(|contact| !touches.just_canceled(contact.id()))
        .map(|contact| (contact.id(), normalized(contact.start_position())))
        .collect();
    for &(id, start) in &started {
        let region = presentation.gameplay_touch_region(start, size);
        let opens_screen = match region.map(|region| region.hit_id) {
            Some(touch::INVENTORY) => {
                let _ = ui.toggle_inventory(&mut player);
                true
            }
            Some(touch::MENU) => {
                if let Some(menu) = menu.as_mut() {
                    menu.open_pause();
                }
                true
            }
            _ => false,
        };
        if opens_screen {
            // The opened screen must not read these presses as its own taps.
            for &(id, _) in &started {
                touches.clear_just_pressed(id);
            }
            state.clear();
            presentation.set_touch_contacts(Vec::new());
            return;
        }
        state.begin(id, start, 0, region);
    }
    for contact in touches.iter().chain(touches.iter_just_released()) {
        state.move_to(contact.id(), normalized(contact.position()), 0);
    }
    let samples = state.sample();
    presentation.set_touch_contacts(samples.clone());
    for sample in samples {
        let sample = if sample.hit_id == Some(touch::LOOK_SURFACE) {
            TouchContact {
                delta: [
                    sample.delta[0] * LOOK_GAIN[0],
                    sample.delta[1] * LOOK_GAIN[1],
                ],
                ..sample
            }
        } else {
            sample
        };
        if touches.just_released(sample.contact_id) {
            targets.pulse(sample);
        } else {
            targets.set_sample(&sample);
        }
    }
    for contact in touches.iter_just_released() {
        state.release(contact.id());
    }
}
