//! Vanilla's vibration system: the delayed, frequency-tagged view of game events that
//! sculk blocks listen to.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::game_events::GameEventRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{
    REGISTRY, RegistryExt as _, TaggedRegistryExt as _, blocks::block_state_ext::BlockStateExt as _,
};
use steel_utils::{BlockPos, Direction, Identifier};

use super::GameEventContext;
use crate::world::World;

/// Vanilla `VibrationSystem.NO_VIBRATION_FREQUENCY`: an event no sculk block reacts to.
pub const NO_VIBRATION_FREQUENCY: i32 = 0;
/// Vanilla's highest vibration frequency.
pub const MAX_VIBRATION_FREQUENCY: i32 = 15;

/// Vanilla `VibrationSystem.VIBRATION_FREQUENCY_FOR_EVENT`.
///
/// Transcribed from `VibrationSystem`; events absent from the table produce
/// [`NO_VIBRATION_FREQUENCY`].
#[must_use]
pub fn game_event_frequency(event: GameEventRef) -> i32 {
    if event.key.namespace != Identifier::VANILLA_NAMESPACE {
        return NO_VIBRATION_FREQUENCY;
    }

    match event.key.path.as_ref() {
        "step" | "swim" | "flap" => 1,
        "projectile_land" | "hit_ground" | "splash" | "bounce" => 2,
        "item_interact_finish" | "projectile_shoot" | "instrument_play" => 3,
        "entity_action" | "elytra_glide" | "unequip" => 4,
        "entity_dismount" | "equip" => 5,
        "entity_interact" | "shear" | "entity_mount" => 6,
        "entity_damage" => 7,
        "drink" | "eat" => 8,
        "container_close" | "block_close" | "block_deactivate" | "block_detach" => 9,
        "container_open" | "block_open" | "block_activate" | "block_attach" | "prime_fuse"
        | "note_block_play" => 10,
        "block_change" => 11,
        "block_destroy" | "fluid_pickup" => 12,
        "block_place" | "fluid_place" => 13,
        "entity_place" | "lightning_strike" | "teleport" => 14,
        "entity_die" | "explode" => 15,
        resonance => resonance_frequency(resonance),
    }
}

/// Maps `resonate_1`..`resonate_15` onto their own frequency.
fn resonance_frequency(path: &str) -> i32 {
    let Some(index) = path.strip_prefix("resonate_") else {
        return NO_VIBRATION_FREQUENCY;
    };
    index
        .parse::<i32>()
        .ok()
        .filter(|frequency| (1..=MAX_VIBRATION_FREQUENCY).contains(frequency))
        .unwrap_or(NO_VIBRATION_FREQUENCY)
}

/// Vanilla `VibrationSystem.getResonanceEventByFrequency`.
#[must_use]
pub fn resonance_event(frequency: i32) -> Option<GameEventRef> {
    if !(1..=MAX_VIBRATION_FREQUENCY).contains(&frequency) {
        return None;
    }
    REGISTRY
        .game_events
        .by_key(&Identifier::vanilla(format!("resonate_{frequency}")))
}

/// Vanilla `VibrationSystem.getRedstoneStrengthForDistance`.
#[must_use]
pub fn redstone_strength_for_distance(distance: f32, listener_radius: i32) -> i32 {
    if listener_radius <= 0 {
        return MAX_VIBRATION_FREQUENCY;
    }
    let power_scale = 15.0 / f64::from(listener_radius);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "mirrors vanilla's Mth.floor on the scaled distance"
    )]
    let scaled = (power_scale * f64::from(distance)).floor() as i32;
    (15 - scaled).max(1)
}

/// Vanilla `VibrationInfo`: one vibration travelling towards a listener.
#[derive(Debug, Clone, Copy)]
pub struct VibrationInfo {
    /// The event that produced this vibration.
    pub event: GameEventRef,
    /// Blocks between the source and the listener.
    pub distance: f32,
    /// Where the vibration started.
    pub pos: DVec3,
}

/// Vanilla `VibrationSelector`: keeps the best candidate seen during one game tick.
#[derive(Debug, Default)]
pub struct VibrationSelector {
    candidate: Option<(VibrationInfo, i64)>,
}

impl VibrationSelector {
    /// Vanilla `VibrationSelector.addCandidate`.
    pub fn add_candidate(&mut self, candidate: VibrationInfo, game_time: i64) {
        if self.should_replace(&candidate, game_time) {
            self.candidate = Some((candidate, game_time));
        }
    }

    /// Vanilla `VibrationSelector.shouldReplaceVibration`: within a tick, the nearest wins,
    /// and ties go to the higher frequency.
    fn should_replace(&self, candidate: &VibrationInfo, game_time: i64) -> bool {
        let Some((current, tick)) = &self.candidate else {
            return true;
        };
        if game_time != *tick {
            return false;
        }
        if candidate.distance < current.distance {
            return true;
        }
        if candidate.distance > current.distance {
            return false;
        }
        game_event_frequency(candidate.event) > game_event_frequency(current.event)
    }

    /// Vanilla `VibrationSelector.chosenCandidate`: a candidate is only chosen on a later
    /// tick than the one it arrived on.
    #[must_use]
    pub fn chosen_candidate(&self, game_time: i64) -> Option<VibrationInfo> {
        self.candidate
            .as_ref()
            .filter(|(_, tick)| *tick < game_time)
            .map(|(candidate, _)| *candidate)
    }

    /// Vanilla `VibrationSelector.startOver`.
    pub const fn start_over(&mut self) {
        self.candidate = None;
    }
}

/// Vanilla `VibrationSystem.Data`: the vibration in flight plus the pending candidate.
#[derive(Debug, Default)]
pub struct VibrationData {
    /// The vibration currently travelling to the listener.
    pub current: Option<VibrationInfo>,
    /// Ticks left before `current` arrives.
    pub travel_ticks: i32,
    /// Candidate selection for the current tick.
    pub selector: VibrationSelector,
}

/// Vanilla `VibrationSystem.User`: what a listening block decides about vibrations.
pub trait VibrationUser {
    /// Blocks away this user can hear.
    fn listener_radius(&self) -> i32;

    /// Where the user itself sits.
    fn listener_pos(&self) -> DVec3;

    /// Vanilla `User.isValidVibration`: events with no frequency are ignored outright.
    fn is_valid_vibration(&self, event: GameEventRef, _context: &GameEventContext<'_>) -> bool {
        game_event_frequency(event) != NO_VIBRATION_FREQUENCY
    }

    /// Vanilla `User.canReceiveVibration`.
    fn can_receive_vibration(
        &self,
        world: &Arc<World>,
        source_pos: BlockPos,
        event: GameEventRef,
        context: &GameEventContext<'_>,
    ) -> bool;

    /// Vanilla `User.calculateTravelTimeInTicks`: one tick per block.
    #[must_use]
    fn calculate_travel_time_in_ticks(&self, distance: f32) -> i32 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "mirrors vanilla's float-to-int travel time"
        )]
        let ticks = distance as i32;
        ticks
    }

    /// Vanilla `User.onReceiveVibration`.
    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        source_pos: BlockPos,
        event: GameEventRef,
        distance: f32,
    );
}

/// Vanilla `VibrationSystem.Listener.isOccluded`: a vibration is blocked only when every
/// nudged line from the source to the listener crosses an occluding block.
#[must_use]
pub fn is_occluded(world: &Arc<World>, origin: DVec3, destination: DVec3) -> bool {
    let from = DVec3::new(
        origin.x.floor() + 0.5,
        origin.y.floor() + 0.5,
        origin.z.floor() + 0.5,
    );
    let to = DVec3::new(
        destination.x.floor() + 0.5,
        destination.y.floor() + 0.5,
        destination.z.floor() + 0.5,
    );

    for direction in Direction::ALL {
        let offset = direction.offset_vec();
        let nudged = from
            + DVec3::new(
                f64::from(offset.x),
                f64::from(offset.y),
                f64::from(offset.z),
            ) * 1.0e-5;

        let blocked = world.is_block_state_in_line(nudged, to, &|state| {
            REGISTRY
                .blocks
                .is_in_tag(state.get_block(), &BlockTag::OCCLUDES_VIBRATION_SIGNALS)
        });
        if !blocked {
            return false;
        }
    }

    true
}

/// Vanilla `VibrationSystem.Listener.handleGameEvent`: queues a candidate vibration.
///
/// Returns whether the event was accepted.
pub fn handle_vibration_event(
    world: &Arc<World>,
    data: &mut VibrationData,
    user: &dyn VibrationUser,
    event: GameEventRef,
    context: &GameEventContext<'_>,
    source_pos: DVec3,
) -> bool {
    if data.current.is_some() || !user.is_valid_vibration(event, context) {
        return false;
    }

    let destination = user.listener_pos();
    if !user.can_receive_vibration(world, BlockPos::from(source_pos), event, context) {
        return false;
    }
    if is_occluded(world, source_pos, destination) {
        return false;
    }

    force_schedule_vibration(world, data, event, source_pos, destination);
    true
}

/// Vanilla `VibrationSystem.Listener.forceScheduleVibration`: skips the validity checks.
pub fn force_schedule_vibration(
    world: &Arc<World>,
    data: &mut VibrationData,
    event: GameEventRef,
    source_pos: DVec3,
    destination: DVec3,
) {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "vanilla stores the vibration distance as a float"
    )]
    let distance = source_pos.distance(destination) as f32;
    data.selector.add_candidate(
        VibrationInfo {
            event,
            distance,
            pos: source_pos,
        },
        world.game_time(),
    );
}

/// Vanilla `VibrationSystem.Ticker.tick`: promotes a candidate, then delivers it once it
/// has travelled far enough.
///
/// Vanilla also spawns the travelling vibration particle here, which Steel does not have.
pub fn tick_vibration(world: &Arc<World>, data: &mut VibrationData, user: &dyn VibrationUser) {
    if data.current.is_none()
        && let Some(chosen) = data.selector.chosen_candidate(world.game_time())
    {
        data.travel_ticks = user.calculate_travel_time_in_ticks(chosen.distance);
        data.current = Some(chosen);
        data.selector.start_over();
    }

    let Some(current) = data.current else {
        return;
    };

    data.travel_ticks = (data.travel_ticks - 1).max(0);
    if data.travel_ticks > 0 {
        return;
    }

    let origin = BlockPos::from(current.pos);
    let destination = BlockPos::from(user.listener_pos());
    let dx = f64::from(origin.x() - destination.x());
    let dy = f64::from(origin.y() - destination.y());
    let dz = f64::from(origin.z() - destination.z());
    #[expect(
        clippy::cast_possible_truncation,
        reason = "mirrors vanilla's float distance between block positions"
    )]
    let distance = dx.mul_add(dx, dy.mul_add(dy, dz * dz)).sqrt() as f32;

    user.on_receive_vibration(world, origin, current.event, distance);
    data.current = None;
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use steel_registry::game_events::GameEvent;
    use steel_registry::test_support::init_test_registry;
    use steel_registry::vanilla_game_events;

    use super::{
        NO_VIBRATION_FREQUENCY, VibrationInfo, VibrationSelector, game_event_frequency,
        redstone_strength_for_distance, resonance_event,
    };

    fn info(event: &'static GameEvent, distance: f32) -> VibrationInfo {
        VibrationInfo {
            event,
            distance,
            pos: DVec3::ZERO,
        }
    }

    #[test]
    fn frequencies_match_the_vanilla_table() {
        assert_eq!(game_event_frequency(&vanilla_game_events::STEP), 1);
        assert_eq!(game_event_frequency(&vanilla_game_events::SHEAR), 6);
        assert_eq!(game_event_frequency(&vanilla_game_events::BLOCK_CHANGE), 11);
        assert_eq!(game_event_frequency(&vanilla_game_events::EXPLODE), 15);
        assert_eq!(game_event_frequency(&vanilla_game_events::RESONATE_7), 7);
        // Not every game event is a vibration.
        assert_eq!(
            game_event_frequency(&vanilla_game_events::SCULK_SENSOR_TENDRILS_CLICKING),
            NO_VIBRATION_FREQUENCY
        );
    }

    #[test]
    fn every_frequency_has_a_resonance_event_of_its_own_frequency() {
        init_test_registry();
        for frequency in 1..=15 {
            let event = resonance_event(frequency).expect("resonance event should exist");
            assert_eq!(game_event_frequency(event), frequency);
        }
        assert!(resonance_event(0).is_none());
        assert!(resonance_event(16).is_none());
    }

    #[test]
    fn redstone_strength_falls_off_with_distance_and_never_reaches_zero() {
        assert_eq!(redstone_strength_for_distance(0.0, 8), 15);
        assert_eq!(redstone_strength_for_distance(8.0, 8), 1);
        // A calibrated sensor spreads the same 15 levels over twice the range.
        assert_eq!(redstone_strength_for_distance(8.0, 16), 8);
    }

    #[test]
    fn the_nearest_candidate_of_a_tick_wins_and_ties_go_to_the_higher_frequency() {
        let mut selector = VibrationSelector::default();
        selector.add_candidate(info(&vanilla_game_events::STEP, 5.0), 10);
        selector.add_candidate(info(&vanilla_game_events::EXPLODE, 9.0), 10);
        // Farther loses even at a much higher frequency.
        assert_eq!(
            selector.chosen_candidate(11).map(|chosen| chosen.distance),
            Some(5.0)
        );

        selector.add_candidate(info(&vanilla_game_events::EXPLODE, 5.0), 10);
        assert_eq!(
            selector
                .chosen_candidate(11)
                .map(|chosen| game_event_frequency(chosen.event)),
            Some(15)
        );
    }

    #[test]
    fn a_candidate_is_not_chosen_on_the_tick_it_arrived() {
        let mut selector = VibrationSelector::default();
        selector.add_candidate(info(&vanilla_game_events::STEP, 1.0), 42);

        assert!(selector.chosen_candidate(42).is_none());
        assert!(selector.chosen_candidate(43).is_some());
    }

    #[test]
    fn a_later_tick_cannot_replace_the_candidate_until_it_is_consumed() {
        let mut selector = VibrationSelector::default();
        selector.add_candidate(info(&vanilla_game_events::STEP, 9.0), 10);
        selector.add_candidate(info(&vanilla_game_events::STEP, 1.0), 11);

        assert_eq!(
            selector.chosen_candidate(12).map(|chosen| chosen.distance),
            Some(9.0)
        );
    }
}
