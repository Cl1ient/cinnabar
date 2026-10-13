//! A capped collection factory keeps the newest form row and its response cursor.

use json_ui::{
    ActionElement, ActionForm, Catalog, Context, FormButton, FormModel, LayoutEnv, TextMeasure,
    TextureMeta, TextureSource, render_form,
};
use serde_json::json;

struct Metrics;

impl TextMeasure for Metrics {
    fn extent(&self, text: &str) -> [f64; 2] {
        [text.len() as f64, 10.0]
    }
}

impl TextureSource for Metrics {
    fn texture(&self, _: &str) -> Option<TextureMeta> {
        None
    }
}

#[test]
fn capped_form_factory_preserves_the_last_button_response_index() {
    let mut catalog = Catalog::default();
    catalog.overlay_text(
        "ui/server_form.json",
        &json!({
            "namespace": "server_form",
            "third_party_server_screen": {
                "type": "screen", "$screen_content": "server_form.content"
            },
            "content": {"type": "panel", "controls": [{
                "server_form_factory": {
                    "type": "factory", "control_ids": {"long_form": "server_form.long_form"}
                }
            }]},
            "long_form": {"type": "panel", "controls": [{"back": {
                "type": "collection_panel", "collection_name": "form_buttons",
                "factory": {
                    "name": "back", "control_name": "server_form.back_button", "max_children_size": 1
                },
                "bindings": [{"binding_name": "#form_button_contents", "binding_name_override": "#collection_length"}]
            }}]},
            "back_button": {
                "type": "button", "size": [24, 24],
                "bindings": [
                    {"binding_type": "collection_details", "binding_collection_name": "form_buttons"},
                    {"binding_type": "collection", "binding_collection_name": "form_buttons", "binding_name": "#form_button_text"},
                    {"binding_type": "view", "source_property_name": "(#form_button_text = 'Back')", "target_property_name": "#visible"}
                ],
                "button_mappings": [{"from_button_id": "button.menu_select", "mapping_type": "pressed", "to_button_id": "button.form_button_click"}]
            }
        }).to_string(),
    );
    let form = FormModel::Action(ActionForm {
        title: "Supply shop".into(),
        elements: ["Blocks", "Tools", "Back"]
            .into_iter()
            .map(|text| {
                ActionElement::Button(FormButton {
                    text: text.into(),
                    image: None,
                })
            })
            .collect(),
        ..Default::default()
    });
    let rendered = render_form(
        &form,
        &catalog,
        &Context::desktop(),
        [320.0, 180.0],
        &LayoutEnv {
            text: &Metrics,
            textures: &Metrics,
        },
    )
    .expect("original form fixture resolves");
    let back = rendered
        .hits
        .iter()
        .find(|hit| hit.pressed.as_deref() == Some("button.form_button_click"))
        .expect("the final Back row must remain clickable");
    assert_eq!(back.collection.as_deref(), Some("form_buttons"));
    assert_eq!(back.collection_index, Some(2));
}
