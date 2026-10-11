//! Numeric fuel admission for inventory gestures.

use assets::{RuntimeEntityAssets, item::fuel::ItemFuel};
use protocol::{ItemRegistryEntry, ItemRegistryVersion};
use std::sync::Arc;

use super::PlayerInventoryLedger;

impl PlayerInventoryLedger {
    /// Installs checked assets once; changed facts retire predictions bound to old facts.
    pub fn set_item_fuel_catalog(&mut self, catalog: Arc<RuntimeEntityAssets>) {
        if self
            .fuel_catalog
            .as_ref()
            .is_some_and(|old| Arc::ptr_eq(old, &catalog))
        {
            return;
        }
        let previous = self.fuel_facts.clone();
        self.fuel_catalog = Some(catalog);
        self.refresh_fuel_facts();
        let affected: Vec<i32> = self
            .queue
            .iter()
            .filter(|pending| {
                pending.fuel_routed
                    && pending
                        .predicted
                        .iter()
                        .filter_map(|prediction| prediction.held.as_ref())
                        .any(|held| {
                            previous.get(&held.stack.network_id)
                                != self.fuel_facts.get(&held.stack.network_id)
                        })
            })
            .map(|pending| pending.request_id)
            .collect();
        self.abandon_requests(|pending| affected.contains(&pending.request_id));
        self.refold();
    }

    /// Returns the admitted numeric fact without interpreting an item name.
    pub fn item_fuel(&self, network_id: i32) -> ItemFuel {
        self.fuel_facts
            .get(&network_id)
            .copied()
            .unwrap_or_default()
    }

    /// Refreshes fuel decisions only when registry or compiled assets change.
    pub(super) fn refresh_fuel_facts(&mut self) {
        self.fuel_facts = self
            .item_registry
            .as_ref()
            .into_iter()
            .flat_map(|registry| registry.iter())
            .map(|(&id, entry)| (id, resolve(entry, self.fuel_catalog.as_deref())))
            .collect();
    }
}

/// Resolves a supported session declaration before its compiled compatibility fallback.
pub(super) fn resolve(entry: &ItemRegistryEntry, assets: Option<&RuntimeEntityAssets>) -> ItemFuel {
    if matches!(entry.version, ItemRegistryVersion::Unknown(_)) {
        return ItemFuel::Unresolved;
    }
    if let Some(components) = &entry.components {
        if let Some(burn_ticks) = components.fuel_duration_ticks {
            return ItemFuel::Session { burn_ticks };
        }
        if components.fuel_declared {
            return ItemFuel::Unresolved;
        }
    }
    if entry.component_based {
        return ItemFuel::Unresolved;
    }
    assets
        .and_then(|assets| {
            let key = assets::ItemVisualKey {
                identifier: entry.identifier.as_ref().into(),
                metadata: 0,
            };
            assets
                .item_visuals()
                .binary_search_by(|visual| visual.key.cmp(&key))
                .ok()
                .map(|index| assets.item_visuals()[index].fuel)
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates two numeric aliases sharing a name but carrying distinct fuel declarations.
    fn entry(id: i32, ticks: Option<u32>) -> ItemRegistryEntry {
        let mut entry = protocol::vanilla_item_registry()[0].clone();
        entry.network_id = id;
        entry.identifier = Arc::from("custom:oak_planks");
        entry.component_based = true;
        entry.components = Some(Arc::new(protocol::ItemComponents {
            fuel_duration_ticks: ticks,
            ..Default::default()
        }));
        entry
    }

    #[test]
    fn numeric_fuel_aliases_refresh_zero_missing_malformed_and_removed_values() {
        let mut ledger = PlayerInventoryLedger::default();
        ledger.apply_registry(&protocol::ItemRegistryEvent {
            entries: vec![entry(10_001, Some(40)), entry(10_002, Some(0))].into(),
        });
        assert_eq!(ledger.item_fuel(10_001).burn_ticks(), Some(40));
        assert_eq!(ledger.item_fuel(10_002).eligible(), Some(false));
        ledger.apply_registry(&protocol::ItemRegistryEvent {
            entries: vec![entry(10_001, Some(0))].into(),
        });
        assert_eq!(ledger.item_fuel(10_001).eligible(), Some(false));
        assert_eq!(ledger.item_fuel(10_002), ItemFuel::Unresolved);
        let mut malformed = entry(10_001, None);
        Arc::make_mut(malformed.components.as_mut().unwrap()).fuel_declared = true;
        ledger.apply_registry(&protocol::ItemRegistryEvent {
            entries: vec![malformed].into(),
        });
        assert_eq!(ledger.item_fuel(10_001), ItemFuel::Unresolved);
        let mut unsupported = entry(10_001, Some(40));
        unsupported.version = ItemRegistryVersion::Unknown(42);
        assert_eq!(resolve(&unsupported, None), ItemFuel::Unresolved);
        assert_eq!(resolve(&entry(10_001, None), None), ItemFuel::Unresolved);
    }
}
