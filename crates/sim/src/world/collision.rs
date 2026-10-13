use super::*;

// Collision revision allocation starts at one; zero identifies an absent column.
const UNLOADED_BOUNDARY_REVISION: u64 = 0;
const UNLOADED_BOUNDARY_VERTICAL_LIMIT: f64 = 100_000.0;

impl<'a> PaletteWorld<'a> {
    /// Borrows movement terrain with solid unloaded-column collision barriers.
    /// Partially received columns still require authoritative data for every queried section.
    #[must_use]
    pub fn for_movement(
        store: &'a ChunkStore,
        registry: &'a CollisionRegistry,
        dimension: i32,
    ) -> Self {
        let mut world = Self::new(store, registry, dimension, Default::default());
        world.unloaded_collision_barriers = true;
        world
    }

    /// Identifies an absent column without assigning it a loaded collision revision.
    pub(super) fn unloaded_boundary_revision(
        &self,
        chunk: ChunkKey,
    ) -> Option<ChunkCollisionRevision> {
        (self.unloaded_collision_barriers && self.store.collision_revision(chunk).is_none())
            .then_some(ChunkCollisionRevision {
                chunk,
                revision: UNLOADED_BOUNDARY_REVISION,
            })
    }

    /// Emits boundary geometry without attributing it to an invented palette block.
    fn unloaded_boundary_instances(
        &self,
        query: Aabb,
        chunks: &[ChunkKey],
    ) -> Vec<CollisionInstance> {
        let side = world::SUB_CHUNK_SIDE as f64;
        chunks
            .iter()
            .filter_map(|&chunk| {
                self.unloaded_boundary_revision(chunk)?;
                let x = f64::from(chunk.x) * side;
                let z = f64::from(chunk.z) * side;
                let shape = Aabb::new(
                    Vec3::new(x, -UNLOADED_BOUNDARY_VERTICAL_LIMIT, z),
                    Vec3::new(x + side, UNLOADED_BOUNDARY_VERTICAL_LIMIT, z + side),
                );
                shape.intersects(query).then_some(CollisionInstance {
                    shape,
                    block: None,
                    runtime_id: None,
                })
            })
            .collect()
    }

    /// Shared emission core behind both public collision surfaces so their
    /// outputs structurally cannot diverge: one translated registry shape
    /// instance per (block, layer, shape) triple in block-scan order, each
    /// kept only when it intersects `query`.
    pub(super) fn collision_instances(
        &self,
        query: Aabb,
    ) -> Result<CollisionQuery<Vec<CollisionInstance>>, WorldQueryError> {
        validate_collision_query(query)?;
        if query.min == query.max {
            return Ok(CollisionQuery {
                value: Vec::new(),
                identity: WorldCollisionIdentity::new(self.registry.identity(), [])?,
            });
        }
        let [min, max] = self.registry.query_bounds(query)?;
        let chunks = (min[0] >> 4..=max[0] >> 4)
            .flat_map(|x| {
                (min[2] >> 4..=max[2] >> 4).map(move |z| ChunkKey::new(self.dimension, x, z))
            })
            .collect::<Vec<_>>();
        let identity = self.identity_for_chunks(chunks.iter().copied())?;

        let mut instances = self.unloaded_boundary_instances(query, &chunks);
        for x in min[0]..=max[0] {
            for z in min[2]..=max[2] {
                if self
                    .unloaded_boundary_revision(ChunkKey::new(self.dimension, x >> 4, z >> 4))
                    .is_some()
                {
                    continue;
                }
                for y in min[1]..=max[1] {
                    let block = [x, y, z];
                    for runtime_id in self.runtime_ids_at(block)? {
                        let physics = self
                            .registry
                            .physics(runtime_id)
                            .ok_or(WorldQueryError::UnknownRuntimeId { runtime_id, block })?;
                        for shape in self
                            .block_collision_shapes(block, physics, query)?
                            .iter()
                            .copied()
                        {
                            let shape = shape.translated(physics.shape_offset(block));
                            if shape.intersects(query) {
                                instances.push(CollisionInstance {
                                    shape,
                                    block: Some(block),
                                    runtime_id: Some(runtime_id),
                                });
                            }
                        }
                    }
                }
            }
        }
        Ok(CollisionQuery {
            value: instances,
            identity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an air registry and one authoritative column beside absent terrain.
    fn terrain() -> (ChunkStore, CollisionRegistry) {
        let mut registry = CollisionRegistry::new();
        registry.register(0, []).unwrap();
        let mut store = ChunkStore::new();
        store.mark_chunk_loaded(ChunkKey::new(0, 0, 0)).unwrap();
        (store, registry)
    }

    #[test]
    fn flight_boundary_has_absence_identity_and_no_invented_block_provenance() {
        let (store, registry) = terrain();
        let query = Aabb::new(Vec3::new(15.5, 80.0, 7.5), Vec3::new(16.5, 82.0, 8.5));
        let flight = PaletteWorld::for_movement(&store, &registry, 0);
        let boxes = flight.collision_boxes_with_provenance(query).unwrap();
        assert_eq!(boxes.value.len(), 1);
        assert_eq!(boxes.value[0].block, None);
        assert_eq!(boxes.value[0].runtime_id, None);
        assert_eq!(boxes.value[0].aabb.min.x, world::SUB_CHUNK_SIDE as f64);
        assert!(boxes.identity.chunks.iter().any(
            |revision| revision.chunk.x == 1 && revision.revision == UNLOADED_BOUNDARY_REVISION
        ));
        assert!(!store.is_chunk_loaded(ChunkKey::new(0, 1, 0)));
        assert!(
            PaletteWorld::new(&store, &registry, 0, Default::default())
                .collision_boxes(query)
                .is_err()
        );
    }

    #[test]
    fn terrain_arrival_replaces_boundary_while_snapshot_preserves_it() {
        let (mut store, registry) = terrain();
        let query = Aabb::new(Vec3::new(15.5, 80.0, 7.5), Vec3::new(16.5, 82.0, 8.5));
        let flight = PaletteWorld::for_movement(&store, &registry, 0);
        let before = flight.collision_boxes(query).unwrap();
        let snapshot = flight.snapshot().unwrap();
        store.mark_chunk_loaded(ChunkKey::new(0, 1, 0)).unwrap();
        let arrived = PaletteWorld::for_movement(&store, &registry, 0)
            .collision_boxes(query)
            .unwrap();
        assert!(arrived.value.is_empty());
        assert!(
            arrived
                .identity
                .chunks
                .iter()
                .all(|revision| revision.revision != UNLOADED_BOUNDARY_REVISION)
        );
        assert_ne!(arrived.identity, before.identity);
        assert_eq!(snapshot.collision_boxes(query).unwrap(), before);
    }

    #[test]
    fn flight_boundaries_preserve_partial_section_and_unknown_block_errors() {
        let (mut store, registry) = terrain();
        let key = SubChunkKey::new(0, 1, 0, 0);
        store.mark_sub_chunk_loaded(key).unwrap();
        let flight = PaletteWorld::for_movement(&store, &registry, 0);
        assert!(matches!(
            flight.block_physics([16, 80, 8]),
            Err(WorldQueryError::UnloadedChunk(_))
        ));
        store.mark_chunk_loaded(key.chunk()).unwrap();
        let high = SubChunkKey::new(0, 1, 5, 0);
        store
            .update_block(high, world::BlockUpdate::new(0, 0, 8, 0, 99), 0)
            .unwrap();
        let flight = PaletteWorld::for_movement(&store, &registry, 0);
        assert!(matches!(
            flight.block_physics([16, 80, 8]),
            Err(WorldQueryError::UnknownRuntimeId { runtime_id: 99, .. })
        ));
    }
}
