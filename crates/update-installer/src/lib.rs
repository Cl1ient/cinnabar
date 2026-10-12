//! Verified local update storage and offline installation. This crate has no HTTP client.

pub mod apply;
pub mod parent;
mod platform;
pub mod stage;
pub mod target;

pub use time::OffsetDateTime;

/// Release channel consumed by installed clients and their offline helper.
pub const CHANNEL: &str = "stable";
/// Installed helper's basename, also used when copying it out of the install.
pub const HELPER_NAME: &str = "cinnabar-updater";
/// Public keys embedded into both the client and the offline helper at build time.
pub const TRUSTED_KEYS: &str = match option_env!("UPDATE_TRUSTED_KEYS") {
    Some(keys) => keys,
    None => "",
};

/// The native helper filename for the current host.
pub fn helper_filename() -> String {
    format!("{HELPER_NAME}{}", std::env::consts::EXE_SUFFIX)
}

#[cfg(test)]
mod fixtures;
