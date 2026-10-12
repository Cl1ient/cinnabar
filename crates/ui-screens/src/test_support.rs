//! Layout fixtures for tests that lay screens out without a carrier's fonts or atlas.

/// Measures text at six px per character and nine px tall.
pub struct FixedText;
impl json_ui::TextMeasure for FixedText {
    fn extent(&self, text: &str) -> [f64; 2] {
        [text.chars().count() as f64 * 6.0, 9.0]
    }
}

/// Answers every texture as a plain 16 px square.
pub struct NoTextures;
impl json_ui::TextureSource for NoTextures {
    fn texture(&self, _: &str) -> Option<json_ui::TextureMeta> {
        Some(json_ui::TextureMeta {
            base_size: [16.0, 16.0],
            pixels: [16.0, 16.0],
            nineslice: None,
        })
    }
}
