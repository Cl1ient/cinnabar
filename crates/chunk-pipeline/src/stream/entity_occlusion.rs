use super::*;

impl WorldStream {
    /// Proves a box hidden using current uploaded terrain. Dirty, pending or missing meshes
    /// cannot occlude; `drawn` must also check the current view and section visibility.
    pub fn terrain_hides_box(
        &self,
        budget: &mut ::meshing::occlusion::OcclusionBudget,
        eye: [f32; 3],
        low: [f32; 3],
        high: [f32; 3],
        drawn: impl Fn([i32; 3], SubChunkKey) -> bool,
    ) -> bool {
        budget.hides_box(eye, low, high, |position| {
            let [x, y, z] = position.map(|axis| axis.div_euclid(world::SUB_CHUNK_SIDE as i32));
            let key = SubChunkKey::new(self.current_dimension(), x, y, z);
            if !self.is_mesh_clean(key)
                || !self.applied_mesh_generations.contains_key(&key)
                || !drawn(position, key)
            {
                return false;
            }
            let Some(sub_chunk) = self.authority.terrain().sub_chunk(key) else {
                return false;
            };
            ::meshing::ContributorResolver::resolve_direct(
                self.classifier,
                self.runtime_assets(),
                self.network_id_mode(),
                &sub_chunk,
                position.map(|axis| axis.rem_euclid(world::SUB_CHUNK_SIDE as i32) as u8),
            )
            .is_opaque_cube(self.runtime_assets())
        })
    }
}
