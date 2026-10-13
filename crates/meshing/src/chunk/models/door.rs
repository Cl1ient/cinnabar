use super::*;
use assets::door::DoorPose;

/// Combines the lower selectors and upper hinge only within one admitted door class.
pub(super) fn select_template<'a>(
    context: PaletteResolutionContext<'_, 'a>,
    facts: &PaletteFacts<'a>,
    neighbour_facts: &[OnceCell<PaletteFacts<'a>>; Face::ALL.len()],
    coordinate: [usize; 3],
    entry: ResolvedPaletteEntry,
) -> Option<u32> {
    let pose = DoorPose::from_variant(entry.variant)?;
    let neighbor = adjacent_palette_entry(
        context,
        facts,
        neighbour_facts,
        coordinate,
        if pose.upper {
            Face::NegativeY
        } else {
            Face::PositiveY
        },
    );
    if model_template_flags(context.visuals, neighbor) != assets::MODEL_TEMPLATE_FLAG_DOOR {
        return None;
    }
    assets::door::paired_template(
        context.visuals.model_templates(),
        entry.model_template,
        entry.variant,
        neighbor.model_template,
        neighbor.variant,
    )
}
