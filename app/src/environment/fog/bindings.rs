//! Registry-generation joins for biome and fog sampling.

use std::{collections::BTreeMap, sync::Arc};

use assets::{
    BiomeVisualProfile, FogProfile, ResolvedBiomeTints, RuntimeAssets, RuntimeAtmosphereAssets,
};

#[cfg(test)]
use assets::BiomeRule;

use super::super::{EnvironmentProfileRoute, profile_lookup::dimension_fallback_biome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BiomeProfileIndex(pub usize);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FogProfileIndex(pub usize);

#[derive(Clone)]
struct Binding {
    biome: BiomeProfileIndex,
    fog: Option<FogProfileIndex>,
    route: Arc<EnvironmentProfileRoute>,
}

struct BiomeBinding {
    identifier: Arc<str>,
    profile: Option<Binding>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BiomeGeneration {
    Startup([u8; 32]),
    Session(meshing::ChunkBiomeTintIdentity),
}

/// Joins immutable registries once and retains their generation identity.
#[derive(bevy::prelude::Resource, Default)]
pub(crate) struct FogBindings {
    atmosphere: Option<Arc<RuntimeAtmosphereAssets>>,
    biome_generation: Option<BiomeGeneration>,
    /// Published atmosphere identity that owns these indices and routes.
    pub(crate) atmosphere_generation: Option<[u8; 32]>,
    by_id: BTreeMap<u32, BiomeBinding>,
    defaults: [Option<Binding>; 3],
    pub default_fog: Option<FogProfileIndex>,
}

impl FogBindings {
    /// Rebinds typed indices only when either owning registry is replaced.
    pub fn refresh(
        &mut self,
        world: &RuntimeAssets,
        atmosphere_assets: Option<&render::AtmosphereTextureAssets>,
    ) {
        self.refresh_registry(
            BiomeGeneration::Startup(world.provenance().biome_registry_sha256),
            world
                .biome_assets()
                .rules
                .iter()
                .map(|rule| (rule.id, rule.name.as_ref())),
            atmosphere_assets,
        );
    }

    /// Rebinds admitted session names only when their registry or atmosphere changes.
    pub fn refresh_session(
        &mut self,
        registry: &ResolvedBiomeTints,
        generation: meshing::ChunkBiomeTintIdentity,
        atmosphere_assets: Option<&render::AtmosphereTextureAssets>,
    ) {
        self.refresh_registry(
            BiomeGeneration::Session(generation),
            Self::registry_identifiers(registry),
            atmosphere_assets,
        );
    }

    /// Iterates admitted raw IDs and names without assigning an identity to fallback cells.
    fn registry_identifiers(registry: &ResolvedBiomeTints) -> impl Iterator<Item = (u32, &str)> {
        registry
            .raw_id_to_dense
            .iter()
            .enumerate()
            .filter_map(|(id, _)| {
                let id = u32::try_from(id).ok()?;
                registry
                    .biome(id)
                    .map(|biome| (id, biome.identifier.as_ref()))
            })
    }

    /// Joins names and visual routes once for the supplied owning registry generation.
    fn refresh_registry<'a>(
        &mut self,
        generation: BiomeGeneration,
        names: impl IntoIterator<Item = (u32, &'a str)>,
        atmosphere_assets: Option<&render::AtmosphereTextureAssets>,
    ) {
        let atmosphere = atmosphere_assets.and_then(|assets| assets.runtime());
        let atmosphere_generation = atmosphere_assets
            .filter(|assets| assets.runtime().is_some())
            .map(|assets| assets.identity());
        let same = match (&self.atmosphere, atmosphere) {
            (Some(old), Some(new)) => Arc::ptr_eq(old, new),
            (None, None) => true,
            _ => false,
        };
        if same
            && self.biome_generation == Some(generation)
            && self.atmosphere_generation == atmosphere_generation
        {
            return;
        }
        self.atmosphere = atmosphere.cloned();
        self.atmosphere_generation = atmosphere_generation;
        self.biome_generation = Some(generation);
        self.by_id.clear();
        self.defaults = Default::default();
        self.default_fog = None;
        self.compile(
            names,
            atmosphere.map_or(&[][..], |assets| assets.biome_profiles()),
            atmosphere.map_or(&[][..], |assets| assets.fog_profiles()),
        );
    }

    /// Compiles palette bindings and shared diagnostic routes from one generation.
    fn compile<'a>(
        &mut self,
        names: impl IntoIterator<Item = (u32, &'a str)>,
        profiles: &[BiomeVisualProfile],
        fogs: &[FogProfile],
    ) {
        self.by_id.clear();
        let bind = |name: &str| {
            let biome = profiles
                .binary_search_by(|profile| profile.biome_identifier.as_ref().cmp(name))
                .ok()?;
            let profile = &profiles[biome];
            Some(Binding {
                biome: BiomeProfileIndex(biome),
                fog: fogs
                    .binary_search_by(|fog| fog.identifier.cmp(&profile.fog_identifier))
                    .ok()
                    .map(FogProfileIndex),
                route: Arc::new(EnvironmentProfileRoute {
                    biome_identifier: Some(Arc::from(profile.biome_identifier.as_ref())),
                    fog_identifier: Some(Arc::from(profile.fog_identifier.as_ref())),
                    atmosphere_identifier: Some(Arc::from(profile.atmosphere_identifier.as_ref())),
                    provisional_lighting_identifier: Some(Arc::from(
                        profile.lighting_identifier.as_ref(),
                    )),
                }),
            })
        };
        for (id, name) in names {
            let profile = bind(name);
            let identifier = profile
                .as_ref()
                .and_then(|binding| binding.route.biome_identifier.clone())
                .unwrap_or_else(|| Arc::from(name));
            self.by_id.insert(
                id,
                BiomeBinding {
                    identifier,
                    profile,
                },
            );
        }
        self.defaults = std::array::from_fn(|dimension| {
            dimension_fallback_biome(dimension as i32).and_then(bind)
        });
        self.default_fog = fogs
            .binary_search_by(|fog| fog.identifier.as_ref().cmp("minecraft:fog_default"))
            .ok()
            .map(FogProfileIndex);
    }

    /// Builds synthetic registry bindings for behavior tests without texture carriers.
    #[cfg(test)]
    pub(super) fn for_profiles(
        rules: &[BiomeRule],
        profiles: &[BiomeVisualProfile],
        fogs: &[FogProfile],
    ) -> Self {
        let mut result = Self {
            atmosphere_generation: Some(Default::default()),
            ..Default::default()
        };
        result.compile(
            rules.iter().map(|rule| (rule.id, rule.name.as_ref())),
            profiles,
            fogs,
        );
        result
    }

    /// Shares the admitted biome name even when it has no visual profile.
    pub fn identifier(&self, raw: Option<u32>) -> Option<Arc<str>> {
        self.by_id
            .get(&raw?)
            .map(|binding| Arc::clone(&binding.identifier))
    }

    /// Returns the sample's prebound fog; only unknown biome IDs use the dimension fallback.
    pub fn fog(&self, raw: Option<u32>, dimension: i32) -> Option<FogProfileIndex> {
        match raw.and_then(|raw| self.by_id.get(&raw)) {
            Some(binding) => binding.profile.as_ref().and_then(|binding| binding.fog),
            None => self.fallback(dimension).and_then(|binding| binding.fog),
        }
    }

    /// Resolves the camera profile with its existing dimension fallback and shared route names.
    pub fn camera(
        &self,
        raw: Option<u32>,
        dimension: i32,
    ) -> (
        Option<BiomeProfileIndex>,
        Option<Arc<EnvironmentProfileRoute>>,
        Option<FogProfileIndex>,
    ) {
        let binding = raw
            .and_then(|raw| self.by_id.get(&raw))
            .and_then(|binding| binding.profile.as_ref())
            .or_else(|| self.fallback(dimension));
        (
            binding.map(|binding| binding.biome),
            binding.map(|binding| Arc::clone(&binding.route)),
            binding.and_then(|binding| binding.fog),
        )
    }

    /// Returns the explicit fallback profile for supported dimension IDs.
    fn fallback(&self, dimension: i32) -> Option<&Binding> {
        self.defaults
            .get(usize::try_from(dimension).ok()?)?
            .as_ref()
    }
}

#[cfg(test)]
mod tests;
