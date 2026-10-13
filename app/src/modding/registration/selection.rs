//! Settings-driven registration writes, and the registration and status the Mods tab shows.

use super::{
    REGISTRATION_FILE, Registration, STATUS_FILE,
    source::observe,
    status::{read_status, write_json},
};
use launcher::mods::Status;
use mod_host::ModGrants;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub(super) const SELECTION_CHANGED: &str = "The mod selection changed before removal finished.";

/// The exact registration published for a removal, checked again before deleting files.
#[derive(Clone, Debug)]
pub(in crate::modding) struct Receipt {
    pub(super) request_id: String,
    fingerprint: [u8; 32],
}

impl Receipt {
    /// Rejects a changed or unreadable registration, including changes which reuse its request ID.
    pub(in crate::modding) fn confirm(&self, directory: &Path) -> Result<(), String> {
        let current = observe(&directory.join(REGISTRATION_FILE));
        if current.fingerprint == self.fingerprint && current.result.is_ok() {
            Ok(())
        } else {
            Err(SELECTION_CHANGED.into())
        }
    }
}

/// Matches resolved entries, also resolving a missing companion through its existing parent.
pub(in crate::modding) fn same_path(left: &Path, right: &Path) -> bool {
    left == right
        || resolved_path(left)
            .zip(resolved_path(right))
            .is_some_and(|(left, right)| left == right)
}

/// Resolves directory aliases without requiring a not-yet-created settings entry to exist.
fn resolved_path(path: &Path) -> Option<PathBuf> {
    path.canonicalize()
        .ok()
        .or_else(|| Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?)))
}

/// A valid registration and the host's status for its current request.
pub(in crate::modding) struct Registered {
    pub(in crate::modding) component: PathBuf,
    pub(in crate::modding) enabled: bool,
    pub(in crate::modding) status: Status,
}

/// Reads the registration as the watcher would, with the status recorded for its request.
pub(in crate::modding) fn registered(directory: &Path) -> Option<Registered> {
    let Ok(Some(registration)) = observe(&directory.join(REGISTRATION_FILE)).result else {
        return None;
    };
    let status = match read_status(&directory.join(STATUS_FILE)) {
        Some(recorded) if recorded.request_id == registration.request_id => {
            match recorded.state.as_str() {
                "loaded" => Status::Running,
                "error" => Status::Failed(recorded.message.unwrap_or_default()),
                _ => Status::Starting,
            }
        }
        _ => Status::Starting,
    };
    Some(Registered {
        component: registration.component,
        enabled: registration.enabled,
        status,
    })
}

/// Points the registration at `component`; grants, font and UI survive only while the
/// component stays the same, so another mod starts with no optional capabilities.
pub(in crate::modding) fn select(
    directory: &Path,
    component: &Path,
    enabled: bool,
    request_id: String,
) -> Result<(), String> {
    let path = directory.join(REGISTRATION_FILE);
    let registration = match observe(&path).result {
        Ok(Some(previous)) if same_path(&previous.component, component) => Registration {
            request_id,
            enabled,
            ..previous
        },
        _ => fresh_selection(component, enabled, request_id),
    };
    write_selection(&path, registration).map(|_| ())
}

/// Disables only the selected component whose resolved entry matches the activated card.
pub(in crate::modding) fn disable(
    directory: &Path,
    component: &Path,
    request_id: String,
) -> Result<(), String> {
    let path = directory.join(REGISTRATION_FILE);
    let previous = observe(&path)
        .result?
        .filter(|previous| previous.enabled && same_path(&previous.component, component))
        .ok_or_else(|| "That mod is no longer enabled.".to_owned())?;
    write_selection(
        &path,
        Registration {
            request_id,
            enabled: false,
            ..previous
        },
    )
    .map(|_| ())
}

/// Clears a removed mod's retained authorization and advances the watcher's request.
/// A different selected mod keeps its authorization and enabled state.
pub(in crate::modding) fn forget(
    directory: &Path,
    component: &Path,
    request_id: String,
) -> Result<Receipt, String> {
    let path = directory.join(REGISTRATION_FILE);
    let registration = match observe(&path).result {
        Ok(Some(previous)) if !same_path(&previous.component, component) => Registration {
            request_id,
            ..previous
        },
        _ => fresh_selection(component, false, request_id),
    };
    write_selection(&path, registration)
}

/// Creates a selection without retained optional capabilities, font or panel metadata.
fn fresh_selection(component: &Path, enabled: bool, request_id: String) -> Registration {
    Registration {
        version: 1,
        request_id,
        enabled,
        component: component.to_owned(),
        font: None,
        ui: None,
        grants: ModGrants::default(),
    }
}

/// Publishes a validated selection and returns a receipt of its exact registration bytes.
fn write_selection(path: &Path, registration: Registration) -> Result<Receipt, String> {
    registration.validate()?;
    let mut json = serde_json::to_value(&registration).map_err(|error| error.to_string())?;
    json["request_id"] = serde_json::Value::String(registration.request_id.clone());
    let fingerprint =
        Sha256::digest(serde_json::to_vec(&json).map_err(|error| error.to_string())?).into();
    write_json(path, &json)
        .map_err(|error| format!("Could not save the mod selection: {error}"))?;
    Ok(Receipt {
        request_id: registration.request_id,
        fingerprint,
    })
}

#[cfg(test)]
mod tests;
