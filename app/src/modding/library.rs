//! The Settings Mods tab's library. Imports, removal and selection run on one worker, which
//! publishes the per-user mods directory with the host's recorded status.

use super::registration::selection::{registered, select};
use crate::global_resources::picker::{self, Filter};
use bevy::prelude::*;
use client_shell::menu::MenuRuntime;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender, unbounded};
use launcher::mods::{Action, COMPONENT_EXTENSION, InstalledMod, Library};
use mod_host::ModHost;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
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
pub(super) fn configure(app: &mut App, mods: PathBuf, registration: PathBuf) {
    let (commands, incoming) = unbounded();
    let (outgoing, published) = unbounded();
    let spawned = std::thread::Builder::new()
        .name("mod-library".into())
        .spawn(move || run(&mods, &registration, &incoming, &outgoing));
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
fn run(mods: &Path, registration: &Path, incoming: &Receiver<Action>, outgoing: &Sender<Library>) {
    let mut requests = 0;
    let mut library = scan(mods, registration, String::new());
    let _ = outgoing.send(library.clone());
    loop {
        let message = match incoming.recv_timeout(super::RELOAD_INTERVAL) {
            Ok(action) => {
                if action == Action::Import {
                    let _ = outgoing.send(Library {
                        busy: true,
                        ..library.clone()
                    });
                }
                perform(action, mods, registration, &mut requests).unwrap_or_else(|error| error)
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
        Action::Import => match picker::pick(&FILTER) {
            Ok(Some(source)) => import(mods, &source).map(|name| format!("Imported {name}.")),
            Ok(None) => Ok(String::new()),
            Err(error) => Err(format!("{error}The mods folder is {}.", mods.display())),
        },
        Action::Enable(id) => {
            select(registration, &path(id)?, true, request()).map(|()| String::new())
        }
        Action::Disable => match registered(registration) {
            Some(current) => {
                select(registration, &current.component, false, request()).map(|()| String::new())
            }
            None => Ok(String::new()),
        },
        Action::Remove(id) => {
            let path = path(id)?;
            if registered(registration)
                .is_some_and(|current| current.enabled && current.component == path)
            {
                select(registration, &path, false, request())?;
            }
            remove(&path).map(|()| String::new())
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
    let mut bytes = Vec::new();
    fs::File::open(source)
        .and_then(|file| {
            file.take(mod_host::MAX_COMPONENT_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|error| format!("Could not read {name}: {error}"))?;
    ModHost::validate_component(&bytes)
        .map_err(|error| format!("{name} is not a valid mod: {error:#}"))?;
    let staging = mods.join(format!(".{name}.importing"));
    fs::create_dir_all(mods)
        .and_then(|()| fs::write(&staging, &bytes))
        .and_then(|()| fs::rename(&staging, mods.join(name)))
        .map_err(|error| {
            let _ = fs::remove_file(&staging);
            format!("Could not install {name}: {error}")
        })?;
    Ok(name.to_owned())
}

/// Deletes a component and the settings file the host keeps beside it.
fn remove(path: &Path) -> Result<(), String> {
    for file in [path.to_owned(), path.with_extension("settings.json")] {
        match fs::remove_file(&file) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && file != path => {}
            Err(error) => return Err(format!("Could not remove the mod: {error}")),
        }
    }
    Ok(())
}

fn is_component(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(COMPONENT_EXTENSION))
}

/// Components in the mods directory by file name, skipping hidden in-progress imports.
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
            .find(|(_, path)| *path == current.component)
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

    /// A bad package is rejected before anything lands in the mods directory.
    #[test]
    fn import_rejects_a_bad_package_and_installs_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let mods = directory.path().join("mods");
        let source = directory.path().join("broken.wasm");
        fs::write(&source, b"not a component").unwrap();
        let error = import(&mods, &source).unwrap_err();
        assert!(error.contains("broken.wasm is not a valid mod"), "{error}");
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
        perform(Action::Enable(id), &mods, &registration, &mut requests).unwrap();
        assert_eq!(scan(&mods, &registration, String::new()).enabled, Some(id));
        perform(Action::Remove(id), &mods, &registration, &mut requests).unwrap();
        let after = scan(&mods, &registration, String::new());
        assert!(after.installed.is_empty() && after.enabled.is_none());
        assert!(!registered(&registration).unwrap().enabled);
        assert!(!mods.join("zoom.settings.json").exists());
    }
}
