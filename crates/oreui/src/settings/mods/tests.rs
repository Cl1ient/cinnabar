use {
    super::*,
    crate::review_tests::paint,
    launcher::menu::MenuView,
    launcher::mods::Library,
    std::{collections::HashMap, sync::Arc},
};

fn installed(id: u64, name: &str) -> InstalledMod {
    InstalledMod {
        id,
        name: name.into(),
        file: format!("{name}.wasm"),
        bytes: 2048,
    }
}

fn render(snapshot: Snapshot) -> (Vec<MenuAction>, Vec<String>) {
    let mut view = MenuView::new(true, "Player".into());
    view.mods = Arc::new(snapshot);
    let (_, hits, nodes) = paint(HashMap::new(), |canvas| {
        let mut content = Content {
            canvas,
            view: &view,
            span: [100.0, 900.0],
            column_width: 800.0,
            y: 100.0,
            translate: &|_| None,
            nested: false,
        };
        draw(&mut content).unwrap();
    });
    let labels = nodes
        .iter()
        .filter_map(|node| match node.visual() {
            ui::UiVisual::Text { layout, .. } => Some(
                layout
                    .glyphs()
                    .iter()
                    .map(|glyph| glyph.codepoint)
                    .collect(),
            ),
            _ => None,
        })
        .collect();
    (hits.into_iter().map(|(action, _)| action).collect(), labels)
}

/// Without mod support the tab still renders, explains why and offers no import.
#[test]
fn a_build_without_mods_explains_itself_and_disables_import() {
    let (hits, labels) = render(Snapshot::default());
    assert!(labels.iter().any(|label| label == UNSUPPORTED));
    assert!(labels.iter().any(|label| label == "No mods installed"));
    assert!(!hits.contains(&command(Action::Import)));
}

#[test]
fn the_enabled_mod_is_the_active_card_with_its_status_and_controls() {
    let (first, second) = (installed(1, "zoom"), installed(2, "minimap"));
    let snapshot = Snapshot {
        library: Library {
            supported: true,
            installed: vec![first.clone(), second.clone()],
            enabled: Some(second.id),
            status: Some(Status::Running),
            ..Library::default()
        },
        details_expanded: Some(second.id),
        ..Snapshot::default()
    };
    let (hits, labels) = render(snapshot);
    for action in [
        Action::Import,
        Action::ReadMore(first.id),
        Action::ReadMore(second.id),
        Action::Disable,
        Action::Remove(second.id),
    ] {
        assert!(hits.contains(&command(action)), "{action:?}");
    }
    assert!(!hits.contains(&command(Action::Enable(second.id))));
    assert!(!hits.contains(&command(Action::Remove(first.id))));
    for label in [
        "Active mods (1)",
        "My mods (1)",
        "Running",
        "Ready to enable",
    ] {
        assert!(labels.iter().any(|found| found == label), "{label}");
    }
    assert!(!labels.iter().any(|label| label == UNSUPPORTED));
}
