//! Numerical item-use admission shared by gameplay, presentation and consume audio.

use super::*;
use assets::item_use::{
    ItemConsumption, ItemProjectileRole, ItemUseDurationRule, ItemUseKind, ItemUseProfile,
    ItemUseRequirement,
};

/// A category identity belongs to one effective registry admission, never a leaked string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UseCooldownCategory {
    pub generation: u64,
    pub index: u16,
}

/// A resolved category timer is small enough to copy into the accepted gameplay frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedUseCooldown {
    pub category: UseCooldownCategory,
    pub ticks: u32,
}

/// Effective facts cached by numerical network ID; unresolved fields grant no prediction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectiveItemUse {
    pub kind: Option<ItemUseKind>,
    pub ticks: Option<u32>,
    pub requirement: ItemUseRequirement,
    pub consumption: ItemConsumption,
    pub movement: f64,
    pub cooldown: Option<ResolvedUseCooldown>,
    pub projectile: ItemProjectileRole,
}

impl Default for EffectiveItemUse {
    /// Keeps unsupported items distinct from declared generic hold or consumption behavior.
    fn default() -> Self {
        Self {
            kind: None,
            ticks: None,
            requirement: ItemUseRequirement::Nothing,
            consumption: ItemConsumption::Unresolved,
            movement: 1.0,
            cooldown: None,
            projectile: ItemProjectileRole::None,
        }
    }
}

/// Resolves immutable rules outside gameplay; stack-dependent crossbow timing remains separate.
pub fn resolve_item_use(
    profile: &ItemUseProfile,
    declared_ticks: Option<u32>,
    category: Option<UseCooldownCategory>,
) -> EffectiveItemUse {
    let ItemUseProfile::Provisional {
        kind,
        duration,
        requirement,
        consumption,
        movement_bits,
        cooldown,
        projectile,
        ..
    } = profile
    else {
        return EffectiveItemUse {
            ticks: declared_ticks,
            ..Default::default()
        };
    };
    EffectiveItemUse {
        kind: Some(*kind),
        ticks: match duration {
            ItemUseDurationRule::Declared => declared_ticks,
            ItemUseDurationRule::LongWeapon => Some(protocol::LONG_WEAPON_USE_TICKS),
            ItemUseDurationRule::Fixed(ticks) => Some(*ticks),
            ItemUseDurationRule::Crossbow => Some(assets::item_use::CROSSBOW_CHARGE_TICKS),
        },
        requirement: *requirement,
        consumption: *consumption,
        movement: f64::from_bits(*movement_bits),
        cooldown: cooldown
            .as_ref()
            .zip(category)
            .map(|(cooldown, category)| ResolvedUseCooldown {
                category,
                ticks: cooldown.ticks,
            }),
        projectile: *projectile,
    }
}

impl ItemStateStore {
    /// Rebuilds numerical profiles and owned category admissions only when source facts change.
    pub(super) fn rebuild_use_profiles(&mut self) {
        // Category generations prevent a previous registry timer from blocking new definitions.
        static NEXT_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let generation = NEXT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut categories = BTreeMap::<Box<str>, u16>::new();
        let mut profiles = BTreeMap::new();
        for entry in self.registry.values() {
            let mut profile = self.assets.as_ref().map_or_else(
                || assets::item_use::diagnostic_profile(&entry.identifier),
                |assets| assets.item_use_profile(&entry.identifier),
            );
            let mut ticks = self
                .use_durations
                .as_ref()
                .and_then(|durations| durations.get(entry.identifier.as_ref()).copied());
            if let ItemUseProfile::Provisional {
                duration_alias: Some(alias),
                ..
            } = &profile
            {
                ticks = ticks.or_else(|| self.use_durations.as_ref()?.get(alias.as_ref()).copied());
            }
            if let Some(overlay) = self
                .use_overlays
                .as_ref()
                .and_then(|overlays| overlays.get(entry.identifier.as_ref()))
            {
                ticks = overlay.duration_ticks.or(ticks);
                overlay_authored(&mut profile, overlay);
            }
            if let Some(components) = entry.components.as_deref() {
                ticks = components.use_duration_ticks.or(ticks);
                overlay_components(&mut profile, components);
            }
            let category = match &profile {
                ItemUseProfile::Provisional {
                    cooldown: Some(cooldown),
                    ..
                } => {
                    let next =
                        u16::try_from(categories.len()).expect("registry bounds category count");
                    let index = *categories.entry(cooldown.category.clone()).or_insert(next);
                    Some(UseCooldownCategory { generation, index })
                }
                _ => None,
            };
            profiles.insert(
                entry.network_id,
                resolve_item_use(&profile, ticks, category),
            );
        }
        self.use_profiles = profiles;
    }

    /// Retrieves admitted facts without resolving names or parsing component data.
    pub(crate) fn item_use_profile(&self, network_id: i32) -> EffectiveItemUse {
        self.use_profiles
            .get(&network_id)
            .copied()
            .unwrap_or_default()
    }
}

/// Converts parsed wire evidence once before applying the common authored overlay.
fn overlay_components(profile: &mut ItemUseProfile, components: &protocol::ItemComponents) {
    let overlay = assets::CompiledItemUseOverlay {
        identifier: Box::from(""),
        duration_ticks: components.use_duration_ticks,
        food_can_always_eat: components.food_can_always_eat,
        consumption: components
            .use_animation
            .as_deref()
            .map(|animation| match animation {
                "eat" => ItemConsumption::Eat,
                "drink" => ItemConsumption::Drink,
                _ => ItemConsumption::None,
            })
            .or_else(|| components.food.then_some(ItemConsumption::Eat)),
        movement_modifier_bits: components.use_movement_modifier_bits,
        cooldown: components.use_cooldown.as_ref().map(|cooldown| {
            assets::item_use::ItemUseCategory {
                category: cooldown.category.as_ref().into(),
                ticks: cooldown.ticks,
            }
        }),
    };
    overlay_authored(profile, &overlay);
}

/// Applies explicit fields while duration alone leaves consumption unresolved.
fn overlay_authored(profile: &mut ItemUseProfile, overlay: &assets::CompiledItemUseOverlay) {
    if overlay.food_can_always_eat.is_some() && matches!(profile, ItemUseProfile::Unresolved) {
        *profile = ItemUseProfile::Provisional {
            kind: ItemUseKind::Hold,
            duration: ItemUseDurationRule::Declared,
            requirement: ItemUseRequirement::Nothing,
            consumption: ItemConsumption::Unresolved,
            movement_bits: 1.0_f64.to_bits(),
            cooldown: None,
            projectile: ItemProjectileRole::None,
            duration_alias: None,
        };
    }
    let ItemUseProfile::Provisional {
        duration,
        requirement,
        consumption,
        movement_bits,
        cooldown,
        ..
    } = profile
    else {
        return;
    };
    if overlay.duration_ticks.is_some() {
        *duration = ItemUseDurationRule::Declared;
    }
    if let Some(always) = overlay.food_can_always_eat {
        *requirement = if always {
            ItemUseRequirement::Nothing
        } else {
            ItemUseRequirement::Appetite
        };
    }
    if let Some(authored) = overlay.consumption {
        *consumption = authored;
    }
    if let Some(bits) = overlay.movement_modifier_bits {
        *movement_bits = bits;
    }
    if let Some(authored) = &overlay.cooldown {
        *cooldown = Some(authored.clone());
    }
}

impl ItemStateStore {
    /// Installs immutable compiled authored facts and rebuilds only after a source replacement.
    pub(crate) fn set_use_overlays(
        &mut self,
        overlays: Arc<BTreeMap<Box<str>, assets::CompiledItemUseOverlay>>,
    ) {
        if !self
            .use_overlays
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, &overlays))
        {
            self.use_overlays = Some(overlays);
            self.rebuild_use_profiles();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds original network admissions with owned component evidence.
    fn entry(
        id: i32,
        name: &str,
        components: protocol::ItemComponents,
    ) -> protocol::ItemRegistryEntry {
        protocol::ItemRegistryEntry {
            identifier: name.into(),
            network_id: id,
            component_based: true,
            version: protocol::ItemRegistryVersion::DataDriven,
            component_digest: [0; 32],
            components: Some(Arc::new(components)),
            negotiated_max_stack_size: None,
            canonical_empty_component_data: false,
            item_tags: Arc::from([]),
        }
    }

    #[test]
    fn declared_duration_does_not_admit_use_kind_food_or_movement() {
        let mut store = ItemStateStore::diagnostic();
        assert!(store.apply_registry(protocol::ItemRegistryEvent {
            entries: Arc::from([entry(
                15000,
                "fixture:hold",
                protocol::ItemComponents {
                    use_duration_ticks: Some(32),
                    ..Default::default()
                }
            )])
        }));
        let profile = store.item_use_profile(15000);
        assert_eq!(profile.ticks, Some(32));
        assert_eq!(profile.kind, None);
        assert_eq!(profile.movement, 1.0);
        assert_eq!(profile.requirement, ItemUseRequirement::Nothing);
        assert_eq!(profile.consumption, ItemConsumption::Unresolved);
        assert_eq!(store.item_use_profile(15001).kind, None);
    }

    #[test]
    fn food_movement_and_owned_shared_categories_are_resolved_at_admission() {
        let mut store = ItemStateStore::diagnostic();
        let components = protocol::ItemComponents {
            food: true,
            food_can_always_eat: Some(true),
            use_animation: Some("drink".into()),
            use_duration_ticks: Some(12),
            use_movement_modifier_bits: Some(0.75_f64.to_bits()),
            use_cooldown: Some(protocol::ItemUseCooldown {
                category: "fixture:shared".into(),
                ticks: 9,
            }),
            ..Default::default()
        };
        assert!(store.apply_registry(protocol::ItemRegistryEvent {
            entries: Arc::from([
                entry(15000, "fixture:one", components.clone()),
                entry(15001, "fixture:two", components.clone()),
            ])
        }));
        let first = store.item_use_profile(15000);
        assert_eq!(first.consumption, ItemConsumption::Drink);
        assert_eq!(first.requirement, ItemUseRequirement::Nothing);
        assert_eq!(first.movement, 0.75);
        assert_eq!(first.cooldown, store.item_use_profile(15001).cooldown);
        let old_category = first.cooldown.unwrap().category;
        assert!(store.apply_registry(protocol::ItemRegistryEvent {
            entries: Arc::from([entry(
                15000,
                "fixture:replacement",
                protocol::ItemComponents {
                    food: true,
                    food_can_always_eat: Some(false),
                    use_duration_ticks: Some(4),
                    ..components
                }
            )])
        }));
        let replacement = store.item_use_profile(15000);
        assert_eq!(replacement.requirement, ItemUseRequirement::Appetite);
        assert_eq!(replacement.ticks, Some(4));
        assert_ne!(replacement.cooldown.unwrap().category, old_category);
        assert_eq!(store.item_use_profile(15001).kind, None);
    }

    #[test]
    fn rejected_registry_replacement_preserves_all_admitted_profiles() {
        let mut store = ItemStateStore::diagnostic();
        let entry = entry(
            15000,
            "fixture:original",
            protocol::ItemComponents {
                use_duration_ticks: Some(7),
                ..Default::default()
            },
        );
        assert!(store.apply_registry(protocol::ItemRegistryEvent {
            entries: Arc::from([entry.clone()])
        }));
        let profile = store.item_use_profile(15000);
        assert!(!store.apply_registry(protocol::ItemRegistryEvent {
            entries: Arc::from([entry.clone(), entry])
        }));
        assert_eq!(store.item_use_profile(15000), profile);
    }
}
