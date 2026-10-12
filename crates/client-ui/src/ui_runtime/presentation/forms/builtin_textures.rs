//! Original artwork shipped with the built-in Java HUD pack.

/// Finds a built-in image by its extensionless pack path, below server replacements.
pub(super) fn image(path: &str) -> Option<&'static [u8]> {
    match path {
        "textures/ui/hotbar_offhand_left" => Some(include_bytes!(
            "../../../../../../assets/java-hud/textures/ui/hotbar_offhand_left.png"
        )),
        "textures/ui/hotbar_offhand_right" => Some(include_bytes!(
            "../../../../../../assets/java-hud/textures/ui/hotbar_offhand_right.png"
        )),
        _ => None,
    }
}
