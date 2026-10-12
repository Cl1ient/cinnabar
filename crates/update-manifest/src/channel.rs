//! Release channels share their names, URLs, and ordering with launcher preferences.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Beta,
    Nightly,
    Stable,
}

impl Channel {
    /// Order used by the saved channel setting; stable stays last so it can be hidden initially.
    pub const ALL: [Self; 3] = [Self::Beta, Self::Nightly, Self::Stable];

    /// The signed channel name and its fixed GitHub release tag.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Beta => "beta",
            Self::Nightly => "nightly",
            Self::Stable => "stable",
        }
    }

    /// The user-facing channel name.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Beta => "Beta",
            Self::Nightly => "Nightly",
            Self::Stable => "Stable",
        }
    }

    /// The radio binding used by the existing settings picker.
    pub const fn binding(self) -> &'static str {
        match self {
            Self::Beta => "update_channel_beta",
            Self::Nightly => "update_channel_nightly",
            Self::Stable => "update_channel_stable",
        }
    }

    /// The persisted setting's index, derived from its declared order.
    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|channel| *channel == self)
            .expect("declared channel")
    }

    /// Resolves only the published channels, rejecting an unknown wire value.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|channel| channel.name() == name)
    }

    /// One fixed signed manifest URL per channel, independent of the latest-release redirect.
    pub fn manifest_url(self) -> String {
        format!(
            "https://github.com/{}/releases/download/{}/update-{}.json",
            super::platforms::repository(),
            self.name(),
            self.name()
        )
    }
}
