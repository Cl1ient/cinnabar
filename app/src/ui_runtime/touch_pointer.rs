//! One finger drives the mouse pointer while a screen with no touch path of its own owns input:
//! the inventory, a server form, the sign editor or the bed screen.
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
use client_ui::ui_runtime::UiRuntime;

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

    /// Whether this finger holds the emulated pointer.
    fn owns(&self, id: u64) -> bool {
        self.owner == Some(id)
    }

    /// Cancels a held pointer once its screen no longer owns input.
    fn lose_screen(&mut self) -> Option<PointerEdge> {
        self.owner.take().map(|_| PointerEdge::Cancel)
    }
}

/// Whether the screen owning input reads only the mouse pointer; a server settings form stays
/// above a visible menu, which otherwise takes touches itself.
fn takes_touch_pointer(ui: &UiRuntime, menu_visible: bool) -> bool {
    let bed = ui.local_sleeping() && !ui.chat_focused();
    (ui.inventory_open() || ui.server_forms().owns_input() || ui.sign_editor().is_open() || bed)
        && (!menu_visible || ui.server_forms().settings_form_active())
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
    let pointer_screen =
        ui.is_some_and(|ui| takes_touch_pointer(&ui, menu.is_some_and(|menu| menu.is_visible())));
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
                            content: String::new().into(),
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
            Some(PointerEdge::Press)
        );
        assert_eq!(pointer.step(true, 3, TouchPhase::Started), None);
        assert_eq!(pointer.step(true, 3, TouchPhase::Ended), None);
        assert_eq!(
            pointer.step(true, 2, TouchPhase::Ended),
            Some(PointerEdge::Release)
        );
    }

    /// The bed screen's Leave Bed and Open Chat read only the pointer, so touch drives it.
    #[test]
    fn the_bed_screen_takes_the_touch_pointer() {
        let mut ui = UiRuntime::new(1);
        assert!(!takes_touch_pointer(&ui, false));
        ui.set_local_sleeping(true);
        assert!(takes_touch_pointer(&ui, false));
        assert!(!takes_touch_pointer(&ui, true));
    }

    /// The sign editor's exit reads only the pointer; without a keyboard touch is the way out.
    #[test]
    fn the_sign_editor_takes_the_touch_pointer() {
        let mut ui = UiRuntime::new(1);
        ui.sign_editor_mut()
            .open(client_ui::ui_runtime::sign_editor::SignEdit::new(
                [0; 3],
                true,
                world::NbtCompound::default(),
            ));
        assert!(takes_touch_pointer(&ui, false));
        assert!(!takes_touch_pointer(&ui, true));
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
