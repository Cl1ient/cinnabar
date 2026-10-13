//! Gameplay touch controls drawn through JSON-UI; the painted rectangles are the touch regions.

use std::sync::Arc;

use json_ui::{Catalog, Context, DataSource, HitRegion, Scalar, ViewState, hit_test};
use semantic_input::{TouchBounds, TouchContact, TouchControlRegion, touch};
use ui::{FONT_DESIGN_PIXEL_TEXELS, UiNode, UiPoint};
use ui_engine::{
    cached_screen::CachedScreen,
    engine::{ScreenArt, screen::PaintFrame},
    frame::EngineFrame,
    text_metrics::TextMetrics,
};

use super::super::{UiPresentationError, UiPresentationRuntime};
use crate::ui_runtime::UiRuntime;

const SCREEN: &str = "cinnabar_touch.screen";
const LAYOUT: &[u8] = include_bytes!("touch_controls.json");
/// Each control's name in the layout, which its `cinnabar.touch.<name>` button id and
/// `#touch_<name>_pressed` binding carry.
const CONTROLS: &[(&str, u16)] = &[
    ("look", touch::LOOK_SURFACE),
    ("joystick", touch::JOYSTICK),
    ("jump", touch::JUMP),
    ("sneak", touch::SNEAK),
    ("sprint", touch::SPRINT),
    ("attack", touch::ATTACK),
    ("use", touch::USE),
    ("menu", touch::MENU),
    ("inventory", touch::INVENTORY),
];

#[derive(Default)]
pub(in super::super) struct TouchControls {
    enabled: bool,
    catalog: Option<Arc<Catalog>>,
    screen: CachedScreen,
    /// The overlay and the HUD as last painted, which touches resolve against.
    frame: Option<EngineFrame>,
    pub(super) hud: Option<EngineFrame>,
    /// Held contacts, which press their controls and move the stick's knob.
    contacts: Vec<TouchContact>,
}

impl UiPresentationRuntime {
    /// Shows the gameplay touch controls; platforms without a touchscreen leave them off.
    pub fn enable_touch_controls(&mut self, enabled: bool) {
        self.screens.touch.enabled = enabled;
    }

    pub fn touch_controls_enabled(&self) -> bool {
        self.screens.touch.enabled
    }

    pub fn set_touch_contacts(&mut self, contacts: Vec<TouchContact>) {
        self.screens.touch.contacts = contacts;
    }

    /// The control under a normalized `position` in a `window` of logical pixels: a hotbar slot,
    /// then an overlay control, whose bounds are the painted rectangle.
    pub fn gameplay_touch_region(
        &self,
        position: [f32; 2],
        window: [f32; 2],
    ) -> Option<TouchControlRegion> {
        let state = &self.screens.touch;
        if !state.enabled || window.iter().any(|size| !size.is_finite() || *size <= 0.0) {
            return None;
        }
        let point = UiPoint::new(position[0] * window[0], position[1] * window[1]).ok()?;
        if let Some(frame) = &state.hud
            && let Some(slot) = hit_test(&frame.hits, frame.to_virtual(point))
                .filter(|hit| hit.collection.as_deref() == Some("hotbar_items"))
        {
            let index = slot.collection_index.filter(|index| *index < 9)?;
            return region_bounds(frame, slot, window, touch::HOTBAR_1 + index as u16);
        }
        let frame = state.frame.as_ref()?;
        let hit = hit_test(&frame.hits, frame.to_virtual(point))?;
        let hit_id = control_id(hit.pressed.as_deref())
            .or_else(|| (hit.name == "knob").then_some(touch::JOYSTICK))?;
        // The knob moves, so the stick's activation area is its frame.
        let region = if hit_id == touch::JOYSTICK {
            frame
                .hits
                .iter()
                .find(|region| control_id(region.pressed.as_deref()) == Some(hit_id))?
        } else {
            hit
        };
        region_bounds(frame, region, window, hit_id)
    }

    pub(in super::super) fn append_touch_controls(
        &mut self,
        runtime: &UiRuntime,
        nodes: &mut Vec<UiNode>,
        next: &mut u32,
        metrics: TextMetrics,
        content: [f32; 2],
    ) -> Result<(), UiPresentationError> {
        let state = &mut self.screens.touch;
        if !state.enabled {
            return Ok(());
        }
        let Some(renderer) = self.engine.as_deref() else {
            return Ok(());
        };
        let catalog = match &state.catalog {
            Some(catalog) => Arc::clone(catalog),
            None => {
                let catalog = Arc::new(layout_catalog().expect("built-in touch layout parses"));
                state.catalog = Some(Arc::clone(&catalog));
                catalog
            }
        };
        let mut view = ViewState::default();
        if let Some(previous) = &state.frame
            && let Some(stick) = state
                .contacts
                .iter()
                .find(|contact| contact.hit_id == Some(touch::JOYSTICK))
            && let Some(knob) = previous.hits.iter().find(|hit| hit.name == "knob")
        {
            view.drags.insert(
                knob.key.clone(),
                [
                    f64::from(stick.delta[0]) * knob.rect.w,
                    -f64::from(stick.delta[1]) * knob.rect.h,
                ],
            );
        }
        let mut data = DataSource::new();
        for &(name, id) in CONTROLS {
            let held = state
                .contacts
                .iter()
                .any(|contact| contact.hit_id == Some(id));
            data.set_global(format!("#touch_{name}_pressed"), Scalar::Bool(held));
        }
        let translate = |key: &str| runtime.translation(key);
        let language = runtime.text_generation();
        let mut paint = PaintFrame {
            layouts: &mut self.layouts,
            font: &self.font,
            metrics,
            solid_page: self.solid_texture_page,
            safe_area: self.safe_area,
            content,
            translate: &translate,
            language,
            now: self.menu_seconds,
            nodes,
            next,
            renderers: &mut [],
        };
        let px = metrics.scale.get() * FONT_DESIGN_PIXEL_TEXELS as f32;
        let art = ScreenArt {
            view: Some(&view),
            ..ScreenArt::default()
        };
        let screen = &mut state.screen;
        state.frame = renderer.draw(art, &mut paint, |env, root| {
            screen.render_with(
                SCREEN,
                &catalog,
                &Context::default(),
                data,
                (root, px, language),
                env,
                &view,
            )
        })?;
        Ok(())
    }
}

fn layout_catalog() -> Result<Catalog, json_ui::LoadError> {
    Catalog::from_files([
        ("ui/_global_variables.json", &b"{}"[..]),
        (
            "ui/_ui_defs.json",
            &br#"{"ui_defs":["ui/cinnabar_touch.json"]}"#[..],
        ),
        ("ui/cinnabar_touch.json", LAYOUT),
    ])
}

fn region_bounds(
    frame: &EngineFrame,
    region: &HitRegion,
    window: [f32; 2],
    hit_id: u16,
) -> Option<TouchControlRegion> {
    let left = region.rect.x.max(region.clip.x);
    let top = region.rect.y.max(region.clip.y);
    let right = (region.rect.x + region.rect.w).min(region.clip.x + region.clip.w);
    let bottom = (region.rect.y + region.rect.h).min(region.clip.y + region.clip.h);
    if left >= right || top >= bottom {
        return None;
    }
    let normalized = |x: f64, y: f64| {
        [
            (frame.origin[0] + x as f32 * frame.scale) / window[0],
            (frame.origin[1] + y as f32 * frame.scale) / window[1],
        ]
    };
    Some(TouchControlRegion {
        hit_id,
        bounds: TouchBounds {
            min: normalized(left, top),
            max: normalized(right, bottom),
        },
    })
}

fn control_id(pressed: Option<&str>) -> Option<u16> {
    let name = pressed?.strip_prefix("cinnabar.touch.")?;
    CONTROLS
        .iter()
        .find_map(|&(control, id)| (control == name).then_some(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every overlay control names a layout button, so a renamed control cannot go unreachable.
    #[test]
    fn every_control_is_a_layout_button() {
        let layout = std::str::from_utf8(LAYOUT).unwrap();
        for (name, _) in CONTROLS {
            assert!(
                layout.contains(&format!("\"cinnabar.touch.{name}\"")),
                "{name} has no button in the touch layout"
            );
        }
        layout_catalog().unwrap();
    }
}
