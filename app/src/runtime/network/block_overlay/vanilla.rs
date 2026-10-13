//! State-aware material replacement and block-specific texture rebinding.

use super::*;

impl Builder<'_> {
    /// Replaces selectors only for new catalog entries; raster-only packs replace their leaves.
    pub(super) fn override_vanilla_materials(&mut self, keys: &MaterialKeys) {
        let mut routes = keys
            .keys()
            .flat_map(|key| keys.materials(key).iter().map(move |&id| (id, key)))
            .chain(
                keys.material_sources()
                    .map(|(id, source)| (id, source.key.as_ref())),
            )
            .collect::<Vec<_>>();
        routes.sort_unstable();
        routes.dedup();
        let mut claimed = HashSet::new();
        for (id, key) in routes {
            if id == DIAGNOSTIC_MATERIAL || claimed.contains(&id) {
                continue;
            }
            let source = keys.material_source(id);
            let variant = source.map_or(0, |source| source.variant);
            let base = self
                .base_assets
                .and_then(|base| base.materials().get(id as usize))
                .copied();
            if self.catalog.redefined(key) {
                // Weighted leaves become unused when their selector is replaced.
                if base.is_some_and(|material| material.variation_weight != 0) {
                    continue;
                }
                let flags = base.map_or(0, |material| material.flags);
                if let Some(replacement) = self.terrain_material(keys, key, variant, flags) {
                    let replacement = self.overlay.materials[replacement as usize];
                    claimed.insert(id);
                    self.overlay.material_overrides.push(MaterialOverride {
                        material: id,
                        texture: replacement.texture,
                        animation: replacement.animation,
                        variation: Some((replacement.variation_start, replacement.variation_count)),
                    });
                }
            } else {
                if base.is_some_and(|material| material.variation_count != 0) {
                    continue;
                }
                let route = source.map_or_else(
                    || TextureRoute::Catalog {
                        key: key.to_owned(),
                        variant,
                        choice: 0,
                    },
                    |source| TextureRoute::Path {
                        variant: source.variant,
                        key: key.to_owned(),
                        path: source.path.to_string(),
                    },
                );
                if let Some(slot) = self.route_slot(route) {
                    claimed.insert(id);
                    self.overlay.material_overrides.push(MaterialOverride {
                        material: id,
                        texture: TextureRef::new(1, slot.layer).expect("bounded layer"),
                        animation: slot.animation,
                        variation: None,
                    });
                }
            }
        }
    }

    /// Rebinds faces per visual so blocks that share a material stay independent.
    pub(super) fn override_vanilla_bindings(
        &mut self,
        keys: &MaterialKeys,
        bindings: &legacy::TextureBindings,
    ) {
        let Some(base) = self.base_assets else {
            return;
        };
        let overridden = self
            .overlay
            .material_overrides
            .iter()
            .map(|replacement| replacement.material)
            .collect::<HashSet<_>>();
        for source in keys.block_sources() {
            let block = base.resolve(assets::NetworkIdMode::Sequential, source.visual);
            if !source.cube_binding_eligible
                || !block.is_known()
                || block.kind() != VisualKind::Cube
                || block.model_template().is_some()
                || block.variant() != 0
                || block.flags().contains(assets::BlockFlags::LEAF_MODEL)
                || assets::BlockFace::ALL.into_iter().any(|face| {
                    base.material(block.face(face).material_id()).flags
                        & assets::MATERIAL_FLAG_TINT_MASK
                        != 0
                })
            {
                continue;
            }
            let affected = assets::BlockFace::ALL.into_iter().any(|face| {
                let index = face as usize;
                let material = base.material(block.face(face).material_id());
                overridden.contains(&block.face(face).material_id())
                    || (material.variation_start
                        ..material.variation_start + material.variation_count)
                        .any(|id| overridden.contains(&id))
                    || bindings
                        .texture(
                            &source.name,
                            &source.binding_name,
                            source.source_faces[index] as usize,
                            false,
                        )
                        .is_some()
                    || bindings
                        .texture(&source.name, &source.binding_name, index, true)
                        .is_some()
            });
            let carried_changed = !affected
                && assets::BlockFace::ALL.into_iter().any(|face| {
                    let index = face as usize;
                    let variant = keys
                        .material_source(block.face(face).material_id())
                        .map_or(0, |source| source.variant);
                    source.carried_keys[index]
                        .as_deref()
                        .into_iter()
                        .chain(source.carried_fallback_keys[index].as_deref())
                        .any(|key| self.carried_changed(key, variant))
                });
            if !affected && !carried_changed {
                continue;
            }
            let carried_keys = assets::BlockFace::ALL.map(|face| {
                let index = face as usize;
                let material = block.face(face).material_id();
                bindings
                    .texture(&source.name, &source.binding_name, index, true)
                    .or(source.carried_keys[index].as_deref())
                    .or_else(|| bindings.texture(&source.name, &source.binding_name, index, false))
                    .or(source.carried_fallback_keys[index].as_deref())
                    .or_else(|| {
                        keys.material_source(material)
                            .map(|source| source.key.as_ref())
                    })
            });
            if carried_keys.iter().enumerate().any(|(index, key)| {
                let material = block.face(assets::BlockFace::ALL[index]).material_id();
                let variant = keys
                    .material_source(material)
                    .map_or(0, |source| source.variant);
                key.and_then(|key| self.catalog.choices(key, variant))
                    .is_some_and(|choices| choices.iter().any(|choice| choice.tint.is_some()))
            }) {
                continue;
            }
            let mut faces = [None; 6];
            let mut carried = [None; 6];

            for face in assets::BlockFace::ALL {
                let index = face as usize;
                let material = block.face(face).material_id();
                let variant = keys
                    .material_source(material)
                    .map_or(0, |source| source.variant);
                let flags = base.material(material).flags;
                let world_binding = bindings.binding(
                    &source.name,
                    &source.binding_name,
                    source.source_faces[index] as usize,
                    false,
                );
                if let Some(binding) = world_binding {
                    let rotation = if binding.per_face && source.source_rotations[index] {
                        assets::MATERIAL_FLAG_ROTATE_UV
                    } else {
                        0
                    };
                    faces[index] = self.terrain_material(
                        keys,
                        binding.key,
                        variant,
                        (flags & !assets::MATERIAL_FLAG_ROTATE_UV) | rotation,
                    );
                }
                let carried_key = carried_keys[index];
                if let Some(key) = carried_key {
                    carried[index] = self.terrain_material(
                        keys,
                        key,
                        variant,
                        flags & !assets::MATERIAL_FLAG_ROTATE_UV,
                    );
                }
            }
            if faces.iter().any(Option::is_some) {
                self.overlay.face_overrides.push((source.visual, faces));
            }
            if carried.iter().all(Option::is_some) {
                self.vanilla_carried.push(CarriedBlock {
                    visual: source.visual,
                    faces: carried.map(Option::unwrap),
                });
            }
        }
    }

    /// Tests optional carried routes without copying unrelated carrier pixels into the overlay.
    fn carried_changed(&mut self, key: &str, variant: u32) -> bool {
        if self.catalog.redefined(key) {
            return true;
        }
        let count = self
            .catalog
            .choices(key, variant)
            .map_or(0, |choices| choices.len());
        (0..count).any(|choice| {
            self.route_slot(TextureRoute::Catalog {
                key: key.to_owned(),
                variant,
                choice: choice as u32,
            })
            .is_some()
        })
    }

    /// Builds one state selector with independent, normalized positional alternatives.
    fn terrain_material(
        &mut self,
        keys: &MaterialKeys,
        key: &str,
        variant: u32,
        flags: u32,
    ) -> Option<u32> {
        let cache_key = (key.to_owned(), variant, flags);
        if let Some(&id) = self.terrain_materials.get(&cache_key) {
            return Some(id);
        }
        let choices = self.catalog.choices(key, variant)?.to_vec();
        if self.overlay.materials.len() + choices.len() + 1 > assets::MAX_MATERIALS {
            return None;
        }
        let mut leaves = Vec::with_capacity(choices.len());
        for (index, choice) in choices.iter().enumerate() {
            let slot = self
                .route_slot(TextureRoute::Catalog {
                    key: key.to_owned(),
                    variant,
                    choice: index as u32,
                })
                .or_else(|| {
                    let (id, source) = keys
                        .material_sources()
                        .filter(|(_, source)| {
                            source.path.as_ref() == choice.path
                                && (source.baked_tint.is_none() || source.baked_tint == choice.tint)
                        })
                        .min_by_key(|(_, source)| source.baked_tint != choice.tint)?;
                    let tint = source.baked_tint.is_none().then_some(choice.tint).flatten();
                    self.copy_base_texture(id, tint)
                })?;
            leaves.push(Material {
                texture: TextureRef::new(1, slot.layer).expect("bounded layer"),
                flags,
                animation: slot.animation,
                variation_weight: if choices.len() == 1 {
                    0
                } else {
                    choice.weight.to_bits()
                },
                ..Material::unvaried()
            });
        }
        let start = self.overlay.materials.len() as u32;
        let mut selector = leaves[0];
        self.overlay.materials.extend(leaves);
        let id = if choices.len() > 1 {
            selector.variation_start = start;
            selector.variation_count = choices.len() as u32;
            selector.variation_weight = 0;
            let id = self.overlay.materials.len() as u32;
            self.overlay.materials.push(selector);
            id
        } else {
            start
        };
        self.terrain_materials.insert(cache_key, id);
        Some(id)
    }

    /// Copies carrier frames and applies a multiplier only when it is not already baked in.
    fn copy_base_texture(&mut self, id: u32, tint: Option<[u8; 3]>) -> Option<TextureSlot> {
        let cache_key = (id, tint);
        if let Some(slot) = self.base_textures.get(&cache_key) {
            return Some(slot.clone());
        }
        let base = self.base_assets?;
        let material = base.material(id);
        let animation = base.animations().get(material.animation as usize).copied();
        let references = if let Some(animation) = animation {
            base.animation_frames()
                .get(
                    animation.frame_start as usize
                        ..animation.frame_start as usize + animation.frame_count as usize,
                )?
                .to_vec()
        } else {
            vec![material.texture]
        };
        if self.sources.len() + references.len() > self.max_layers {
            return None;
        }
        let mut frames = Vec::with_capacity(references.len());
        for reference in references {
            let page = base.texture_pages().get(reference.page() as usize)?;
            let mip = page.texture.mips.first()?;
            let bytes = (mip.size * mip.size * 4) as usize;
            let start = reference.layer() as usize * bytes;
            let mut frame = DecodedTexture {
                width: mip.size,
                height: mip.size,
                rgba8: mip.rgba8.get(start..start + bytes)?.into(),
            };
            if let Some(tint) = tint {
                pack_compiler::apply_atlas_tint(&mut frame.rgba8, tint);
            }
            frames.push(shrink_to_max(&frame, MAX_TILE));
        }
        let bytes: usize = frames.iter().map(|frame| frame.rgba8.len()).sum();
        if self.source_bytes + bytes > MAX_OVERLAY_SOURCE_BYTES {
            return None;
        }
        self.source_bytes += bytes;
        let first = self.sources.len() as u32;
        let animation = if let Some(animation) = animation {
            let frame_start = self.overlay.animation_frames.len() as u32;
            self.overlay.animation_frames.extend(
                (0..frames.len() as u32)
                    .map(|index| TextureRef::new(1, first + index).expect("bounded layer")),
            );
            let id = self.overlay.animations.len() as u32;
            self.overlay.animations.push(Animation {
                frame_start,
                ..animation
            });
            id
        } else {
            NO_ANIMATION
        };
        self.sources.extend(frames.into_iter().map(Source::Image));
        let slot = TextureSlot {
            layer: first,
            animation,
        };
        self.base_textures.insert(cache_key, slot.clone());
        Some(slot)
    }
}
