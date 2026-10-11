//! Recipe inputs bound to numeric item IDs before inventory gestures.

use protocol::{ItemRegistryEntry, RecipeCatalog, ScreenRecipeKind, WindowKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

/// Bounds retained numeric metadata joins independently of the recipe payload limit.
const MAX_BINDINGS: usize = protocol::MAX_ITEM_REGISTRY_ENTRIES;
/// Caps cold tag comparisons before a sparse catalog can stall admission.
const MAX_JOIN_WORK: usize = MAX_BINDINGS * 16;
type Accepted = BTreeMap<(u8, i32), BTreeSet<u16>>;

#[derive(Clone, Debug, Default)]
pub(super) struct FurnaceInputs {
    catalog: Option<Arc<RecipeCatalog>>,
    registry: Option<Arc<BTreeMap<i32, ItemRegistryEntry>>>,
    accepted: Option<Arc<Accepted>>,
}

impl FurnaceInputs {
    /// Rebuilds on allocation changes and reports whether accepted numeric inputs changed.
    pub(super) fn refresh(
        &mut self,
        catalog: Option<Arc<RecipeCatalog>>,
        registry: Option<Arc<BTreeMap<i32, ItemRegistryEntry>>>,
    ) -> bool {
        if same(&self.catalog, &catalog) && same(&self.registry, &registry) {
            return false;
        }
        self.catalog = catalog;
        self.registry = registry;
        let next = bind(self.catalog.as_deref(), self.registry.as_deref()).map(Arc::new);
        let changed = self.accepted != next;
        self.accepted = next;
        changed
    }

    /// Reads only numeric IDs and metadata; unavailable admission remains unknown.
    pub(super) fn accepts(
        &self,
        window: WindowKind,
        network_id: i32,
        metadata: u32,
    ) -> Option<bool> {
        let accepted = self.accepted.as_ref()?;
        let kind = crate::furnace_recipes::kind(window)?;
        Some(
            accepted
                .get(&(kind as u8, network_id))
                .is_some_and(|values| {
                    values.contains(&protocol::RECIPE_ANY_AUX)
                        || u16::try_from(metadata).is_ok_and(|metadata| values.contains(&metadata))
                }),
        )
    }
}

/// Joins names and tags once, with bounded work and storage; unsupported joins stay absent.
fn bind(
    catalog: Option<&RecipeCatalog>,
    registry: Option<&BTreeMap<i32, ItemRegistryEntry>>,
) -> Option<Accepted> {
    let (catalog, registry) = (catalog?, registry?);
    let mut names = BTreeMap::<&str, BTreeSet<i32>>::new();
    for (&id, entry) in registry {
        names.entry(&entry.identifier).or_default().insert(id);
    }
    let recipes = || {
        catalog
            .screen_recipe_entries()
            .iter()
            .filter(|recipe| station(recipe.kind) && recipe.ingredients.len() == 1)
    };
    let mut tags = BTreeMap::<&str, BTreeSet<i32>>::new();
    for recipe in recipes() {
        let ingredient = &recipe.ingredients[0];
        if ingredient.tag {
            tags.entry(&ingredient.name).or_default();
        }
    }
    let mut tag_bindings = 0;
    let mut join_work = 0;
    for (tag, ids) in &mut tags {
        for (&id, entry) in registry {
            join_work += 1;
            if join_work > MAX_JOIN_WORK {
                tracing::warn!(
                    limit = MAX_JOIN_WORK,
                    "furnace tag joins exceed the admission work limit"
                );
                return None;
            }
            if (entry.item_tags.iter().any(|value| &**value == *tag)
                || protocol::vanilla_tag_contains(tag, &entry.identifier) == Some(true))
                && ids.insert(id)
            {
                tag_bindings += 1;
                if tag_bindings > MAX_BINDINGS {
                    tracing::warn!(
                        limit = MAX_BINDINGS,
                        "furnace tag bindings exceed the admission limit"
                    );
                    return None;
                }
            }
        }
    }
    let mut accepted = Accepted::new();
    let mut bindings = 0;
    for recipe in recipes() {
        let ingredient = &recipe.ingredients[0];
        let ids = if ingredient.tag {
            tags.get(&*ingredient.name)
        } else {
            names.get(&*ingredient.name)
        };
        let metadata = if ingredient.tag {
            protocol::RECIPE_ANY_AUX
        } else {
            ingredient.aux
        };
        for &id in ids.into_iter().flatten() {
            if accepted
                .entry((recipe.kind as u8, id))
                .or_default()
                .insert(metadata)
            {
                bindings += 1;
                if bindings > MAX_BINDINGS {
                    tracing::warn!(
                        limit = MAX_BINDINGS,
                        "furnace input joins exceed the admission limit"
                    );
                    return None;
                }
            }
        }
    }
    Some(accepted)
}

/// Compares retained allocations so unavailability also invalidates admission.
fn same<T>(left: &Option<Arc<T>>, right: &Option<Arc<T>>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => Arc::ptr_eq(left, right),
        (None, None) => true,
        _ => false,
    }
}

/// Restricts this index to the three furnace recipe families.
fn station(kind: ScreenRecipeKind) -> bool {
    matches!(
        kind,
        ScreenRecipeKind::Furnace | ScreenRecipeKind::BlastFurnace | ScreenRecipeKind::Smoker
    )
}

impl super::PlayerInventoryLedger {
    /// Binds committed recipes and retires ingredient predictions when their admission changes.
    pub(crate) fn bind_furnace_inputs(&mut self, catalog: Option<Arc<RecipeCatalog>>) {
        if !self
            .furnace_inputs
            .refresh(catalog, self.item_registry_snapshot().cloned())
        {
            return;
        }
        if self
            .window_kind()
            .and_then(crate::furnace_recipes::kind)
            .is_none()
        {
            return;
        }
        self.abandon_requests(|pending| {
            pending.predicted.iter().any(|prediction| {
                prediction.cell == super::Cell::Storage(0) && prediction.held.is_some()
            })
        });
        self.refold();
    }

    /// Reads cached station input acceptance without names or recipe iteration.
    pub(crate) fn furnace_input_accepts(
        &self,
        window: WindowKind,
        network_id: i32,
        metadata: u32,
    ) -> Option<bool> {
        self.furnace_inputs.accepts(window, network_id, metadata)
    }
}

#[cfg(test)]
mod tests;
