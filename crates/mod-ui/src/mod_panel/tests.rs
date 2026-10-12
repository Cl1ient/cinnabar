use super::*;
use crate::test_support::sample_panel as panel;

/// Reopening unchanged content clears settled hover and restores a toggle closed mid-transition.
#[test]
fn reopening_panel_restores_settled_feedback_bindings() {
    use ui::mod_panel::Theme;

    for theme in [Theme::Default, Theme::Monochrome] {
        for enabled in [false, true] {
            let mut settings = panel();
            settings.motion = true;
            settings.theme = theme;
            if let Control::Toggle { value, .. } = &mut settings.controls[0] {
                *value = !enabled;
            }
            let mut ui = ModUi::default();
            ui.set_mod_panel(Some(&settings)).unwrap();
            ui.set_mod_panel_open(true);
            {
                let current = ui.mod_panel.as_mut().unwrap();
                current.viewport = [600.0, 400.0];
                current
                    .motion
                    .advance(&current.panel, &mut current.view, Some(0), 1.0);
                current
                    .motion
                    .advance(&current.panel, &mut current.view, Some(0), 1.2);
                current
                    .motion
                    .write(&current.panel, Arc::make_mut(&mut current.data));
            }
            if let Control::Toggle { value, .. } = &mut settings.controls[0] {
                *value = enabled;
            }
            ui.set_mod_panel(Some(&settings)).unwrap();
            let settled = Arc::clone(&ui.mod_panel.as_ref().unwrap().data);
            {
                let current = ui.mod_panel.as_mut().unwrap();
                current
                    .motion
                    .advance(&current.panel, &mut current.view, Some(0), 1.2);
                current
                    .motion
                    .advance(&current.panel, &mut current.view, Some(0), 1.24);
                current
                    .motion
                    .write(&current.panel, Arc::make_mut(&mut current.data));
                assert_ne!(current.data, settled);
            }
            assert!(ui.mod_panel_motion_active());

            ui.set_mod_panel_open(false);
            ui.set_mod_panel_open(true);
            ui.set_mod_panel(Some(&settings)).unwrap();

            let reopened = ui.mod_panel.as_ref().unwrap();
            assert_eq!(reopened.viewport, [600.0, 400.0]);
            assert_eq!(reopened.data, settled);
        }
    }
}

/// Bound labels are values rather than static geometry and cannot interrupt a captured drag.
#[test]
fn relabeled_controls_keep_the_retained_catalog_shape() {
    let original = panel();
    let mut changed = original.clone();
    if let Control::Toggle { label, value, .. } = &mut changed.controls[0] {
        *label = "Updated title".into();
        *value = true;
    }
    assert!(template::same_shape(&original, &changed));
    assert!(changed.validate_values().is_ok());
}

#[test]
fn compact_module_cards_align_and_wrap_without_losing_bindings() {
    use ui::mod_panel::{Icon, Section, Style};
    let mut settings = panel();
    settings.style = Style::Compact;
    settings.controls.clear();
    settings.sections = (0..3)
        .map(|index| {
            settings.controls.extend([
                Control::Toggle {
                    id: format!("enabled{index}"),
                    label: "Enabled".into(),
                    value: false,
                },
                Control::Keybind {
                    id: format!("binding{index}"),
                    label: "Keybind".into(),
                    key: "F8".into(),
                    capturing: false,
                },
            ]);
            Section {
                id: format!("section{index}"),
                label: "Module".into(),
                category: "Modules".into(),
                icon: Icon::Pointer,
                toggle: Some(format!("enabled{index}")),
                controls: vec![format!("binding{index}")],
            }
        })
        .collect();
    let wide = layout::Layout::new(&settings, [600., 400.], 0, 10);
    assert_eq!(wide.pages.len(), 1);
    assert_eq!(wide.pages[0].len(), 3);
    let baseline = wide.pages[0][0].offset[1] + wide.pages[0][0].height;
    assert!(
        wide.pages[0]
            .iter()
            .all(|card| card.offset[1] + card.height == baseline)
    );
    let narrow = layout::Layout::new(&settings, [260., 200.], 0, 3);
    let bindings: Vec<_> = narrow
        .pages
        .iter()
        .flatten()
        .flat_map(|card| &card.controls)
        .collect();
    assert_eq!(bindings.len(), 3);
    assert!(
        narrow
            .pages
            .iter()
            .flatten()
            .all(|card| card.offset[0] + narrow.card_width <= narrow.width)
    );
}

#[test]
fn keycaps_use_readable_ascii_names_for_modifiers_and_extended_keys() {
    for (physical, visible) in [
        ("KeyR", "R"),
        ("Digit9", "9"),
        ("ControlRight", "RCtrl"),
        ("ShiftLeft", "LShift"),
        ("NumpadDivide", "Num/"),
        ("ArrowRight", "Right"),
        ("PrintScreen", "PrtSc"),
        ("Numpad9", "Num9"),
    ] {
        assert_eq!(data::key_label(physical), visible);
        assert!(visible.is_ascii());
    }
}

#[test]
fn long_numeric_draft_keeps_caret_visible_at_both_ends() {
    let text = "123456789012345678901234";
    assert_eq!(edit::visible_draft(text, false, 0, 42.), "|123456");
    assert_eq!(edit::visible_draft(text, false, 24, 42.), "901234|");
}
