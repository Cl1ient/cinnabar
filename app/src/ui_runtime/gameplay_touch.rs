//! Gameplay touch controls: each finger keeps the control it pressed until it lifts.

use bevy::{
    ecs::message::{MessageCursor, Messages},
    input::touch::{TouchInput, TouchPhase, Touches},
    prelude::{Local, Query, Res, ResMut, With},
    window::{PrimaryWindow, Window},
};
use semantic_input::{TouchContact, TouchControlRegion, TouchControlState, touch};

use crate::semantic_controls::SemanticTouchTargets;
use client_shell::{
    menu::MenuRuntime,
    ui_resources::{UiPresentationResource, UiRuntimeResource},
};

/// Look strength per viewport fraction dragged, wider across for a landscape screen.
/// Provisional until measured against the native touch look curve.
const LOOK_GAIN: [f32; 2] = [4.0, 2.0];

/// Applies touch events in arrival order, so an id Android reuses after a lift starts a fresh
/// contact. Returns the lifted contacts' last samples, or the screen-opening control a press hit.
fn apply_touch_events(
    state: &mut TouchControlState,
    events: impl IntoIterator<Item = (u64, TouchPhase, [f32; 2])>,
    region: impl Fn([f32; 2]) -> Option<TouchControlRegion>,
) -> Result<Vec<TouchContact>, u16> {
    let mut lifted = Vec::new();
    for (id, phase, position) in events {
        match phase {
            TouchPhase::Started => {
                let region = region(position);
                if let Some(hit_id) = region
                    .map(|region| region.hit_id)
                    .filter(|hit_id| matches!(*hit_id, touch::INVENTORY | touch::MENU))
                {
                    return Err(hit_id);
                }
                state.begin(id, position, 0, region);
            }
            TouchPhase::Moved => {
                state.move_to(id, position, 0);
            }
            TouchPhase::Ended => {
                state.move_to(id, position, 0);
                if let Some(sample) = state.end(id) {
                    // A reused id may lift twice in one frame; a frame carries each id once.
                    lifted.retain(|earlier: &TouchContact| earlier.contact_id != id);
                    lifted.push(sample);
                }
            }
            TouchPhase::Canceled => state.release(id),
        }
    }
    Ok(lifted)
}

#[allow(
    clippy::too_many_arguments,
    reason = "UI, menu and player authority are borrowed separately."
)]
pub(crate) fn drive_gameplay_touch_targets(
    mut touches: ResMut<Touches>,
    touch_input: Option<Res<Messages<TouchInput>>>,
    mut touch_cursor: Local<MessageCursor<TouchInput>>,
    mut ui: ResMut<UiRuntimeResource>,
    mut targets: ResMut<SemanticTouchTargets>,
    player: Option<ResMut<client_state::PlayerRuntime>>,
    mut menu: Option<ResMut<MenuRuntime>>,
    presentation: Option<ResMut<UiPresentationResource>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut state: Local<TouchControlState>,
) {
    targets.release_all();
    // Every event is consumed each frame, so none replays once gameplay regains input.
    let events: Vec<_> = touch_input
        .as_deref()
        .map(|messages| touch_cursor.read(messages).cloned().collect())
        .unwrap_or_default();
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
    let events = events.iter().map(|event| {
        let position = [
            (event.position.x / size[0]).clamp(0.0, 1.0),
            (event.position.y / size[1]).clamp(0.0, 1.0),
        ];
        (event.id, event.phase, position)
    });
    let lifted = match apply_touch_events(&mut state, events, |position| {
        presentation.gameplay_touch_region(position, size)
    }) {
        Ok(lifted) => lifted,
        Err(hit_id) => {
            if hit_id == touch::INVENTORY {
                let _ = ui.toggle_inventory(&mut player);
            } else if let Some(menu) = menu.as_mut() {
                menu.open_pause();
            }
            // The opened screen must not read this frame's presses as its own taps.
            let started: Vec<_> = touches
                .iter_just_pressed()
                .map(|touch| touch.id())
                .collect();
            for id in started {
                touches.clear_just_pressed(id);
            }
            state.clear();
            presentation.set_touch_contacts(Vec::new());
            return;
        }
    };
    let held = state.sample();
    presentation.set_touch_contacts(held.clone());
    let look_scaled = |sample: TouchContact| {
        if sample.hit_id == Some(touch::LOOK_SURFACE) {
            TouchContact {
                delta: [
                    sample.delta[0] * LOOK_GAIN[0],
                    sample.delta[1] * LOOK_GAIN[1],
                ],
                ..sample
            }
        } else {
            sample
        }
    };
    for sample in held.iter().cloned().map(look_scaled) {
        targets.set_sample(&sample);
    }
    // A lifted contact whose id already carries a new press routes as that press instead.
    for sample in lifted.into_iter().map(look_scaled) {
        if !held
            .iter()
            .any(|other| other.contact_id == sample.contact_id)
        {
            targets.pulse(sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semantic_input::TouchBounds;

    fn stick(_: [f32; 2]) -> Option<TouchControlRegion> {
        Some(TouchControlRegion {
            hit_id: touch::JOYSTICK,
            bounds: TouchBounds {
                min: [0.0; 2],
                max: [1.0; 2],
            },
        })
    }

    /// Android may end a finger and reuse its id for a new press before the next update.
    #[test]
    fn a_reused_touch_id_keeps_its_new_contact() {
        let mut state = TouchControlState::default();
        let at = [0.5, 0.5];
        apply_touch_events(&mut state, [(3, TouchPhase::Started, at)], stick).unwrap();
        let lifted = apply_touch_events(
            &mut state,
            [(3, TouchPhase::Ended, at), (3, TouchPhase::Started, at)],
            stick,
        )
        .unwrap();
        assert_eq!(lifted.len(), 1);
        let held = state.sample();
        assert_eq!(held.len(), 1);
        assert_eq!(
            (held[0].contact_id, held[0].hit_id),
            (3, Some(touch::JOYSTICK))
        );
    }

    /// A reused id that lifts twice in one frame yields one sample, so the routed frame stays
    /// valid and other fingers keep moving the player.
    #[test]
    fn a_reused_id_lifting_twice_still_routes_other_fingers() {
        use semantic_input::{DeviceFrame, SemanticInputRouter};

        let controls = |position: [f32; 2]| {
            let (hit_id, min, max) = if position[0] < 0.5 {
                (touch::JOYSTICK, [0.0, 0.0], [0.5, 1.0])
            } else {
                (touch::JUMP, [0.5, 0.0], [1.0, 1.0])
            };
            Some(TouchControlRegion {
                hit_id,
                bounds: TouchBounds { min, max },
            })
        };
        let jump = [0.75, 0.5];
        let mut state = TouchControlState::default();
        apply_touch_events(
            &mut state,
            [
                (9, TouchPhase::Started, [0.25, 0.5]),
                (3, TouchPhase::Started, jump),
            ],
            controls,
        )
        .unwrap();
        let lifted = apply_touch_events(
            &mut state,
            [
                (3, TouchPhase::Ended, jump),
                (3, TouchPhase::Started, jump),
                (3, TouchPhase::Ended, jump),
                (9, TouchPhase::Moved, [0.25, 0.0]),
            ],
            controls,
        )
        .unwrap();
        assert_eq!(lifted.len(), 1);
        let mut touches = state.sample();
        touches.extend(lifted);
        let mut router = SemanticInputRouter::default();
        router
            .route(DeviceFrame {
                touches,
                ..DeviceFrame::default()
            })
            .unwrap();
        assert!(router.finalize().unwrap().movement[1] > 0.0);
    }

    /// Pause and inventory presses stop processing and open their screens.
    #[test]
    fn screen_controls_open_instead_of_capturing() {
        let mut state = TouchControlState::default();
        let pause = |_| {
            Some(TouchControlRegion {
                hit_id: touch::MENU,
                bounds: TouchBounds {
                    min: [0.0; 2],
                    max: [1.0; 2],
                },
            })
        };
        assert_eq!(
            apply_touch_events(&mut state, [(1, TouchPhase::Started, [0.5; 2])], pause),
            Err(touch::MENU)
        );
        assert!(state.sample().is_empty());
    }
}
