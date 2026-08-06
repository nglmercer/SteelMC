//! Village helpers — POI claiming and iron golem spawning.
//!
//! Mirrors vanilla `Villager` coordination with `PoiManager` and
//! `GolemSensor` / `SpawnUtil.trySpawnMob(IRON_GOLEM)` at a high level.
//! Full world-level spawning requires `ServerLevel` access; this module
//! provides the pure predicate and state-transition helpers that entity
//! ticks can call without pulling the entire village subsystem.

use std::time::Duration;

use steel_utils::BlockPos;

/// How long a villager must have slept recently to be eligible to summon a golem.
pub const GOLEM_SPAWN_SLEEP_WINDOW_TICKS: i64 = 24000;

/// How many villagers must agree that a golem is needed.
pub const GOLEMS_VILLAGERS_NEEDED: usize = 5;

/// Radius to search for agreeing villagers.
pub const GOLEM_SEARCH_RADIUS: f64 = 10.0;

/// Simple golem-spawn eligibility predicate (mirrors `Villager.wantsToSpawnGolem`).
#[must_use]
pub fn wants_to_spawn_golem(
    last_slept_tick: Option<i64>,
    game_time: i64,
    golem_detected_recently: bool,
) -> bool {
    if golem_detected_recently {
        return false;
    }
    match last_slept_tick {
        Some(t) => game_time - t < GOLEM_SPAWN_SLEEP_WINDOW_TICKS,
        None => false,
    }
}

/// Validates that a villager can still claim a POI (bed / workstation) at `pos`.
/// In the full server this consults `PoiManager.getType(pos)` and ticket counts;
/// here we expose the predicate so callers can wire the real check.
#[must_use]
pub fn can_claim_poi(free_tickets: u32) -> bool {
    free_tickets > 0
}

/// Computes a gossip decay interval helper (vanilla decays every 24000 ticks).
#[must_use]
pub const fn gossip_decay_interval() -> Duration {
    Duration::from_millis(24000 * 50) // 50ms per tick
}

/// Food points required for breeding (mirrors `Villager.FOOD_POINTS` aggregation).
pub const BREEDING_FOOD_THRESHOLD: i32 = 12;

/// Returns true if a villager with `food_level` and `inventory_food_points` can breed.
#[must_use]
pub const fn can_breed(food_level: i32, inventory_food_points: i32) -> bool {
    food_level + inventory_food_points >= BREEDING_FOOD_THRESHOLD
}

/// Workstation claim helper — returns the identifier for a villager's claimed job site if valid.
#[must_use]
pub fn claimed_workstation_is_valid(
    claimed_pos: Option<BlockPos>,
    free_tickets_at_pos: Option<u32>,
) -> bool {
    match (claimed_pos, free_tickets_at_pos) {
        (Some(_), Some(free)) => free == 0, // fully claimed
        _ => false,
    }
}
