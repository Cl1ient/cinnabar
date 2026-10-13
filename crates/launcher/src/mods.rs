//! Mod-management commands and immutable launcher snapshots for the Settings Mods tab.

/// The Mods tab's settings selector variable.
pub const SECTION: &str = "cinnabar_mods_index";
/// File extension of an importable mod component, as `mod-host pack` writes it.
pub const COMPONENT_EXTENSION: &str = "wasm";

/// Commands the settings host can submit without owning mod storage; mods are named by
/// [`InstalledMod::id`], so a list change cannot retarget a press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ReadMore(u64),
    ToggleActive,
    ToggleAvailable,
    Enable(u64),
    Disable,
    Remove(u64),
    Import,
}

/// One component in the per-user mods directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledMod {
    /// Stable per-file identity.
    pub id: u64,
    pub name: String,
    pub file: String,
    pub bytes: u64,
}

/// What the mod host last recorded for the registered component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Starting,
    Running,
    Failed(String),
}

/// Installed mods and the host's record of them, published by the mod runtime.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    /// False when this build cannot run mods.
    pub supported: bool,
    pub installed: Vec<InstalledMod>,
    /// The enabled mod's id; the host runs at most one registered component.
    pub enabled: Option<u64>,
    pub status: Option<Status>,
    pub busy: bool,
    pub message: String,
}

impl Library {
    /// Splits the installed mods into the enabled one and the rest, in directory order.
    pub fn groups(&self) -> (Vec<InstalledMod>, Vec<InstalledMod>) {
        self.installed
            .iter()
            .cloned()
            .partition(|installed| Some(installed.id) == self.enabled)
    }
}

/// The Mods page: the published library and its disclosure state.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub library: Library,
    pub details_expanded: Option<u64>,
    pub active_expanded: bool,
    pub available_expanded: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            library: Library::default(),
            details_expanded: None,
            active_expanded: true,
            available_expanded: true,
        }
    }
}

impl Snapshot {
    /// Applies a disclosure toggle in place; returns the commands the mod runtime must run,
    /// which a build without mod support drops.
    pub fn apply(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::ReadMore(id) => {
                self.details_expanded = (self.details_expanded != Some(id)).then_some(id);
                None
            }
            Action::ToggleActive => {
                self.active_expanded = !self.active_expanded;
                None
            }
            Action::ToggleAvailable => {
                self.available_expanded = !self.available_expanded;
                None
            }
            Action::Enable(_) | Action::Disable | Action::Remove(_) | Action::Import => {
                (self.library.supported && !self.library.busy).then_some(action)
            }
        }
    }

    /// Replaces the library and closes details of a mod that left it.
    pub fn publish(&mut self, library: Library) {
        if self
            .details_expanded
            .is_some_and(|id| !library.installed.iter().any(|installed| installed.id == id))
        {
            self.details_expanded = None;
        }
        self.library = library;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(id: u64) -> InstalledMod {
        InstalledMod {
            id,
            name: format!("mod-{id}"),
            file: format!("mod-{id}.wasm"),
            bytes: 1,
        }
    }

    #[test]
    fn disclosure_stays_local_and_work_needs_a_supporting_idle_runtime() {
        let mut snapshot = Snapshot::default();
        assert_eq!(snapshot.apply(Action::ReadMore(7)), None);
        assert_eq!(snapshot.details_expanded, Some(7));
        assert_eq!(snapshot.apply(Action::ToggleAvailable), None);
        assert!(!snapshot.available_expanded);
        assert_eq!(snapshot.apply(Action::Import), None);
        snapshot.library.supported = true;
        assert_eq!(snapshot.apply(Action::Enable(7)), Some(Action::Enable(7)));
        snapshot.library.busy = true;
        assert_eq!(snapshot.apply(Action::Remove(7)), None);
    }

    #[test]
    fn publishing_closes_details_of_a_removed_mod_and_splits_the_enabled_one() {
        let mut snapshot = Snapshot::default();
        snapshot.apply(Action::ReadMore(2));
        snapshot.publish(Library {
            installed: vec![installed(1), installed(2)],
            enabled: Some(2),
            ..Library::default()
        });
        assert_eq!(snapshot.details_expanded, Some(2));
        let (active, available) = snapshot.library.groups();
        assert_eq!(active, [installed(2)]);
        assert_eq!(available, [installed(1)]);
        snapshot.publish(Library {
            installed: vec![installed(1)],
            ..Library::default()
        });
        assert_eq!(snapshot.details_expanded, None);
    }
}
