//! The portable JSON-UI engine: catalog policy, cached layout, painting into retained UI nodes,
//! and texture lookup over the carrier and an on-demand atlas. Screen families register
//! through [`engine::screen`]; hosts supply filesystem and network reads through [`sources`].

pub mod atlas;
pub mod cached_screen;
pub mod caret;
pub mod engine;
pub mod frame;
pub mod pages;
pub mod prepared;
pub mod scroll;
pub mod sources;
pub mod text_metrics;
pub mod textures;

#[cfg(test)]
mod allocation_count;
