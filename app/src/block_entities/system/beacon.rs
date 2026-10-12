//! Beacon beams: their height to the build limit and the tint of glass above them.

use render_model::{BeaconModel, BlockEntityKind};

use super::BlockEntityRuntime;

/// Highest world Y a beacon beam is drawn to; the beam stops at the build limit.
const BEAM_TOP: i32 = 320;

/// Blends the tint of each stained-glass block above the beacon, each new pane averaging
/// with the color so far.
fn beam_tint(glass: impl IntoIterator<Item = [f32; 3]>) -> [f32; 3] {
    glass.into_iter().fold([1.0; 3], |tint, color| {
        if tint == [1.0; 3] {
            color
        } else {
            std::array::from_fn(|axis| (tint[axis] + color[axis]) * 0.5)
        }
    })
}

/// The beam a beacon at `position` draws, tinted from compiled descriptors of the blocks above it.
pub(super) fn kind(
    runtime: &mut BlockEntityRuntime,
    store: &world::ChunkStore,
    dimension: i32,
    mode: assets::NetworkIdMode,
    position: [i32; 3],
) -> Option<BlockEntityKind> {
    let height = u32::try_from(BEAM_TOP - position[1] - 1)
        .ok()
        .filter(|height| *height > 0)?;
    let mut glass = Vec::new();
    for y in position[1] + 1..BEAM_TOP {
        let runtime_id = store
            .sub_chunk(world::SubChunkKey::new(
                dimension,
                position[0].div_euclid(16),
                y.div_euclid(16),
                position[2].div_euclid(16),
            ))
            .and_then(|sub_chunk| {
                sub_chunk.runtime_id(
                    0,
                    position[0].rem_euclid(16) as u8,
                    y.rem_euclid(16) as u8,
                    position[2].rem_euclid(16) as u8,
                )
            });
        if let Some(colour) =
            runtime_id.and_then(|id| runtime.beacon_tints.get(mode, id).dye_colour())
        {
            glass.push(assets::banner::color_linear(colour));
        }
    }
    Some(BlockEntityKind::Beacon(BeaconModel {
        height,
        tint: beam_tint(glass),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_tint_averages_each_new_pane_with_the_color_so_far() {
        assert_eq!(beam_tint([]), [1.0; 3]);
        let red = [1.0, 0.0, 0.0];
        let blue = [0.0, 0.0, 1.0];
        assert_eq!(beam_tint([red]), red);
        assert_eq!(beam_tint([red, blue]), [0.5, 0.0, 0.5]);
        assert_eq!(assets::BeaconTint::Red.dye_colour(), Some(1));
        assert_eq!(assets::BeaconTint::LightGray.dye_colour(), Some(7));
        assert_eq!(assets::BeaconTint::NoTinting.dye_colour(), None);
    }
}
