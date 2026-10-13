//! Import uses the existing OreUI modal, menu rows and primary/secondary buttons.

use super::{
    Look, OreUiScreen,
    modal::{self, Modal},
    paint::Canvas,
    theme::{Appearance, TEXT},
    widgets::{MenuItem, Variant},
};
use launcher::{
    menu::{MenuAction, MenuView},
    minecraft_import::{Action, Phase},
};
use ui::UiRect;
use ui_engine::{
    engine::screen::{PaintError, PaintFrame},
    scroll::ScrollArea,
};

/// The import modal's click targets and scrolling body areas.
type ImportTargets = (Vec<(MenuAction, UiRect)>, Vec<ScrollArea>);

impl OreUiScreen {
    /// Draws the consent, profile, preview and restore steps with the standard modal controls.
    pub fn paint_minecraft_import(
        &mut self,
        view: &MenuView,
        size: [f32; 2],
        frame: &mut PaintFrame<'_>,
    ) -> Result<ImportTargets, PaintError> {
        let state = &view.minecraft_import;
        let action = |action| MenuAction::MinecraftImport(action);
        let items = if state.phase == Phase::Profiles {
            state
                .profiles
                .iter()
                .enumerate()
                .map(|(index, label)| MenuItem {
                    label,
                    picture_slot: false,
                    picture: None,
                    selected: false,
                    enabled: true,
                    action: Some(action(Action::Select(index))),
                })
                .collect()
        } else {
            Vec::new()
        };
        let buttons = state
            .actions()
            .into_iter()
            .filter(|action| !matches!(action, Action::Select(_)))
            .enumerate()
            .map(|(index, value)| {
                (
                    state.label(value).into(),
                    if index == 0 {
                        Variant::Primary
                    } else {
                        Variant::Secondary
                    },
                    Some(action(value)),
                )
            })
            .collect();
        let close = if state.phase == Phase::Busy {
            None
        } else if state.phase == Phase::Offered {
            Some(action(Action::NotNow))
        } else {
            Some(action(Action::Close))
        };
        let dialog = Modal {
            title: state.title(),
            items,
            body: if state.pack_status.is_empty() {
                state.body.as_str().into()
            } else {
                format!("{}\n\n{}", state.body, state.pack_status).into()
            },
            body_color: TEXT,
            buttons,
            close,
        };
        let originals = self
            .originals
            .as_deref()
            .filter(|_| self.look == Look::Originals);
        let mut canvas = Canvas::new(
            &mut *frame.nodes,
            &mut *frame.next,
            &mut *frame.layouts,
            frame.font,
            frame.metrics,
            frame.solid_page,
            originals,
        );
        canvas.appearance = Appearance::from_dark(view.settings_options.oreui_dark_mode());
        canvas.seconds = frame.now;
        canvas.transitions = Some(&mut self.transitions);
        modal::draw(&mut canvas, view, size, &dialog)?;
        Ok((canvas.hits, canvas.scrolls))
    }
}
