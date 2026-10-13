//! The Settings Mods tab's library. Imports, removal and selection run on one worker, which
//! publishes the per-user mods directory with the host's recorded status.

use super::registration::{
    removal,
    selection::{Receipt, disable, forget, registered, same_path, select},
};
use crate::global_resources::picker::{self, Filter};
use bevy::prelude::*;
use client_shell::menu::MenuRuntime;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use launcher::mods::{Action, COMPONENT_EXTENSION, InstalledMod, Library};
use mod_host::ModHost;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

const FILTER: Filter = Filter {
    title: "Import mod",
    name: "Mods",
    extensions: &[COMPONENT_EXTENSION],
    unavailable_hint: "",
};

#[derive(Resource)]
pub(super) struct ModLibrary {
    commands: Sender<Action>,
    published: Receiver<Library>,
}

/// Starts the library worker and the system that connects it to the Mods tab.
pub(super) fn configure(
    app: &mut App,
    mods: PathBuf,
    registration: PathBuf,
    removal: removal::Client,
) {
    let (commands, incoming) = unbounded();
    let (outgoing, published) = unbounded();
    let spawned = std::thread::Builder::new()
        .name("mod-library".into())
        .spawn(move || {
            run(
                &mods,
                &registration,
                &incoming,
                &outgoing,
                || picker::pick(&FILTER),
                |request| removal.wait(request),
            )
        });
    if let Err(error) = spawned {
        eprintln!("Mod library unavailable: {error}");
        return;
    }
    app.insert_resource(ModLibrary {
        commands,
        published,
    })
    .add_systems(Update, drive);
}

/// Forwards Mods-tab work and publishes the newest library.
fn drive(library: Res<ModLibrary>, menu: Option<ResMut<MenuRuntime>>) {
    let Some(mut menu) = menu else { return };
    if !menu.mod_actions.is_empty() {
        for action in std::mem::take(&mut menu.mod_actions) {
            let _ = library.commands.send(action);
        }
    }
    if let Some(latest) = library.published.try_iter().last() {
        Arc::make_mut(&mut menu.mods).publish(latest);
    }
}

/// Serves commands in order and republishes when the directory or recorded status changes.
fn run(
    mods: &Path,
    registration: &Path,
    incoming: &Receiver<Action>,
    outgoing: &Sender<Library>,
    mut pick: impl FnMut() -> Result<Option<PathBuf>, String>,
    mut retire: impl FnMut(&Receipt) -> Result<removal::Permit, String>,
) {
    let mut requests = 0;
    let mut library = scan(mods, registration, String::new());
    let _ = outgoing.send(library.clone());
    loop {
        let message = match incoming.recv_timeout(super::RELOAD_INTERVAL) {
            Ok(action) => {
                if matches!(action, Action::Import | Action::Remove(_)) {
                    library.busy = true;
                    let _ = outgoing.send(library.clone());
                }
                perform(
                    action,
                    mods,
                    registration,
                    &mut requests,
                    &mut pick,
                    &mut retire,
                )
                .unwrap_or_else(|error| error)
            }
            Err(RecvTimeoutError::Timeout) => library.message.clone(),
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let next = scan(mods, registration, message);
        if next != library {
            library = next;
            let _ = outgoing.send(library.clone());
        }
    }
}

/// Runs one Mods-tab command; both outcomes are the message the tab shows.
fn perform(
    action: Action,
    mods: &Path,
    registration: &Path,
    requests: &mut u64,
    pick: &mut impl FnMut() -> Result<Option<PathBuf>, String>,
    retire: &mut impl FnMut(&Receipt) -> Result<removal::Permit, String>,
) -> Result<String, String> {
    let installed = installed(mods);
    let path = |id: u64| {
        installed
            .iter()
            .find(|(item, _)| item.id == id)
            .map(|(_, path)| path.clone())
            .ok_or_else(|| "That mod is no longer installed.".to_owned())
    };
    let mut request = || {
        *requests += 1;
        format!("settings-{}-{}", std::process::id(), *requests)
    };
    match action {
        Action::Import => match pick() {
            Ok(Some(source)) => import(mods, &source).map(|name| format!("Imported {name}.")),
            Ok(None) => Ok(String::new()),
            Err(error) => Err(format!("{error}The mods folder is {}.", mods.display())),
        },
        Action::Enable(id) => {
            select(registration, &path(id)?, true, request()).map(|()| String::new())
        }
        Action::Disable(id) => disable(registration, &path(id)?, request()).map(|()| String::new()),
        Action::Remove(id) => {
            let path = path(id)?;
            let request_id = request();
            let receipt = forget(registration, &path, request_id)?;
            let permit = retire(&receipt)?;
            let companion = settings_companion(&path);
            let shared = registered(registration).is_some_and(|current| {
                !same_path(&current.component, &path)
                    && same_path(&settings_companion(&current.component), &companion)
            });
            remove(
                &path,
                (!shared).then_some(companion.as_path()),
                registration,
                &receipt,
                &permit,
            )
            .map(|()| String::new())
        }
        Action::ReadMore(_) | Action::ToggleActive | Action::ToggleAvailable => Ok(String::new()),
    }
}

/// Validates a chosen component and copies it into the mods directory; returns its file name.
fn import(mods: &Path, source: &Path) -> Result<String, String> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|_| is_component(source))
        .ok_or_else(|| format!("Choose a .{COMPONENT_EXTENSION} mod component."))?;
    if name.starts_with('.') {
        return Err("Mod file names must not start with a dot.".into());
    }
    let bytes = mod_host::local_files::read(source, mod_host::MAX_COMPONENT_BYTES)
        .map_err(|error| format!("Could not read {name}: {error}"))?;
    ModHost::validate_component(&bytes)
        .map_err(|error| format!("{name} is not a valid mod: {error:#}"))?;
    publish_component(mods, name, &bytes)?;
    Ok(name.to_owned())
}

/// Publishes complete bytes under a new name without overwriting any existing file or link.
fn publish_component(mods: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    fs::create_dir_all(mods).map_err(|error| format!("Could not install {name}: {error}"))?;
    let staging = mods.join(format!(".{name}.{}.importing", std::process::id()));
    let mut file = fs::File::create_new(&staging)
        .map_err(|error| format!("Could not stage {name}: {error}"))?;
    let result = file
        .write_all(bytes)
        .and_then(|()| fs::hard_link(&staging, mods.join(name)));
    drop(file);
    let _ = fs::remove_file(&staging);
    result.map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            format!("A mod named {name} is already installed. Choose a different file name.")
        } else {
            format!("Could not install {name}: {error}")
        }
    })
}

/// The companion entry used for this component's persisted settings.
fn settings_companion(path: &Path) -> PathBuf {
    path.with_extension("settings.json")
}

/// Rechecks the removal's selection before deleting each retired component or owned companion.
fn remove(
    path: &Path,
    companion: Option<&Path>,
    registration: &Path,
    receipt: &Receipt,
    permit: &removal::Permit,
) -> Result<(), String> {
    for file in std::iter::once(path).chain(companion) {
        receipt.confirm(registration)?;
        permit.confirm()?;
        match fs::remove_file(file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && file != path => {}
            Err(error) => return Err(format!("Could not remove the mod: {error}")),
        }
    }
    Ok(())
}

/// Recognizes the component extension accepted by the Mods tab.
fn is_component(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(COMPONENT_EXTENSION))
}

/// Lists visible components by file name; staging files have a different extension.
fn installed(mods: &Path) -> Vec<(InstalledMod, PathBuf)> {
    let Ok(entries) = fs::read_dir(mods) else {
        return Vec::new();
    };
    let mut found: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let file = path.file_name()?.to_str()?.to_owned();
            let name = path.file_stem()?.to_str()?.to_owned();
            let metadata = entry.metadata().ok().filter(fs::Metadata::is_file)?;
            (!file.starts_with('.') && is_component(&path)).then(|| {
                let digest = Sha256::digest(file.as_bytes());
                let id = u64::from_le_bytes(digest[..8].try_into().expect("eight digest bytes"));
                let item = InstalledMod {
                    id,
                    name,
                    file,
                    bytes: metadata.len(),
                };
                (item, path)
            })
        })
        .collect();
    found.sort_by(|(left, _), (right, _)| left.file.cmp(&right.file));
    found
}

/// The directory's components with the registered one and its status.
fn scan(mods: &Path, registration: &Path, message: String) -> Library {
    let installed = installed(mods);
    let current = registered(registration).filter(|current| current.enabled);
    let enabled = current.as_ref().and_then(|current| {
        installed
            .iter()
            .find(|(_, path)| same_path(path, &current.component))
            .map(|(item, _)| item.id)
    });
    Library {
        supported: true,
        installed: installed.into_iter().map(|(item, _)| item).collect(),
        enabled,
        status: enabled.and(current.map(|current| current.status)),
        busy: false,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Models a completed retirement whose watcher generation remains current.
    fn retirement_permit() -> removal::Permit {
        removal::Permit::new(1, Arc::new(std::sync::atomic::AtomicU64::new(1)))
    }

    /// Creates a directory alias, or reports the missing Windows symlink privilege fixture.
    #[cfg(any(unix, windows))]
    fn directory_alias(target: &Path, alias: &Path) -> bool {
        #[cfg(unix)]
        let result = std::os::unix::fs::symlink(target, alias);
        #[cfg(windows)]
        let result = std::os::windows::fs::symlink_dir(target, alias);
        match result {
            Ok(()) => true,
            #[cfg(windows)]
            Err(error) if error.raw_os_error() == Some(1314) => {
                eprintln!("Missing fixture: Windows directory symlink privilege is unavailable");
                false
            }
            Err(error) => panic!("create directory alias: {error}"),
        }
    }

    /// A stale Disable command cannot revoke the component which replaced its displayed card.
    #[test]
    fn disabling_a_stale_card_preserves_the_new_selection() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let registration = directory.path().join("config");
        fs::create_dir_all(&mods).unwrap();
        let first = mods.join("first.wasm");
        let second = mods.join("second.wasm");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        select(&registration, &first, true, "first".into()).unwrap();
        let shown = scan(&mods, &registration, String::new());
        let displayed_id = shown.enabled.unwrap();
        select(&registration, &second, true, "second".into()).unwrap();
        let record = registration.join(super::super::registration::REGISTRATION_FILE);
        let before = fs::read(&record).unwrap();
        let error = perform(
            Action::Disable(displayed_id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap_err();
        assert!(error.contains("no longer enabled"), "{error}");
        assert_eq!(fs::read(&record).unwrap(), before);
        let current = registered(&registration).unwrap();
        assert!(current.enabled && same_path(&current.component, &second));
        let id = scan(&mods, &registration, String::new()).enabled.unwrap();
        perform(
            Action::Disable(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap();
        assert!(!registered(&registration).unwrap().enabled);
    }

    /// Re-enabling after a successful retirement reply must still prevent file deletion.
    #[test]
    fn removal_rechecks_selection_after_retirement_returns() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let registration = directory.path().join("config");
        fs::create_dir_all(&mods).unwrap();
        let component = mods.join("zoom.wasm");
        let companion = settings_companion(&component);
        fs::write(&component, b"component").unwrap();
        fs::write(&companion, b"preferences").unwrap();
        let id = installed(&mods)[0].0.id;
        let error = perform(
            Action::Remove(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_: &Receipt| {
                select(&registration, &component, true, "replacement".into()).unwrap();
                Ok(retirement_permit())
            },
        )
        .unwrap_err();
        assert!(error.contains("selection changed"), "{error}");
        assert_eq!(fs::read(component).unwrap(), b"component");
        assert_eq!(fs::read(companion).unwrap(), b"preferences");
        assert!(registered(&registration).unwrap().enabled);
    }

    /// A newer applied generation blocks deletion even if the registration bytes stay identical.
    #[test]
    fn removal_rechecks_generation_after_retirement_returns() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let registration = directory.path().join("config");
        fs::create_dir_all(&mods).unwrap();
        let component = mods.join("zoom.wasm");
        let companion = settings_companion(&component);
        fs::write(&component, b"component").unwrap();
        fs::write(&companion, b"preferences").unwrap();
        let id = installed(&mods)[0].0.id;
        let error = perform(
            Action::Remove(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_| {
                Ok(removal::Permit::new(
                    1,
                    Arc::new(std::sync::atomic::AtomicU64::new(2)),
                ))
            },
        )
        .unwrap_err();
        assert!(error.contains("selection changed"), "{error}");
        assert_eq!(fs::read(component).unwrap(), b"component");
        assert_eq!(fs::read(companion).unwrap(), b"preferences");
    }

    /// Real and linked directory spellings select, disable and remove the same component.
    #[cfg(any(unix, windows))]
    #[test]
    fn removal_resolves_the_selected_components_directory_alias() {
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real");
        let mods = directory.path().join("linked");
        let registration = directory.path().join("config");
        fs::create_dir_all(&real).unwrap();
        if !directory_alias(&real, &mods) {
            return;
        }
        let component = real.join("zoom.wasm");
        let companion = settings_companion(&component);
        fs::write(&component, b"component").unwrap();
        fs::write(&companion, b"preferences").unwrap();
        select(&registration, &component, true, "selected".into()).unwrap();
        let record = registration.join(super::super::registration::REGISTRATION_FILE);
        let mut granted: serde_json::Value =
            serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        granted["grants"]["settings"] = true.into();
        fs::write(&record, serde_json::to_vec(&granted).unwrap()).unwrap();
        let id = scan(&mods, &registration, String::new()).enabled.unwrap();
        perform(
            Action::Disable(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap();
        select(
            &registration,
            &mods.join("zoom.wasm"),
            true,
            "reenabled".into(),
        )
        .unwrap();
        let kept: serde_json::Value = serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        assert_eq!(kept["grants"], granted["grants"]);
        perform(
            Action::Remove(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_: &Receipt| {
                assert!(!registered(&registration).unwrap().enabled);
                Ok(retirement_permit())
            },
        )
        .unwrap();
        let cleared: serde_json::Value =
            serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
        assert_eq!(
            cleared["grants"],
            serde_json::to_value(mod_host::ModGrants::default()).unwrap()
        );
        assert!(!component.exists() && !companion.exists());
    }

    /// Companion ownership also follows directory aliases, including an absent companion entry.
    #[cfg(any(unix, windows))]
    #[test]
    fn removal_preserves_shared_preferences_through_a_directory_alias() {
        let directory = tempfile::tempdir().unwrap();
        let real = directory.path().join("real");
        let mods = directory.path().join("linked");
        let registration = directory.path().join("config");
        fs::create_dir_all(&real).unwrap();
        if !directory_alias(&real, &mods) {
            return;
        }
        let component = real.join("zoom.wasm");
        let selected = real.join("zoom.wat");
        let companion = settings_companion(&component);
        fs::write(&component, b"component").unwrap();
        fs::write(&selected, b"selected component").unwrap();
        assert!(same_path(
            &companion,
            &settings_companion(&mods.join("zoom.wat"))
        ));
        fs::write(&companion, b"selected preferences").unwrap();
        select(&registration, &selected, true, "selected".into()).unwrap();
        let id = installed(&mods)[0].0.id;
        perform(
            Action::Remove(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap();
        assert_eq!(fs::read(companion).unwrap(), b"selected preferences");
        assert!(!component.exists() && selected.exists());
    }

    /// Import reads share the host's source ceiling and require regular files.
    #[test]
    fn import_rejects_nonregular_and_oversized_sources() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let source = directory.path().join("large.wasm");
        fs::File::create(&source)
            .unwrap()
            .set_len(mod_host::MAX_COMPONENT_BYTES as u64 + 1)
            .unwrap();
        assert!(import(&mods, &source).unwrap_err().contains("byte limit"));
        fs::remove_file(&source).unwrap();
        fs::create_dir(&source).unwrap();
        assert!(
            import(&mods, &source)
                .unwrap_err()
                .contains("Could not read")
        );
        assert!(!mods.exists());
    }

    /// Linked source components cannot bypass the host's admission rules through Import.
    #[cfg(unix)]
    #[test]
    fn import_rejects_a_linked_source_component() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let source = directory.path().join("source.wasm");
        let link = directory.path().join("linked.wasm");
        fs::write(&source, component("")).unwrap();
        std::os::unix::fs::symlink(&source, &link).unwrap();
        assert!(import(&mods, &link).unwrap_err().contains("Could not read"));
        assert!(!mods.exists());
    }

    /// Hidden filenames are rejected before any component is installed or reported as imported.
    #[test]
    fn import_rejects_hidden_component_names() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let source = directory.path().join(".zoom.wasm");
        fs::write(&source, component("")).unwrap();
        let error = import(&mods, &source).unwrap_err();
        assert!(error.contains("must not start with a dot"), "{error}");
        assert!(!mods.exists());
        assert!(installed(&mods).is_empty());
    }

    /// A final retirement save happens before deletion, so it cannot recreate the companion.
    #[test]
    fn removal_waits_for_pending_settings_before_deleting_files() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let registration = directory.path().join("config");
        fs::create_dir_all(&mods).unwrap();
        let path = mods.join("zoom.wasm");
        let companion = settings_companion(&path);
        fs::write(&path, b"component").unwrap();
        fs::write(&companion, b"old preferences").unwrap();
        select(&registration, &path, true, "selected".into()).unwrap();
        let id = installed(&mods)[0].0.id;
        let retire = |_: &Receipt| {
            assert!(!registered(&registration).unwrap().enabled);
            assert!(path.exists() && companion.exists());
            fs::write(&companion, b"final preferences").unwrap();
            Ok(retirement_permit())
        };
        let (commands, incoming) = unbounded();
        let (outgoing, published) = unbounded();
        commands.send(Action::Remove(id)).unwrap();
        drop(commands);
        run(
            &mods,
            &registration,
            &incoming,
            &outgoing,
            || Ok(None),
            retire,
        );
        let updates: Vec<_> = published.try_iter().collect();
        assert_eq!(
            updates
                .iter()
                .map(|library| library.busy)
                .collect::<Vec<_>>(),
            [false, true, false]
        );
        assert!(updates[2].message.is_empty() && updates[2].installed.is_empty());
        assert!(!path.exists() && !companion.exists());
    }

    /// Failed retirement keeps both files available for a later removal attempt.
    #[test]
    fn removal_keeps_files_when_retirement_cannot_complete() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let registration = directory.path().join("config");
        fs::create_dir_all(&mods).unwrap();
        let path = mods.join("zoom.wasm");
        let companion = settings_companion(&path);
        fs::write(&path, b"component").unwrap();
        fs::write(&companion, b"preferences").unwrap();
        let id = installed(&mods)[0].0.id;
        let error = perform(
            Action::Remove(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_| Err("host stopped".into()),
        )
        .unwrap_err();
        assert_eq!(error, "host stopped");
        assert_eq!(fs::read(&path).unwrap(), b"component");
        assert_eq!(fs::read(&companion).unwrap(), b"preferences");
    }

    /// Components sharing a settings anchor cannot delete the selected mod's preferences.
    #[test]
    fn removal_preserves_a_companion_shared_with_the_selected_component() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let registration = directory.path().join("config");
        fs::create_dir_all(&mods).unwrap();
        let removed = mods.join("zoom.wasm");
        let selected = mods.join("zoom.wat");
        let companion = settings_companion(&removed);
        fs::write(&removed, b"removed component").unwrap();
        fs::write(&selected, b"selected component").unwrap();
        fs::write(&companion, b"selected preferences").unwrap();
        select(&registration, &selected, true, "selected".into()).unwrap();
        let id = installed(&mods)[0].0.id;
        perform(
            Action::Remove(id),
            &mods,
            &registration,
            &mut 0,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap();
        assert!(!removed.exists());
        assert_eq!(fs::read(&selected).unwrap(), b"selected component");
        assert_eq!(fs::read(&companion).unwrap(), b"selected preferences");
        assert!(registered(&registration).unwrap().enabled);
    }

    /// Builds a minimal mod with a distinct frame body for import tests.
    fn component(frame: &str) -> String {
        format!(
            r#"(component
                (core module $m
                    (func (export "init"))
                    (func (export "frame") {frame}))
                (core instance $i (instantiate $m))
                (func (export "init") (canon lift (core func $i "init")))
                (func (export "frame") (canon lift (core func $i "frame"))))"#
        )
    }

    /// A filename collision leaves the enabled component, grants and registration untouched.
    #[test]
    fn importing_the_same_name_never_replaces_the_enabled_mod() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let registration = directory.path().join("config");
        fs::create_dir_all(&registration).unwrap();
        let source = directory.path().join("zoom.wasm");
        let original = component("");
        fs::write(&source, &original).unwrap();
        assert_eq!(import(&mods, &source).unwrap(), "zoom.wasm");
        let installed = mods.join("zoom.wasm");
        select(&registration, &installed, true, "enabled".into()).unwrap();
        let record = registration.join(super::super::registration::REGISTRATION_FILE);
        let mut granted: serde_json::Value =
            serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        granted["grants"]["hud"] = true.into();
        fs::write(&record, serde_json::to_vec(&granted).unwrap()).unwrap();
        let previous = fs::read(&record).unwrap();

        fs::write(&source, component("unreachable")).unwrap();
        let error = import(&mods, &source).unwrap_err();
        assert!(error.contains("already installed"), "{error}");
        assert_eq!(fs::read(&installed).unwrap(), original.as_bytes());
        assert_eq!(fs::read(&record).unwrap(), previous);
        assert!(registered(&registration).unwrap().enabled);
        assert_eq!(fs::read_dir(&mods).unwrap().count(), 1);
    }

    /// Cancelled imports always finish even when the idle library and message are unchanged.
    #[test]
    fn cancelling_import_publishes_idle_and_allows_more_commands() {
        let directory = tempfile::tempdir().unwrap();
        let (commands, incoming) = unbounded();
        let (outgoing, published) = unbounded();
        commands.send(Action::Import).unwrap();
        commands.send(Action::Import).unwrap();
        drop(commands);
        run(
            &directory.path().join("mods"),
            &directory.path().join("config"),
            &incoming,
            &outgoing,
            || Ok(None),
            |_| Ok(retirement_permit()),
        );
        let updates: Vec<_> = published.try_iter().collect();
        assert_eq!(
            updates
                .iter()
                .map(|library| library.busy)
                .collect::<Vec<_>>(),
            [false, true, false, true, false]
        );
        let mut snapshot = launcher::mods::Snapshot::default();
        snapshot.publish(updates[4].clone());
        assert_eq!(snapshot.library, updates[0]);
        for action in [
            Action::Import,
            Action::Enable(1),
            Action::Disable(1),
            Action::Remove(1),
        ] {
            assert_eq!(snapshot.apply(action), Some(action));
        }
    }

    /// Existing links cannot redirect staging writes or be replaced at publication.
    #[cfg(unix)]
    #[test]
    fn publishing_never_overwrites_destination_or_staging_links() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        let target = directory.path().join("outside.wasm");
        let destination = mods.join("zoom.wasm");
        symlink(&target, &destination).unwrap();
        assert!(publish_component(&mods, "zoom.wasm", b"replacement").is_err());
        assert_eq!(fs::read_link(&destination).unwrap(), target);
        assert!(!target.exists());

        fs::remove_file(&destination).unwrap();
        fs::write(&target, b"original").unwrap();
        let staging = mods.join(format!(".zoom.wasm.{}.importing", std::process::id()));
        symlink(&target, &staging).unwrap();
        assert!(publish_component(&mods, "zoom.wasm", b"replacement").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read_link(&staging).unwrap(), target);
        assert!(!destination.exists());
    }

    /// A bad package is rejected before anything lands in the mods directory.
    #[test]
    fn import_rejects_a_bad_package_and_installs_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let source = directory.path().join("broken.wasm");
        fs::write(&source, b"not a component").unwrap();
        let error = import(&mods, &source).unwrap_err();
        assert!(error.contains("broken.wasm is not a valid mod"), "{error}");
        fs::write(&source, b"(component)").unwrap();
        let error = import(&mods, &source).unwrap_err();
        assert!(error.contains("component lacks"), "{error}");
        let text = directory.path().join("notes.txt");
        fs::write(&text, b"(component)").unwrap();
        assert!(import(&mods, &text).is_err());
        assert!(installed(&mods).is_empty());
        assert!(!mods.exists());
    }

    /// Removal enforces the single-registration contract: the enabled mod is disabled first.
    #[test]
    fn removing_the_enabled_mod_disables_its_registration() {
        let directory = tempfile::tempdir().unwrap();
        let (mods, registration) = (
            directory.path().join("mods"),
            directory.path().join("config"),
        );
        fs::create_dir_all(&mods).unwrap();
        fs::create_dir_all(&registration).unwrap();
        fs::write(mods.join("zoom.wasm"), b"component").unwrap();
        fs::write(mods.join("zoom.settings.json"), b"{}").unwrap();
        let library = scan(&mods, &registration, String::new());
        let id = library.installed[0].id;
        let mut requests = 0;
        perform(
            Action::Enable(id),
            &mods,
            &registration,
            &mut requests,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap();
        assert_eq!(scan(&mods, &registration, String::new()).enabled, Some(id));
        let record = registration.join(super::super::registration::REGISTRATION_FILE);
        let mut granted: serde_json::Value =
            serde_json::from_slice(&fs::read(&record).unwrap()).unwrap();
        granted["grants"]["settings"] = true.into();
        granted["font"] = serde_json::to_value(directory.path().join("font.ttf")).unwrap();
        granted["ui"] = serde_json::to_value(directory.path().join("ui.json")).unwrap();
        fs::write(&record, serde_json::to_vec(&granted).unwrap()).unwrap();
        perform(
            Action::Remove(id),
            &mods,
            &registration,
            &mut requests,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap();
        let after = scan(&mods, &registration, String::new());
        assert!(after.installed.is_empty() && after.enabled.is_none());
        assert!(!registered(&registration).unwrap().enabled);
        assert!(!mods.join("zoom.settings.json").exists());
        let source = directory.path().join("zoom.wasm");
        fs::write(&source, component("unreachable")).unwrap();
        import(&mods, &source).unwrap();
        perform(
            Action::Enable(id),
            &mods,
            &registration,
            &mut requests,
            &mut || Ok(None),
            &mut |_| Ok(retirement_permit()),
        )
        .unwrap();
        let replacement: serde_json::Value =
            serde_json::from_slice(&fs::read(record).unwrap()).unwrap();
        assert_eq!(
            replacement["grants"],
            serde_json::to_value(mod_host::ModGrants::default()).unwrap()
        );
        assert!(replacement["font"].is_null() && replacement["ui"].is_null());
    }
}
