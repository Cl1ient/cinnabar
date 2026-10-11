//! The implemented renderer contracts used by JSON-UI custom controls.

use super::{hud_renderers, item_renderer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HudRenderer {
    Heart,
    Armor,
    Hunger,
    Bubbles,
    HorseHeart,
    Effects,
    Hotbar,
    HorseJump,
    BossNotches,
    Cursor,
    HotbarCooldown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRenderer {
    Hud(HudRenderer),
    Tooltip,
    Credits,
    VectorIcon,
    RoundedRectangle,
    InventoryItem,
    ProgressBar,
    Gradient,
    AnimatedGif,
    ProfileImage,
    Player,
    SplashText,
    NameTag,
    BannerPattern,
}

impl NativeRenderer {
    /// Returns a supported drawing contract; unknown or unimplemented renderers stay absent.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "heart_renderer" => Self::Hud(HudRenderer::Heart),
            "armor_renderer" => Self::Hud(HudRenderer::Armor),
            "hunger_renderer" => Self::Hud(HudRenderer::Hunger),
            "bubbles_renderer" => Self::Hud(HudRenderer::Bubbles),
            "horse_heart_renderer" => Self::Hud(HudRenderer::HorseHeart),
            "mob_effects_renderer" => Self::Hud(HudRenderer::Effects),
            "hotbar_renderer" => Self::Hud(HudRenderer::Hotbar),
            "horse_jump_renderer" => Self::Hud(HudRenderer::HorseJump),
            "java_boss_notches" => Self::Hud(HudRenderer::BossNotches),
            "cursor_renderer" => Self::Hud(HudRenderer::Cursor),
            "hotbar_cooldown_renderer" => Self::Hud(HudRenderer::HotbarCooldown),
            super::tooltip::RENDERER => Self::Tooltip,
            super::credits_renderer::RENDERER => Self::Credits,
            "cinnabar_vector_icon" => Self::VectorIcon,
            "cinnabar_rounded_rectangle" => Self::RoundedRectangle,
            "inventory_item_renderer" => Self::InventoryItem,
            "progress_bar_renderer" => Self::ProgressBar,
            "gradient_renderer" => Self::Gradient,
            "animated_gif_renderer" => Self::AnimatedGif,
            "profile_image_renderer" => Self::ProfileImage,
            "live_player_renderer" | "paper_doll_renderer" | "hud_player_renderer" => Self::Player,
            "splash_text_renderer" => Self::SplashText,
            "name_tag_renderer" => Self::NameTag,
            "banner_pattern_renderer" => Self::BannerPattern,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unimplemented_native_renderers_are_not_advertised_as_drawable() {
        for name in [
            "dash_renderer",
            "locator_bar",
            "vignette_renderer",
            "progress_indicator_renderer",
            "camera_renderer",
            "editor_gizmo_renderer",
            "editor_compass_renderer",
            "editor_volume_highlight_renderer",
            "custom:unknown",
        ] {
            assert_eq!(NativeRenderer::parse(name), None, "{name}");
        }
        assert_eq!(
            NativeRenderer::parse("banner_pattern_renderer"),
            Some(NativeRenderer::BannerPattern)
        );
        assert_eq!(
            NativeRenderer::parse("heart_renderer"),
            Some(NativeRenderer::Hud(HudRenderer::Heart))
        );
    }
}

use ui::UiVisual;

impl super::Painter<'_> {
    /// Bridge the custom renderers screens use: item icons, the durability bar, the
    /// player preview, tooltips, and the HUD's native renderers. Unsupported contracts are reported by the frame.
    pub(super) fn custom(
        &mut self,
        key: &str,
        renderer: &str,
        data: &std::collections::BTreeMap<String, serde_json::Value>,
        dest: [f32; 4],
        alpha: impl Fn([u8; 4]) -> [u8; 4],
    ) -> Option<(UiVisual, [f32; 4])> {
        let kind = NativeRenderer::parse(renderer)?;
        if let NativeRenderer::Hud(hud_kind) = kind {
            if let Some(hud) = self.art.hud {
                hud_renderers::paint(self, hud, key, hud_kind, data, dest, &alpha);
            }
            return None;
        }
        match kind {
            NativeRenderer::VectorIcon => Some((self.vector_icon(data, dest, &alpha)?, dest)),
            NativeRenderer::RoundedRectangle => {
                Some((self.rounded_rectangle(data, dest, &alpha)?, dest))
            }
            NativeRenderer::Credits => {
                self.credits(dest, &alpha);
                None
            }
            NativeRenderer::InventoryItem => {
                let icon = item_renderer::icon(data, self.art.icons, self.art.id_aux)?;
                Some((icon.visual(alpha([255; 4])), dest))
            }
            NativeRenderer::ProgressBar => {
                self.progress_bar(data, dest, &alpha);
                None
            }
            NativeRenderer::Gradient => Some((self.gradient(data, &alpha)?, dest)),
            NativeRenderer::AnimatedGif => self.animated_gif(data, dest, &alpha),
            NativeRenderer::ProfileImage => {
                // A friend row names its own gamerpic; elsewhere it is the player's.
                let portrait = match data.get("#profile_image_options") {
                    Some(serde_json::Value::String(path)) => *self.art.images?.get(path)?,
                    _ => self.art.portrait?,
                };
                Some((
                    UiVisual::Sprite {
                        texture_page: portrait.page,
                        uv: portrait.uv,
                        color: alpha([255; 4]),
                    },
                    dest,
                ))
            }
            NativeRenderer::Player => self.player_preview(renderer, data, dest, &alpha),
            NativeRenderer::SplashText => {
                self.splash(dest, &alpha);
                None
            }
            NativeRenderer::NameTag => self.name_tag(data, dest, &alpha),
            NativeRenderer::BannerPattern => {
                self.banner_pattern(data, dest, &alpha);
                None
            }
            NativeRenderer::Tooltip | NativeRenderer::Hud(_) => None,
        }
    }
}
