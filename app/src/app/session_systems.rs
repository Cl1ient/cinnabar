//! Session recovery and acceptance completion scheduling.

#[cfg(feature = "acceptance")]
use crate::runtime::{shutdown::finish_acceptance_run, telemetry::record_metrics};
use crate::{
    runtime::{network::receive_network_events, shutdown::exit_on_fatal_runtime_error},
    session::{follow_server_transfer, recover_session_failure},
};
use bevy::prelude::{App, IntoScheduleConfigs, Update};
use client_state::ClientFrameSet;

/// Orders session recovery before fatal exits and acceptance completion after publication.
pub(crate) fn configure_session_failure_systems(app: &mut App) {
    #[cfg(feature = "acceptance")]
    app.add_systems(
        Update,
        finish_acceptance_run
            .after(ClientFrameSet::NetworkSend)
            .after(ClientFrameSet::UiPublication)
            .after(record_metrics)
            .after(recover_session_failure),
    );
    app
        // The launcher gets first refusal on a fatal session error, so a failed
        // join returns to the menu instead of ending the process. This has to sit
        // after the failure is recorded (network drain) and before both systems
        // that act on it. The transfer follower runs first so a server-directed
        // move is classified as a replacement handoff, not a failure.
        .add_systems(
            Update,
            (follow_server_transfer, recover_session_failure)
                .chain()
                .after(receive_network_events)
                .after(ClientFrameSet::NetworkSend)
                .after(ClientFrameSet::UiPublication)
                .before(exit_on_fatal_runtime_error),
        );
}
