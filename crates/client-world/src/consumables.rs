//! Which vanilla items are eaten or drunk, for the use presentation (arm pose, crumbs, sounds)
//! that vanilla drives from an item's use animation.

/// How a consumable is used: eaten food scatters crumbs, drinks do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsumeAnimation {
    Eat,
    Drink,
}

const DRINKS: [&str; 4] = [
    "minecraft:potion",
    "minecraft:milk_bucket",
    "minecraft:ominous_bottle",
    "minecraft:honey_bottle",
];

const FOODS: &[&str] = &[
    "minecraft:apple",
    "minecraft:golden_apple",
    "minecraft:enchanted_golden_apple",
    "minecraft:baked_potato",
    "minecraft:potato",
    "minecraft:poisonous_potato",
    "minecraft:carrot",
    "minecraft:golden_carrot",
    "minecraft:beetroot",
    "minecraft:beetroot_soup",
    "minecraft:bread",
    "minecraft:cookie",
    "minecraft:melon_slice",
    "minecraft:pumpkin_pie",
    "minecraft:mushroom_stew",
    "minecraft:rabbit_stew",
    "minecraft:suspicious_stew",
    "minecraft:beef",
    "minecraft:cooked_beef",
    "minecraft:porkchop",
    "minecraft:cooked_porkchop",
    "minecraft:chicken",
    "minecraft:cooked_chicken",
    "minecraft:mutton",
    "minecraft:cooked_mutton",
    "minecraft:rabbit",
    "minecraft:cooked_rabbit",
    "minecraft:cod",
    "minecraft:cooked_cod",
    "minecraft:salmon",
    "minecraft:cooked_salmon",
    "minecraft:tropical_fish",
    "minecraft:pufferfish",
    "minecraft:rotten_flesh",
    "minecraft:spider_eye",
    "minecraft:dried_kelp",
    "minecraft:sweet_berries",
    "minecraft:glow_berries",
    "minecraft:chorus_fruit",
];

/// The use animation of the vanilla consumable `identifier`, or `None` for other items.
#[must_use]
pub fn consume_animation(identifier: &str) -> Option<ConsumeAnimation> {
    if DRINKS.contains(&identifier) {
        Some(ConsumeAnimation::Drink)
    } else if FOODS.contains(&identifier) {
        Some(ConsumeAnimation::Eat)
    } else {
        None
    }
}
