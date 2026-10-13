//! Experimental component host. Only the explicit WIT imports carry authority.

#[cfg(feature = "execution")]
mod grants;
#[cfg(feature = "execution")]
mod requested_grants;
#[cfg(feature = "execution")]
pub use requested_grants::{REQUESTED_GRANTS_SECTION, requested_grants};
pub mod helper;
#[cfg(feature = "execution")]
pub use grants::{Capability, ModGrants};
#[cfg(feature = "execution")]
mod callback_budget;
#[cfg(feature = "execution")]
mod callbacks;
#[cfg(feature = "execution")]
pub use callback_budget::CallbackBudget;
#[cfg(feature = "execution")]
mod exclusive;
#[cfg(feature = "execution")]
pub use exclusive::HudClaims;
#[cfg(feature = "execution")]
mod execution;
#[cfg(feature = "execution")]
mod load;
#[cfg(feature = "execution")]
pub mod local_files;
#[cfg(feature = "execution")]
pub mod module_observations;
#[cfg(feature = "execution")]
mod outputs;
#[cfg(feature = "execution")]
pub mod package;
#[cfg(feature = "execution")]
mod runtime;
#[cfg(feature = "execution")]
mod screens;
#[cfg(feature = "execution")]
pub mod server;
#[cfg(feature = "execution")]
mod settings;
#[cfg(feature = "execution")]
pub use settings::settings_companion;

#[cfg(feature = "execution")]
pub use screens::{DataSource, KeyModifiers, LoadedPackage, ModEvent, ModScreens};

#[cfg(feature = "execution")]
pub use runtime::cinnabar::extension::gameplay::{
    CameraRig as GameplayCameraRig, Mob as GameplayMob,
    MovementSnapshot as GameplayMovementSnapshot, Player as GameplayPlayer,
    Snapshot as GameplaySnapshot, Vector3 as GameplayVector3,
};
#[cfg(feature = "execution")]
pub use runtime::cinnabar::extension::player_state::{
    Actor as ActorObservation, Bounds as ActorObservationBounds,
    ChatMessage as ObservedChatMessage, CombatEvent as CombatObservation, CombatKind,
    Effect as PlayerStateEffect, Food as FoodObservation, Item as PlayerStateItem,
    Motion as MotionObservation, Sidebar as SidebarSnapshot, SidebarRow, Slot as PlayerStateSlot,
    Snapshot as PlayerStateSnapshot, Vitals as VitalsObservation, WorldObservation,
};
#[cfg(feature = "execution")]
pub use runtime::cinnabar::extension::{
    events::Cue as ModCue, input::Controls as ControlFrame, panel::Event as ControlEvent,
};

/// Successfully committed local interaction requests, consumed once per frame.
#[cfg(feature = "execution")]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InteractionOutput {
    pub attack_reach: Option<f32>,
    pub attack_pulse: bool,
}

/// Committed local actor rotation; yaw turns left and pitch turns up, in radians.
#[cfg(feature = "execution")]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CameraDelta {
    pub yaw: f32,
    pub pitch: f32,
}
#[cfg(feature = "execution")]
use {
    runtime::Instance,
    server_experience::{screen::ScreenLayout, session_data::SessionData},
    std::{path::PathBuf, sync::Arc},
    wasmtime::Engine,
};

/// Maximum bytes accepted before compilation or allocation of a package buffer.
pub const MAX_COMPONENT_BYTES: usize = 4 * 1024 * 1024;
/// Plain-text UI limit, checked before publishing any guest output.
pub const MAX_LABEL_BYTES: usize = 256;
#[cfg(feature = "execution")]
pub(crate) const FRAME_FUEL: u64 = 100_000;
#[cfg(feature = "execution")]
pub(crate) const MEMORY_BYTES: usize = 16 * 1024 * 1024;

/// Where a mod came from, which reload reads again.
#[cfg(feature = "execution")]
enum Source {
    Component(PathBuf),
    Package(PathBuf),
}
/// A developer-selected component with transactional reload and trap quarantine.
#[cfg(feature = "execution")]
pub struct ModHost {
    engine: Engine,
    _ticker: execution::EpochTicker,
    instance: Instance,
    source: Source,
    attempted: [u8; 32],
    grants: ModGrants,
    /// The loader's authorization before the manifest narrows package permissions.
    authorized_grants: ModGrants,
    settings_writer: Option<settings::SettingsWriter>,
    settings_seed: Option<String>,
    package: Option<LoadedPackage>,
    layout: Option<ScreenLayout>,
    session: Arc<SessionData>,
}

#[cfg(feature = "execution")]
/// Builds an empty, unfocused control snapshot for a frame with no routed input.
pub fn empty_controls() -> ControlFrame {
    ControlFrame {
        seconds: 0.0,
        focused: false,
        gameplay: false,
        panel_open: false,
        keys_pressed: Vec::new(),
        keys_held: Vec::new(),
        events: Vec::new(),
    }
}

#[cfg(all(test, feature = "execution"))]
mod tests;
