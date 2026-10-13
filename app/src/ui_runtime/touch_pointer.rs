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

    /// Whether this finger holds the emulated pointer.
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
    let pointer_screen = ui.is_some_and(|ui| {
        (ui.inventory_open() || ui.server_forms().owns_input())
            && (!menu.is_some_and(|menu| menu.is_visible())
                || ui.server_forms().settings_form_active())
    });
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
    use bevy::{
        ecs::message::MessageCursor,
        prelude::{App, Messages, Update, Vec2},
    };
    use client_ui::ui_runtime::{SequencedUiEvent, UiRuntime};

    /// Creates a retained form whose ownership the pointer adapter must follow.
    fn form_runtime(settings: bool) -> UiRuntime {
        let mut runtime = UiRuntime::new(1);
        runtime
            .apply(
                &mut player_state::PlayerState::new(1),
                SequencedUiEvent {
                    session_id: 1,
                    fifo_sequence: 1,
                    local_millis: 0,
                    server_tick: None,
                    event: protocol::UiEvent::Form(protocol::FormRequestEvent {
                        form_id: 7,
                        kind: if settings {
                            protocol::FormKind::ServerSettings
                        } else {
                            protocol::FormKind::Menu
                        },
                        title: None,
                        json: "{}".into(),
                        model: protocol::ServerFormModel::TextMenu(protocol::TextMenuForm {
                            title: "Test".into(),
                            content: String::new(),
                            buttons: vec!["Choose".into()].into(),
                            button_images: [].into(),
                            omitted_images: 0,
                        }),
                    }),
                },
            )
            .unwrap();
        runtime
    }

    #[test]
    fn settings_form_over_the_menu_emits_touch_press_and_release() {
        for (settings, visible, expected) in [
            (true, true, true),
            (false, true, false),
            (false, false, true),
        ] {
            let mut menu = MenuRuntime::new(true, 2, "Test".into());
            menu.set_visible(visible);
            let mut app = App::new();
            app.add_message::<TouchInput>()
                .add_message::<MouseButtonInput>()
                .insert_resource(UiRuntimeResource(form_runtime(settings)))
                .insert_resource(menu)
                .add_systems(Update, emulate_ui_pointer);
            let entity = app
                .world_mut()
                .spawn((Window::default(), PrimaryWindow))
                .id();
            let position = Vec2::new(40.0, 50.0);
            for phase in [TouchPhase::Started, TouchPhase::Ended] {
                app.world_mut().write_message(TouchInput {
                    phase,
                    position,
                    window: entity,
                    force: None,
                    id: 1,
                });
            }
            app.update();
            let mut cursor = MessageCursor::<MouseButtonInput>::default();
            let edges = cursor
                .read(app.world().resource::<Messages<MouseButtonInput>>())
                .map(|event| (event.button, event.state, event.window))
                .collect::<Vec<_>>();
            assert_eq!(
                edges,
                if expected {
                    vec![
                        (MouseButton::Left, ButtonState::Pressed, entity),
                        (MouseButton::Left, ButtonState::Released, entity),
                    ]
                } else {
                    Vec::new()
                },
                "settings={settings}, visible={visible}"
            );
            if expected {
                assert_eq!(
                    app.world().get::<Window>(entity).unwrap().cursor_position(),
                    Some(position)
                );
            }
        }
    }

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
