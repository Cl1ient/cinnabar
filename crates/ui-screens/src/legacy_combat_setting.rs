//! Optional Bedrock 1.12 combat presentation in Video settings.

use json_ui::Catalog;
use launcher::menu::settings_options::{
    ANIMATIONS_OPTION, LEGACY_112_COMBAT_HELP, LEGACY_112_COMBAT_OPTION,
};
use serde_json::json;

/// Adds the combat toggle and its precedence explanation beside Animations.
pub fn install(catalog: &mut Catalog) {
    let option = LEGACY_112_COMBAT_OPTION;
    let name = option.name;
    catalog.overlay_text(
        "ui/cinnabar_legacy_combat.json",
        &json!({
            "namespace": "general_section",
            "video_section": {"modifications": [{
                "array_name": "controls", "operation": "insert_after",
                "control_name": ANIMATIONS_OPTION.name,
                "value": [
                    {format!("{name}@settings_common.option_toggle"): {
                        "$option_label": option.label,
                        "$option_binding_name": format!("#{name}"),
                        "$option_enabled_binding_name": format!("#{name}_enabled"),
                        "$toggle_name": name,
                        "$focus_override_right": "FOCUS_OVERRIDE_STOP"
                    }},
                    {"legacy_combat_help": {
                        "type": "label",
                        "text": LEGACY_112_COMBAT_HELP,
                        "size": ["100% - 8px", "default"],
                        "text_alignment": "left",
                        "anchor_from": "top_left",
                        "anchor_to": "top_left",
                        "color": "$settings_text_color",
                        "localize": false
                    }}
                ]
            }]}
        })
        .to_string(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_setting_explains_precedence_and_binds_the_persisted_option() {
        let mut catalog = Catalog::default();
        catalog.overlay_text(
            "ui/general_section.json",
            &json!({
                "namespace": "general_section",
                "video_section": {"type":"stack_panel", "controls":[
                    {ANIMATIONS_OPTION.name: {"type":"panel"}}
                ]}
            })
            .to_string(),
        );
        catalog.overlay_text(
            "ui/settings_common.json",
            r#"{
            "namespace":"settings_common", "option_toggle": {
                "type":"toggle", "text":"$option_label", "toggle_name":"$toggle_name"
            }
        }"#,
        );
        install(&mut catalog);
        let section = json_ui::resolve(
            &catalog,
            "general_section.video_section",
            &json_ui::Context::default(),
        )
        .control
        .unwrap();
        let toggle = section
            .children
            .iter()
            .find(|child| child.name == LEGACY_112_COMBAT_OPTION.name)
            .unwrap();
        assert_eq!(toggle.properties["text"], LEGACY_112_COMBAT_OPTION.label);
        assert_eq!(
            toggle.properties["toggle_name"],
            LEGACY_112_COMBAT_OPTION.name
        );
        assert!(
            section
                .children
                .iter()
                .any(|child| child.properties.get("text") == Some(&json!(LEGACY_112_COMBAT_HELP)))
        );
    }
}
