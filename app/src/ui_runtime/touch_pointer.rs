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

impl TouchPointer {
    /// The button edge one touch event produces; only a finger that lands while a pointer
    /// screen owns input presses, and losing that screen releases it.
    fn step(&mut self, pointer_screen: bool, id: u64, phase: TouchPhase) -> Option<ButtonState> {
        match phase {
            TouchPhase::Started if pointer_screen && self.owner.is_none() => {
                self.owner = Some(id);
                Some(ButtonState::Pressed)
            }
            TouchPhase::Ended | TouchPhase::Canceled if self.owner == Some(id) => {
                self.owner = None;
                Some(ButtonState::Released)
            }
            _ => None,
        }
    }

    fn owns(&self, id: u64) -> bool {
        self.owner == Some(id)
    }

    /// Releases a held pointer once its screen no longer owns input.
    fn lose_screen(&mut self) -> Option<ButtonState> {
        self.owner.take().map(|_| ButtonState::Released)
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
    let mut edge = |state| {
        buttons.write(MouseButtonInput {
            button: MouseButton::Left,
            state,
            window: entity,
        });
    };
    for input in touch_input.read() {
        let state = pointer.step(pointer_screen, input.id, input.phase);
        if pointer.owns(input.id) || state.is_some() {
            // Bypassed so the window backend is not asked to warp a cursor touchscreens lack.
            window
                .bypass_change_detection()
                .set_cursor_position(Some(input.position));
        }
        if let Some(state) = state {
            edge(state);
        }
    }
    if !pointer_screen && let Some(state) = pointer.lose_screen() {
        edge(state);
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
            Some(ButtonState::Pressed)
        );
        assert_eq!(pointer.step(true, 3, TouchPhase::Started), None);
        assert_eq!(pointer.step(true, 3, TouchPhase::Ended), None);
        assert_eq!(
            pointer.step(true, 2, TouchPhase::Ended),
            Some(ButtonState::Released)
        );
        assert_eq!(
            pointer.step(true, 4, TouchPhase::Started),
            Some(ButtonState::Pressed)
        );
        assert_eq!(pointer.lose_screen(), Some(ButtonState::Released));
        assert_eq!(pointer.step(true, 4, TouchPhase::Canceled), None);
    }
}
