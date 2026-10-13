//! Settings-driven registration writes, and the registration and status the Mods tab shows.

use super::{
    REGISTRATION_FILE, Registration, STATUS_FILE,
    source::observe,
    status::{read_status, write_json},
};
use launcher::mods::Status;
use mod_host::ModGrants;
use std::path::{Path, PathBuf};

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
        Ok(Some(previous)) if previous.component == component => Registration {
            request_id,
            enabled,
            ..previous
        },
        _ => Registration {
            version: 1,
            request_id,
            enabled,
            component: component.to_owned(),
            font: None,
            ui: None,
            grants: ModGrants::default(),
        },
    };
    registration.validate()?;
    let mut json = serde_json::to_value(&registration).map_err(|error| error.to_string())?;
    json["request_id"] = serde_json::Value::String(registration.request_id);
    write_json(&path, &json).map_err(|error| format!("Could not save the mod selection: {error}"))
}

#[cfg(test)]
mod tests {
    use super::super::Status as Written;
    use super::*;

    /// A Settings write reads back through the watcher's own reader, with its recorded status.
    #[test]
    fn selection_round_trips_through_the_watcher_reader() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let zoom = root.join("zoom.wasm");
        select(root, &zoom, true, "settings-1".into()).unwrap();
        let read = observe(&root.join(REGISTRATION_FILE))
            .result
            .unwrap()
            .unwrap();
        assert_eq!(read.component, zoom);
        assert!(read.enabled);
        assert_eq!(read.request_id, "settings-1");
        assert_eq!(read.grants, ModGrants::default());
        assert_eq!(registered(root).unwrap().status, Status::Starting);
        let status = root.join(STATUS_FILE);
        write_json(&status, &Written::new("settings-1".into(), "loaded", None)).unwrap();
        assert_eq!(registered(root).unwrap().status, Status::Running);
        write_json(
            &status,
            &Written::new("settings-1".into(), "error", Some("trap".into())),
        )
        .unwrap();
        assert_eq!(
            registered(root).unwrap().status,
            Status::Failed("trap".into())
        );

        let mut granted = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(root.join(REGISTRATION_FILE)).unwrap(),
        )
        .unwrap();
        granted["grants"]["hud"] = true.into();
        std::fs::write(
            root.join(REGISTRATION_FILE),
            serde_json::to_vec(&granted).unwrap(),
        )
        .unwrap();
        select(root, &zoom, false, "settings-2".into()).unwrap();
        let disabled = registered(root).unwrap();
        assert!(!disabled.enabled && disabled.component == zoom);
        assert_eq!(disabled.status, Status::Starting);
        let kept = observe(&root.join(REGISTRATION_FILE))
            .result
            .unwrap()
            .unwrap();
        assert!(kept.grants.hud);

        let minimap = root.join("minimap.wasm");
        select(root, &minimap, true, "settings-3".into()).unwrap();
        let switched = observe(&root.join(REGISTRATION_FILE))
            .result
            .unwrap()
            .unwrap();
        assert_eq!(switched.component, minimap);
        assert_eq!(switched.grants, ModGrants::default());
    }
}
