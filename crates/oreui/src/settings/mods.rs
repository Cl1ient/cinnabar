//! The Mods tab: the resource-pack page's toolbar, groups and cards over the mod library.

use super::super::theme;
use super::super::transitions::resources::{Group, Listing, Mods};
use super::super::widgets::Variant;
use super::Content;
use super::resources::{Face, accordion, actions, caption, card, cards, empty, note, reveal};
use launcher::menu::{MenuAction, file_size};
use launcher::mods::{Action, InstalledMod, Snapshot, Status};
use ui_engine::engine::screen::PaintError;

#[cfg(test)]
mod tests;

const UNSUPPORTED: &str = "Mods aren't available in this build.";

fn command(action: Action) -> MenuAction {
    MenuAction::Mods(action)
}

pub(super) fn draw(content: &mut Content<'_, '_>) -> Result<(), PaintError> {
    let mut motion = content
        .canvas
        .transitions
        .as_deref_mut()
        .map(|transitions| std::mem::take(&mut transitions.mods))
        .unwrap_or_default();
    let animated = content
        .canvas
        .transitions
        .as_deref()
        .is_some_and(|t| t.motion.enabled());
    let snapshot = content.view.mods.clone();
    let (active, available) = snapshot.library.groups();
    let position = |list: &[InstalledMod], id: u64| list.iter().position(|item| item.id == id);
    let details = snapshot.details_expanded.and_then(|id| {
        position(&active, id)
            .map(|index| (true, index))
            .or_else(|| position(&available, id).map(|index| (false, index)))
    });
    motion.sync_listing(
        Listing {
            active: &active,
            available: &available,
            expanded: [snapshot.active_expanded, snapshot.available_expanded],
            details,
            pending: false,
        },
        content.canvas.seconds,
        animated,
    );
    let result = draw_sections(content, &snapshot, &motion);
    if let Some(transitions) = content.canvas.transitions.as_deref_mut() {
        transitions.mods = motion;
    }
    result
}

fn draw_sections(
    content: &mut Content<'_, '_>,
    snapshot: &Snapshot,
    motion: &Mods,
) -> Result<(), PaintError> {
    let library = &snapshot.library;
    content.heading("Mods", "Add features to your game with WebAssembly mods.")?;
    actions(
        content,
        &[(
            "Import mod…",
            Variant::Secondary,
            (library.supported && !library.busy).then_some(command(Action::Import)),
        )],
    )?;
    content.y += content.canvas.r(1.2);
    if !library.supported {
        note(content, UNSUPPORTED, theme::NEUTRAL80.fill)?;
    }
    if !library.message.is_empty() {
        note(content, &library.message, theme::NEUTRAL80.fill)?;
    }
    let enabled = library
        .installed
        .iter()
        .filter(|item| Some(item.id) == library.enabled)
        .count();
    accordion(
        content,
        &format!("Active mods ({enabled})"),
        motion.expanded[0],
        command(Action::ToggleActive),
    )?;
    reveal(
        content,
        motion.expanded[0],
        snapshot.active_expanded,
        |content| {
            cards(content, motion, Group::Active, |content, card| {
                mod_card(
                    content,
                    snapshot,
                    &card.item,
                    true,
                    card.details,
                    card.present,
                )
            })?;
            caption(content, "One mod runs at a time.")
        },
    )?;
    content.y += content.canvas.r(1.2);
    accordion(
        content,
        &format!("My mods ({})", library.installed.len() - enabled),
        motion.expanded[1],
        command(Action::ToggleAvailable),
    )?;
    reveal(
        content,
        motion.expanded[1],
        snapshot.available_expanded,
        |content| {
            reveal(content, motion.empty_fraction, false, |content| {
                empty(
                    content,
                    if library.installed.is_empty() {
                        "No mods installed"
                    } else {
                        "No other mods"
                    },
                    "Import a .wasm mod component to add it here.",
                )
            })?;
            cards(content, motion, Group::Available, |content, card| {
                mod_card(
                    content,
                    snapshot,
                    &card.item,
                    false,
                    card.details,
                    card.present,
                )
            })
        },
    )?;
    content.y += content.canvas.r(2.4);
    Ok(())
}

fn mod_card(
    content: &mut Content<'_, '_>,
    snapshot: &Snapshot,
    item: &InstalledMod,
    active: bool,
    details: f32,
    interactive: bool,
) -> Result<(), PaintError> {
    let library = &snapshot.library;
    let failure = library.status.as_ref().and_then(|status| match status {
        Status::Failed(message) if active => Some(message.as_str()),
        _ => None,
    });
    let subtitle = match (active, &library.status) {
        (false, _) => "Ready to enable",
        (true, Some(Status::Running)) => "Running",
        (true, Some(Status::Failed(_))) => "Failed to load",
        (true, Some(Status::Starting) | None) => "Starting…",
    };
    let face = Face {
        icon: None,
        title: &item.name,
        subtitle,
        expanded: snapshot.details_expanded == Some(item.id),
        toggle: command(Action::ReadMore(item.id)),
        details,
        interactive,
    };
    card(content, face, |content| {
        caption(
            content,
            &format!("{} · {}", item.file, file_size(item.bytes)),
        )?;
        if let Some(message) = failure {
            caption(content, message)?;
        }
        let available = interactive && library.supported && !library.busy;
        let toggle = if active {
            (
                "Disable",
                Variant::Secondary,
                available.then_some(command(Action::Disable)),
            )
        } else {
            (
                "Enable",
                Variant::Primary,
                available.then_some(command(Action::Enable(item.id))),
            )
        };
        actions(
            content,
            &[
                toggle,
                (
                    "Remove",
                    Variant::Destructive,
                    available.then_some(command(Action::Remove(item.id))),
                ),
            ],
        )
    })
}
