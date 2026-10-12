//! Launcher-owned update choices and the small notice shared with presentation.

use update_manifest::channel::Channel;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Restart,
    Later,
    Download,
    SwitchNow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ready {
    pub version: String,
    pub writable: bool,
    pub protocol_required: bool,
}

impl Ready {
    /// The primary action always requires the user's click, including protocol-required updates.
    pub fn action(&self) -> Action {
        if self.writable {
            Action::Restart
        } else {
            Action::Download
        }
    }

    /// Uses stronger wording after an outdated-client refusal without starting installation.
    pub fn title(&self) -> String {
        if self.protocol_required {
            format!("Update required to connect ({})", self.version)
        } else {
            format!("Update ready ({})", self.version)
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct View {
    pub ready: Option<Ready>,
    pub stable_available: bool,
    pub switch_available: bool,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Preferences {
    pub checks: bool,
    pub install_on_quit: bool,
    pub channel: Channel,
}

impl Preferences {
    /// Shared defaults for the saved settings and the host's initial session state.
    pub const DEFAULT: Self = Self {
        checks: true,
        install_on_quit: false,
        channel: Channel::Beta,
    };
}

impl Default for Preferences {
    /// Checks default on; installation on exit needs a separate opt-in.
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl View {
    /// Only verified publication of a stable release unlocks its selector entry.
    pub fn channel_visible(&self, choice: usize) -> bool {
        Channel::ALL
            .get(choice)
            .is_some_and(|channel| *channel != Channel::Stable || self.stable_available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_is_hidden_until_a_verified_release_is_available() {
        let mut view = View::default();
        assert!(view.channel_visible(Channel::Beta.index()));
        assert!(view.channel_visible(Channel::Nightly.index()));
        assert!(!view.channel_visible(Channel::Stable.index()));
        view.stable_available = true;
        assert!(view.channel_visible(Channel::Stable.index()));
    }
    #[test]
    fn protocol_requirement_changes_wording_without_authorizing_installation() {
        let notice = Ready {
            version: "2.0.0".into(),
            writable: false,
            protocol_required: true,
        };
        assert!(notice.title().contains("required to connect"));
        assert_eq!(notice.action(), Action::Download);
        assert!(!Preferences::default().install_on_quit);
    }
}
