//! One finger drives the mouse pointer while the inventory or a server form owns input.
//!
//! Those screens already read the cursor and left button, so a touchscreen needs no screen
//! changes. Menus read touches themselves and gameplay maps them to touch controls.

use bevy::{
    input::{
        ButtonState,
        mouse::{MouseButton, MouseButtonInput},
        touch::{TouchInput, TouchPhase},
    },
    prelude::{DetectChangesMut, Entity, Local, MessageReader, MessageWriter, Query, Res, With},
    window::{PrimaryWindow, Window},
};
use client_shell::{menu::MenuRuntime, ui_resources::UiRuntimeResource};

/// Which finger owns the emulated pointer, if any.
#[derive(Debug, Default)]
pub(crate) struct TouchPointer {
    owner: Option<u64>,
}

/// What one touch event does to the emulated pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PointerEdge {
    Press,
    /// A completed tap: the button lifts where the finger did.
    Release,
    /// An aborted gesture: the pointer leaves the screen before the button lifts, so nothing
    /// that fires on release sees it over a control.
    Cancel,
}

impl TouchPointer {
    /// Only a finger that lands while a pointer screen owns input presses.
    fn step(&mut self, pointer_screen: bool, id: u64, phase: TouchPhase) -> Option<PointerEdge> {
        match phase {
            TouchPhase::Started if pointer_screen && self.owner.is_none() => {
                self.owner = Some(id);
                Some(PointerEdge::Press)
            }
            TouchPhase::Ended if self.owner == Some(id) => {
                self.owner = None;
                Some(PointerEdge::Release)
            }
            TouchPhase::Canceled if self.owner == Some(id) => {
                self.owner = None;
                Some(PointerEdge::Cancel)
            }
            _ => None,
        }
    }

    fn owns(&self, id: u64) -> bool {
        self.owner == Some(id)
    }

    /// Cancels a held pointer once its screen no longer owns input.
    fn lose_screen(&mut self) -> Option<PointerEdge> {
        self.owner.take().map(|_| PointerEdge::Cancel)
    }
}

/// Runs before Bevy's input systems, so the emitted button edges reach `ButtonInput` this frame.
pub(crate) fn emulate_ui_pointer(
    mut touch_input: MessageReader<TouchInput>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut windows: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    ui: Option<Res<UiRuntimeResource>>,
    menu: Option<Res<MenuRuntime>>,
    mut pointer: Local<TouchPointer>,
) {
    let Ok((entity, mut window)) = windows.single_mut() else {
        return;
    };
    let pointer_screen = ui.is_some_and(|ui| ui.inventory_open() || ui.server_forms().owns_input())
        && !menu.is_some_and(|menu| menu.is_visible());
    // Bypassed so the window backend is not asked to warp a cursor touchscreens lack.
    let mut apply = |window: &mut Window, edge: PointerEdge| {
        let state = match edge {
            PointerEdge::Press => ButtonState::Pressed,
            PointerEdge::Release => ButtonState::Released,
            PointerEdge::Cancel => {
                window.set_cursor_position(None);
                ButtonState::Released
            }
        };
        buttons.write(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window: entity,
        });
    };
    for input in touch_input.read() {
        let edge = pointer.step(pointer_screen, input.id, input.phase);
        let window = window.bypass_change_detection();
        if pointer.owns(input.id) || matches!(edge, Some(PointerEdge::Release)) {
            window.set_cursor_position(Some(input.position));
        }
        if let Some(edge) = edge {
            apply(window, edge);
        }
    }
    if !pointer_screen && let Some(edge) = pointer.lose_screen() {
        apply(window.bypass_change_detection(), edge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fingers that land outside a pointer screen, or a second finger, never press.
    #[test]
    fn only_the_first_finger_on_a_pointer_screen_presses() {
        let mut pointer = TouchPointer::default();
        assert_eq!(pointer.step(false, 1, TouchPhase::Started), None);
        assert_eq!(pointer.step(true, 1, TouchPhase::Ended), None);
        assert_eq!(
            pointer.step(true, 2, TouchPhase::Started),
            Some(PointerEdge::Press)
        );
        assert_eq!(pointer.step(true, 3, TouchPhase::Started), None);
        assert_eq!(pointer.step(true, 3, TouchPhase::Ended), None);
        assert_eq!(
            pointer.step(true, 2, TouchPhase::Ended),
            Some(PointerEdge::Release)
        );
    }

    /// A cancelled gesture, or one whose screen closes, must not complete as a tap.
    #[test]
    fn cancelled_gestures_cancel_instead_of_releasing() {
        let mut pointer = TouchPointer::default();
        pointer.step(true, 1, TouchPhase::Started);
        assert_eq!(
            pointer.step(true, 1, TouchPhase::Canceled),
            Some(PointerEdge::Cancel)
        );
        pointer.step(true, 2, TouchPhase::Started);
        assert_eq!(pointer.lose_screen(), Some(PointerEdge::Cancel));
        assert_eq!(pointer.step(true, 2, TouchPhase::Ended), None);
        assert_eq!(pointer.lose_screen(), None);
    }
}
