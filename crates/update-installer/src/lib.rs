//! Verified local downloads and one offline Rust installer process.

pub mod apply;
pub mod parent;
mod platform;
pub mod stage;
pub mod target;

/// Keys embedded into the client and helper; the existing list supports key rotation.
pub const TRUSTED_KEYS: &str = match option_env!("UPDATE_TRUSTED_KEYS") {
    Some(keys) => keys,
    None => "",
};

/// The packaged helper's filename on this host.
pub fn helper_filename() -> String {
    format!("cinnabar-updater{}", std::env::consts::EXE_SUFFIX)
}

/// Environment variable used only for the helper's first-start acknowledgement.
pub const STARTUP_ENV: &str = "CINNABAR_UPDATE_STARTUP_FILE";

#[cfg(test)]
mod fixtures;
