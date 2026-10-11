//! Typed beacon column rules and colour sections.

/// The beam continues through this world Y after reaching open sky.
pub const BEACON_SKY_TOP_Y: i32 = 2048;

/// One contiguous beam segment, counted from the previous segment's end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeaconBeamSection {
    pub height: u32,
    /// Normalized dye channels before conversion for the rendering target.
    pub tint: [f32; 3],
}

/// A block's admitted effect on the beam above it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BeaconMedium {
    Clear,
    Tint([f32; 3]),
    Obstruction,
}

const TINTED_GLASS: [(i64, [&str; 4]); 16] = [
    (
        15,
        [
            "minecraft:white_stained_glass",
            "minecraft:hard_white_stained_glass",
            "minecraft:white_stained_glass_pane",
            "minecraft:hard_white_stained_glass_pane",
        ],
    ),
    (
        14,
        [
            "minecraft:orange_stained_glass",
            "minecraft:hard_orange_stained_glass",
            "minecraft:orange_stained_glass_pane",
            "minecraft:hard_orange_stained_glass_pane",
        ],
    ),
    (
        13,
        [
            "minecraft:magenta_stained_glass",
            "minecraft:hard_magenta_stained_glass",
            "minecraft:magenta_stained_glass_pane",
            "minecraft:hard_magenta_stained_glass_pane",
        ],
    ),
    (
        12,
        [
            "minecraft:light_blue_stained_glass",
            "minecraft:hard_light_blue_stained_glass",
            "minecraft:light_blue_stained_glass_pane",
            "minecraft:hard_light_blue_stained_glass_pane",
        ],
    ),
    (
        11,
        [
            "minecraft:yellow_stained_glass",
            "minecraft:hard_yellow_stained_glass",
            "minecraft:yellow_stained_glass_pane",
            "minecraft:hard_yellow_stained_glass_pane",
        ],
    ),
    (
        10,
        [
            "minecraft:lime_stained_glass",
            "minecraft:hard_lime_stained_glass",
            "minecraft:lime_stained_glass_pane",
            "minecraft:hard_lime_stained_glass_pane",
        ],
    ),
    (
        9,
        [
            "minecraft:pink_stained_glass",
            "minecraft:hard_pink_stained_glass",
            "minecraft:pink_stained_glass_pane",
            "minecraft:hard_pink_stained_glass_pane",
        ],
    ),
    (
        8,
        [
            "minecraft:gray_stained_glass",
            "minecraft:hard_gray_stained_glass",
            "minecraft:gray_stained_glass_pane",
            "minecraft:hard_gray_stained_glass_pane",
        ],
    ),
    (
        7,
        [
            "minecraft:light_gray_stained_glass",
            "minecraft:hard_light_gray_stained_glass",
            "minecraft:light_gray_stained_glass_pane",
            "minecraft:hard_light_gray_stained_glass_pane",
        ],
    ),
    (
        6,
        [
            "minecraft:cyan_stained_glass",
            "minecraft:hard_cyan_stained_glass",
            "minecraft:cyan_stained_glass_pane",
            "minecraft:hard_cyan_stained_glass_pane",
        ],
    ),
    (
        5,
        [
            "minecraft:purple_stained_glass",
            "minecraft:hard_purple_stained_glass",
            "minecraft:purple_stained_glass_pane",
            "minecraft:hard_purple_stained_glass_pane",
        ],
    ),
    (
        4,
        [
            "minecraft:blue_stained_glass",
            "minecraft:hard_blue_stained_glass",
            "minecraft:blue_stained_glass_pane",
            "minecraft:hard_blue_stained_glass_pane",
        ],
    ),
    (
        3,
        [
            "minecraft:brown_stained_glass",
            "minecraft:hard_brown_stained_glass",
            "minecraft:brown_stained_glass_pane",
            "minecraft:hard_brown_stained_glass_pane",
        ],
    ),
    (
        2,
        [
            "minecraft:green_stained_glass",
            "minecraft:hard_green_stained_glass",
            "minecraft:green_stained_glass_pane",
            "minecraft:hard_green_stained_glass_pane",
        ],
    ),
    (
        1,
        [
            "minecraft:red_stained_glass",
            "minecraft:hard_red_stained_glass",
            "minecraft:red_stained_glass_pane",
            "minecraft:hard_red_stained_glass_pane",
        ],
    ),
    (
        0,
        [
            "minecraft:black_stained_glass",
            "minecraft:hard_black_stained_glass",
            "minecraft:black_stained_glass_pane",
            "minecraft:hard_black_stained_glass_pane",
        ],
    ),
];

/// Resolves exact ordinary and hardened glass identities; other blocks never tint.
pub fn beacon_glass_tint(identifier: &str) -> Option<[f32; 3]> {
    let (dye, _) = TINTED_GLASS
        .iter()
        .find(|(_, names)| names.contains(&identifier))?;
    Some(if *dye == 15 {
        [1.0; 3]
    } else {
        crate::banner::color_rgb(*dye).map(|value| f32::from(value) / 255.0)
    })
}

/// Classifies one block using block-owned tint eligibility and light filtering.
pub fn beacon_medium(identifier: &str, light_filter: u8) -> BeaconMedium {
    if light_filter >= 15 && identifier != "minecraft:bedrock" {
        BeaconMedium::Obstruction
    } else if let Some(tint) = beacon_glass_tint(identifier) {
        BeaconMedium::Tint(tint)
    } else {
        BeaconMedium::Clear
    }
}

/// Scans a changed column once. Obstruction removes the whole beam; tint blocks split
/// successive colours, and open sky extends the last section to the sky limit.
pub fn beacon_sections(
    first_y: i32,
    world_top_exclusive: i32,
    mut sample: impl FnMut(i32) -> BeaconMedium,
) -> Box<[BeaconBeamSection]> {
    if first_y > BEACON_SKY_TOP_Y || first_y > world_top_exclusive {
        return Box::default();
    }
    let mut sections = Vec::new();
    let mut start = first_y;
    let mut tint = [1.0; 3];
    let mut first_tint = true;
    for y in first_y..world_top_exclusive.min(BEACON_SKY_TOP_Y + 1) {
        match sample(y) {
            BeaconMedium::Clear => {}
            BeaconMedium::Obstruction => return Box::default(),
            BeaconMedium::Tint(next) => {
                sections.push(BeaconBeamSection {
                    height: (y + 1 - start) as u32,
                    tint,
                });
                tint = if first_tint {
                    next
                } else {
                    std::array::from_fn(|axis| (tint[axis] + next[axis]) * 0.5)
                };
                first_tint = false;
                start = y + 1;
            }
        }
    }
    if start <= BEACON_SKY_TOP_Y {
        sections.push(BeaconBeamSection {
            height: (BEACON_SKY_TOP_Y + 2 - start) as u32,
            tint,
        });
    }
    sections.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_sections_preserve_lower_colours_and_continue_above_world_height() {
        let red = [1.0, 0.0, 0.0];
        let blue = [0.0, 0.0, 1.0];
        let sections = beacon_sections(64, 320, |y| match y {
            66 => BeaconMedium::Tint(red),
            68 => BeaconMedium::Tint(blue),
            _ => BeaconMedium::Clear,
        });
        assert_eq!(
            sections.as_ref(),
            &[
                BeaconBeamSection {
                    height: 3,
                    tint: [1.0; 3]
                },
                BeaconBeamSection {
                    height: 2,
                    tint: red
                },
                BeaconBeamSection {
                    height: 1981,
                    tint: [0.5, 0.0, 0.5]
                },
            ]
        );
        assert!(
            beacon_sections(64, 320, |y| if y == 70 {
                BeaconMedium::Obstruction
            } else {
                BeaconMedium::Clear
            })
            .is_empty()
        );
    }

    #[test]
    fn top_layer_beacons_extend_to_sky_without_sampling_blocks() {
        for world_top in [128, 256, 320] {
            let sections = beacon_sections(world_top, world_top, |_| {
                panic!("a top-layer beacon has no blocks above it to sample")
            });
            assert_eq!(
                sections.as_ref(),
                &[BeaconBeamSection {
                    height: (BEACON_SKY_TOP_Y + 2 - world_top) as u32,
                    tint: [1.0; 3],
                }]
            );
        }
    }

    #[test]
    fn beacons_above_the_sky_limit_remain_empty_without_sampling() {
        assert!(
            beacon_sections(BEACON_SKY_TOP_Y + 1, BEACON_SKY_TOP_Y + 2, |_| {
                panic!("a beacon above the sky limit must not sample blocks")
            })
            .is_empty()
        );
    }

    #[test]
    fn hardened_glass_and_bedrock_use_exact_block_rules() {
        assert_eq!(
            beacon_medium("minecraft:hard_red_stained_glass", 0),
            beacon_medium("minecraft:red_stained_glass", 0)
        );
        assert_eq!(
            beacon_medium("minecraft:hard_blue_stained_glass_pane", 0),
            beacon_medium("minecraft:blue_stained_glass_pane", 0)
        );
        assert_eq!(beacon_medium("minecraft:bedrock", 15), BeaconMedium::Clear);
        assert_eq!(
            beacon_medium("minecraft:stone", 15),
            BeaconMedium::Obstruction
        );
        assert_eq!(
            beacon_medium("custom:red_stained_glass", 0),
            BeaconMedium::Clear
        );
    }
}
