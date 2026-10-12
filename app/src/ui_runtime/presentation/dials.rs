//! Advances the local player's compass and clock needles once per tick and publishes the
//! strip frames every dial icon shows.

use bevy::prelude::*;
use client_shell::ui_resources::UiPresentationResource;
use inventory::{ClockReading, CompassReading, DialFrames, DialNeedle};

use crate::runtime::world::ClientWorld;

/// The local player's dial needles, the tick they last advanced on, and the random state
/// their spin draws from.
#[derive(Default)]
pub(crate) struct DialNeedles {
    compass: DialNeedle,
    clock: DialNeedle,
    advanced_tick: Option<u64>,
    random: u64,
}

impl DialNeedles {
    /// A uniform draw in `0..1` from a xorshift sequence.
    fn next_random(&mut self) -> f32 {
        let mut state = self.random.max(1);
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.random = state;
        (state >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Advances both needles one tick for a holder at `position` facing `yaw` degrees in
    /// `dimension`, under the world spawn and the sky's `celestial_angle`.
    fn advance(
        &mut self,
        dimension: i32,
        world_spawn: [i32; 3],
        position: [f32; 3],
        yaw: f32,
        celestial_angle: f32,
    ) -> DialFrames {
        let natural = inventory::natural_dimension(dimension);
        let compass = if natural {
            CompassReading::Target {
                target: [world_spawn[0], world_spawn[2]],
                position: [position[0], position[2]],
                yaw,
            }
        } else {
            CompassReading::Spin {
                random: self.next_random(),
            }
        };
        let clock = if natural {
            ClockReading::Sky { celestial_angle }
        } else {
            ClockReading::Spin {
                random: self.next_random(),
            }
        };
        DialFrames {
            compass: Some(self.compass.update_compass(compass, false)),
            clock: Some(self.clock.update_clock(clock, false)),
        }
    }
}

/// Advances the dial needles on each new movement tick and publishes their frames to the
/// UI icons and held items; without a local player, dials keep their static icons.
pub(crate) fn advance_dials(
    client_world: Res<ClientWorld>,
    movement: Res<client_interaction::movement::MovementTicker>,
    clock: Res<crate::environment::WorldClock>,
    time: Res<Time<Real>>,
    mut presentation: ResMut<UiPresentationResource>,
    equipment: Option<ResMut<client_presentation::presentation::equipment::EquipmentRuntime>>,
    mut needles: Local<DialNeedles>,
    mut frames: Local<DialFrames>,
) {
    let local = client_world.stream.as_ref().and_then(|stream| {
        let authority = stream.authority();
        let actor = authority.actor(stream.local_player_runtime_id())?;
        Some((
            authority.current_dimension(),
            authority.world_spawn(),
            actor.position,
            actor.yaw,
        ))
    });
    let Some((dimension, world_spawn, position, yaw)) = local else {
        *needles = DialNeedles::default();
        *frames = DialFrames::default();
        publish(&mut presentation, equipment, *frames);
        return;
    };
    let tick = movement.completed_tick();
    if needles.advanced_tick != Some(tick) {
        needles.advanced_tick = Some(tick);
        let celestial_angle = render_model::celestial_angle(crate::environment::visual_world_time(
            *clock,
            time.elapsed_secs_f64(),
        ));
        *frames = needles.advance(dimension, world_spawn, position, yaw, celestial_angle);
    }
    publish(&mut presentation, equipment, *frames);
}

/// Hands `frames` to the UI icons and held items.
fn publish(
    presentation: &mut UiPresentationResource,
    equipment: Option<ResMut<client_presentation::presentation::equipment::EquipmentRuntime>>,
    frames: DialFrames,
) {
    presentation.set_dial_frames(frames);
    if let Some(mut equipment) = equipment {
        equipment.set_dial_frames(frames);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overworld_dials_read_spawn_and_sky_while_other_dimensions_spin() {
        let mut needles = DialNeedles::default();
        let mut overworld = needles.advance(0, [0, 64, 10], [0.0, 64.0, 0.0], 0.0, 0.25);
        for _ in 0..200 {
            overworld = needles.advance(0, [0, 64, 10], [0.0, 64.0, 0.0], 0.0, 0.25);
        }
        assert_eq!(overworld.compass, Some(16), "settles on the spawn straight ahead");
        assert_eq!(overworld.clock, Some(16), "settles on the sky reading");

        let mut nether = DialNeedles::default();
        let readings: Vec<_> = (0..40)
            .map(|_| nether.advance(1, [0, 64, 10], [0.0, 64.0, 0.0], 0.0, 0.25))
            .collect();
        assert!(
            readings.windows(2).any(|pair| pair[0].compass != pair[1].compass),
            "the compass wanders without a target"
        );
    }
}
