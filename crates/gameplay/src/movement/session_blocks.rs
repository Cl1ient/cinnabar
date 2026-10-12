//! Collision palette installation for an accepted StartGame session.

use super::PhysicsCollisionRegistries;
use std::ops::Range;

/// The collision registration and wire mapping selected for a session's palette mode.
pub struct SessionBlockRegistration {
    pub custom_ids: Option<Range<u32>>,
    pub remap: assets::SequentialIdRemap,
    /// Whether this palette mode supports the supplied custom collision definitions.
    pub supported: bool,
}

impl PhysicsCollisionRegistries {
    /// Replaces custom collision facts for the selected palette mode and returns its wire mapping.
    pub fn begin_session_blocks(
        &mut self,
        custom: &protocol::CustomBlocks,
        hashed_ids: bool,
    ) -> SessionBlockRegistration {
        if hashed_ids {
            self.begin_session_custom_blocks(&protocol::CustomBlocks::default());
            SessionBlockRegistration {
                custom_ids: None,
                remap: assets::SequentialIdRemap::default(),
                supported: self.begin_session_hashed_custom_blocks(custom).is_some(),
            }
        } else {
            let registered = self.begin_session_custom_blocks(custom);
            let supported = registered.is_some() || custom.blocks.is_empty();
            let (custom_ids, remap) = match registered {
                Some((range, remap)) => (Some(range), remap),
                None => (None, assets::SequentialIdRemap::default()),
            };
            SessionBlockRegistration {
                custom_ids,
                remap,
                supported,
            }
        }
    }
}
