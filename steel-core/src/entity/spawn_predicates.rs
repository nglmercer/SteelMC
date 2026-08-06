//! Vanilla `SpawnPlacements.SpawnPredicate` implementations.
//!
//! Vanilla registers one predicate per `EntityType` in the `SpawnPlacements` static
//! initializer, most of them shared static methods on `Mob`, `Monster`, `Animal`, or
//! `WaterAnimal`. This module ports those predicates plus the per-type overrides, keyed by
//! the same table that carries the placement types in [`super::spawn_placements`].
//!
//! Every predicate keeps vanilla's short-circuit order, because several of them consume
//! random numbers and reordering the checks would change how many draws happen.
//!
//! DIVERGENCE: vanilla's light queries pass `Level.getSkyDarken()`; Steel has no
//! sky-darkening accumulator yet and every `raw_brightness` caller in the tree passes `0`,
//! so these do the same. This makes monsters slightly harder to spawn during rain.

use std::sync::Arc;

use rand::RngExt as _;
use steel_utils::random::Random as _;
use steel_registry::{
    REGISTRY, TaggedRegistryExt,
    blocks::block_state_ext::BlockStateExt as _,
    entity_type::EntityTypeRef,
    fluid::is_water_fluid,
    vanilla_biome_tags::BiomeTag,
    vanilla_block_tags::BlockTag,
    vanilla_blocks,
};
use steel_utils::{BlockPos, types::Difficulty};

use crate::{
    chunk::{heightmap::HeightmapType, light::LightLayer},
    entity::{Entity as _, EntitySpawnReason},
    world::{LevelReader as _, World},
};

/// A vanilla `SpawnPlacements.SpawnPredicate`.
pub type SpawnPredicate =
    fn(EntityTypeRef, &Arc<World>, EntitySpawnReason, BlockPos, &mut dyn rand::Rng) -> bool;

// ---------------------------------------------------------------------------
// Shared helpers

fn below_has_tag(world: &Arc<World>, pos: BlockPos, tag: &steel_utils::Identifier) -> bool {
    let below = pos.below();
    REGISTRY
        .blocks
        .is_in_tag(world.get_block_state(below).get_block(), tag)
}

fn biome_has_tag(world: &Arc<World>, pos: BlockPos, tag: &steel_utils::Identifier) -> bool {
    world
        .biome_at(pos)
        .is_some_and(|biome| REGISTRY.biomes.is_in_tag(biome, tag))
}

fn is_water_at(world: &Arc<World>, pos: BlockPos) -> bool {
    is_water_fluid(world.get_block_state(pos).get_fluid_state().fluid_id)
}

fn is_water_block_at(world: &Arc<World>, pos: BlockPos) -> bool {
    world.get_block_state(pos).get_block() == &vanilla_blocks::WATER
}

/// Returns vanilla `LevelReader.canSeeSkyFromBelowWater`.
fn can_see_sky_from_below_water(world: &Arc<World>, pos: BlockPos) -> bool {
    let sea_level = world.sea_level;
    if pos.y() >= sea_level {
        return world.can_see_sky(pos);
    }

    let scan_point = BlockPos::new(pos.x(), sea_level, pos.z());
    if !world.can_see_sky(scan_point) {
        return false;
    }

    let mut cursor = scan_point.below();
    while cursor.y() > pos.y() {
        let state = world.get_block_state(cursor);
        if state.get_light_dampening() > 0 && state.get_fluid_state().is_empty() {
            return false;
        }
        cursor = cursor.below();
    }
    true
}

/// Returns whether vanilla `EntityGetter.getNearestPlayer(x, y, z, range, true)` would find
/// a player: the `filterOutCreative` overload, which skips creative-mode and spectators.
fn has_nearest_player_within(world: &Arc<World>, pos: BlockPos, range: f64) -> bool {
    let x = f64::from(pos.x()) + 0.5;
    let y = f64::from(pos.y()) + 0.5;
    let z = f64::from(pos.z()) + 0.5;
    let mut found = false;
    world.players.iter_players(|_, player| {
        if player.is_spectator() || player.has_infinite_materials() {
            return true;
        }
        let position = player.position();
        let dx = position.x - x;
        let dy = position.y - y;
        let dz = position.z - z;
        if dx.mul_add(dx, dy.mul_add(dy, dz * dz)) < range * range {
            found = true;
            return false;
        }
        true
    });
    found
}

/// Returns vanilla `Monster.isDarkEnoughToSpawn`.
pub fn is_dark_enough_to_spawn(world: &Arc<World>, pos: BlockPos, rng: &mut dyn rand::Rng) -> bool {
    if i32::from(world.light_value_at(LightLayer::Sky, pos)) > rng.random_range(0..32) {
        return false;
    }

    let block_light_limit = world.dimension_type.monster_spawn_block_light_limit;
    if block_light_limit < 15
        && i32::from(world.light_value_at(LightLayer::Block, pos)) > block_light_limit
    {
        return false;
    }

    let sky_darkening = if world.is_thundering() { 10 } else { 0 };
    let brightness = i32::from(world.max_local_raw_brightness(pos, sky_darkening));
    brightness <= sample_monster_spawn_light_test(world, rng)
}

/// Returns vanilla `DimensionType.monsterSpawnLightTest().sample(random)`.
fn sample_monster_spawn_light_test(world: &Arc<World>, rng: &mut dyn rand::Rng) -> i32 {
    use steel_registry::dimension_type::MonsterSpawnLightLevel;
    match world.dimension_type.monster_spawn_light_level {
        MonsterSpawnLightLevel::Simple(value) => value,
        MonsterSpawnLightLevel::Complex {
            min_inclusive,
            max_inclusive,
            ..
        } => {
            if min_inclusive >= max_inclusive {
                min_inclusive
            } else {
                rng.random_range(min_inclusive..=max_inclusive)
            }
        }
    }
}

/// Returns vanilla `Animal.isBrightEnoughToSpawn`.
pub fn is_bright_enough_to_spawn(world: &Arc<World>, pos: BlockPos) -> bool {
    world.raw_brightness(pos, 0) > 8
}

// ---------------------------------------------------------------------------
// Shared predicates

/// Vanilla `Mob.checkMobSpawnRules`.
pub fn check_mob_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    if spawn_reason.is_spawner() {
        return true;
    }
    let below = pos.below();
    let below_state = world.get_block_state(below);
    crate::behavior::BLOCK_BEHAVIORS
        .get_behavior(below_state.get_block())
        .is_valid_spawn(below_state, world.as_ref(), below, entity_type)
}

/// Vanilla `Monster.checkMonsterSpawnRules`.
pub fn check_monster_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    (spawn_reason.ignores_light_requirements() || is_dark_enough_to_spawn(world, pos, rng))
        && check_mob_spawn_rules(entity_type, world, spawn_reason, pos, rng)
}

/// Vanilla `Monster.checkAnyLightMonsterSpawnRules`.
pub fn check_any_light_monster_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    check_mob_spawn_rules(entity_type, world, spawn_reason, pos, rng)
}

/// Vanilla `Monster.checkSurfaceMonstersSpawnRules`.
pub fn check_surface_monsters_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    check_monster_spawn_rules(entity_type, world, spawn_reason, pos, rng)
        && (spawn_reason.is_spawner() || world.can_see_sky(pos))
}

/// Vanilla `Animal.checkAnimalSpawnRules`.
pub fn check_animal_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    let bright_enough =
        spawn_reason.ignores_light_requirements() || is_bright_enough_to_spawn(world, pos);
    below_has_tag(world, pos, &BlockTag::ANIMALS_SPAWNABLE_ON) && bright_enough
}

/// Vanilla `WaterAnimal.checkSurfaceWaterAnimalSpawnRules`, shared verbatim with
/// `AgeableWaterCreature.checkSurfaceAgeableWaterCreatureSpawnRules`.
pub fn check_surface_water_animal_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    let sea_level = world.sea_level;
    pos.y() >= sea_level - 13
        && pos.y() <= sea_level
        && is_water_at(world, pos.below())
        && is_water_block_at(world, pos.above())
}

/// Vanilla `PatrollingMonster.checkPatrollingMonsterSpawnRules`.
pub fn check_patrolling_monster_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    if world.light_value_at(LightLayer::Block, pos) > 8 {
        return false;
    }
    check_any_light_monster_spawn_rules(entity_type, world, spawn_reason, pos, rng)
}

// ---------------------------------------------------------------------------
// Per-type predicates

/// Vanilla `Axolotl.checkAxolotlSpawnRules`.
pub fn check_axolotl_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::AXOLOTLS_SPAWNABLE_ON)
}

/// Vanilla `Drowned.checkDrownedSpawnRules`.
pub fn check_drowned_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    if !is_water_at(world, pos.below()) && !spawn_reason.is_spawner() {
        return false;
    }

    let can_monster_spawn = world.difficulty() != Difficulty::Peaceful
        && (spawn_reason.ignores_light_requirements() || is_dark_enough_to_spawn(world, pos, rng))
        && (spawn_reason.is_spawner() || is_water_at(world, pos));

    if can_monster_spawn
        && (spawn_reason.is_spawner() || spawn_reason == EntitySpawnReason::Reinforcement)
    {
        return true;
    }

    if biome_has_tag(world, pos, &BiomeTag::MORE_FREQUENT_DROWNED_SPAWNS) {
        rng.random_range(0..15) == 0 && can_monster_spawn
    } else {
        rng.random_range(0..40) == 0
            && pos.y() < world.sea_level - 5
            && can_monster_spawn
    }
}

/// Vanilla `Guardian.checkGuardianSpawnRules`.
pub fn check_guardian_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    (rng.random_range(0..20) == 0 || !can_see_sky_from_below_water(world, pos))
        && world.difficulty() != Difficulty::Peaceful
        && (spawn_reason.is_spawner() || is_water_at(world, pos))
        && is_water_at(world, pos.below())
}

/// Vanilla `TropicalFish.checkTropicalFishSpawnRules`.
pub fn check_tropical_fish_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    is_water_at(world, pos.below())
        && is_water_block_at(world, pos.above())
        && (biome_has_tag(
            world,
            pos,
            &BiomeTag::ALLOWS_TROPICAL_FISH_SPAWNS_AT_ANY_HEIGHT,
        ) || check_surface_water_animal_spawn_rules(entity_type, world, spawn_reason, pos, rng))
}

/// Vanilla `AbstractNautilus.checkNautilusSpawnRules`.
pub fn check_nautilus_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    let sea_level = world.sea_level;
    pos.y() >= sea_level - 25
        && pos.y() <= sea_level - 5
        && is_water_at(world, pos.below())
        && is_water_block_at(world, pos.above())
}

/// Vanilla `GlowSquid.checkGlowSquidSpawnRules`.
pub fn check_glow_squid_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    pos.y() <= world.sea_level - 33
        && world.raw_brightness(pos, 0) == 0
        && is_water_block_at(world, pos)
}

/// Vanilla `Armadillo.checkArmadilloSpawnRules`.
pub fn check_armadillo_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::ARMADILLO_SPAWNABLE_ON)
        && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Bat.checkBatSpawnRules`.
pub fn check_bat_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    let surface = world
        .height_at(HeightmapType::WorldSurface, pos.x(), pos.z())
        .unwrap_or_else(|| world.get_min_y());
    if pos.y() >= surface {
        return false;
    }
    if rng.random::<bool>() {
        return false;
    }
    if i32::from(world.max_local_raw_brightness(pos, 0)) > rng.random_range(0..4) {
        return false;
    }
    below_has_tag(world, pos, &BlockTag::BATS_SPAWNABLE_ON)
        && check_mob_spawn_rules(entity_type, world, spawn_reason, pos, rng)
}

/// Vanilla `Camel.checkCamelSpawnRules`.
pub fn check_camel_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::CAMELS_SPAWNABLE_ON)
        && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Frog.checkFrogSpawnRules`.
pub fn check_frog_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::FROGS_SPAWNABLE_ON)
        && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Goat.checkGoatSpawnRules`.
pub fn check_goat_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::GOATS_SPAWNABLE_ON) && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `MushroomCow.checkMushroomSpawnRules`.
pub fn check_mushroom_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::MOOSHROOMS_SPAWNABLE_ON)
        && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Parrot.checkParrotSpawnRules`.
pub fn check_parrot_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::PARROTS_SPAWNABLE_ON)
        && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Rabbit.checkRabbitSpawnRules`.
pub fn check_rabbit_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::RABBITS_SPAWNABLE_ON)
        && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Wolf.checkWolfSpawnRules`.
pub fn check_wolf_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::WOLVES_SPAWNABLE_ON) && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Fox.checkFoxSpawnRules`.
pub fn check_fox_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    below_has_tag(world, pos, &BlockTag::FOXES_SPAWNABLE_ON) && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Ocelot.checkOcelotSpawnRules`.
pub fn check_ocelot_spawn_rules(
    _entity_type: EntityTypeRef,
    _world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    _pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    rng.random_range(0..3) != 0
}

/// Vanilla `PolarBear.checkPolarBearSpawnRules`.
pub fn check_polar_bear_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    if biome_has_tag(world, pos, &BiomeTag::POLAR_BEARS_SPAWN_ON_ALTERNATE_BLOCKS) {
        is_bright_enough_to_spawn(world, pos)
            && below_has_tag(world, pos, &BlockTag::POLAR_BEARS_SPAWNABLE_ON_ALTERNATE)
    } else {
        check_animal_spawn_rules(entity_type, world, spawn_reason, pos, rng)
    }
}

/// Vanilla `SkeletonHorse.checkSkeletonHorseSpawnRules`.
pub fn check_skeleton_horse_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    if spawn_reason.is_spawner() {
        spawn_reason.ignores_light_requirements() || is_bright_enough_to_spawn(world, pos)
    } else {
        check_animal_spawn_rules(entity_type, world, spawn_reason, pos, rng)
    }
}

/// Vanilla `Turtle.checkTurtleSpawnRules`.
pub fn check_turtle_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    // `TurtleEggBlock.onSand` checks the block two below the spawn position.
    let sand = REGISTRY.blocks.is_in_tag(
        world.get_block_state(pos.below().below()).get_block(),
        &BlockTag::SAND,
    );
    pos.y() < world.sea_level + 4 && sand && is_bright_enough_to_spawn(world, pos)
}

/// Vanilla `Endermite.checkEndermiteSpawnRules` and `Silverfish.checkSilverfishSpawnRules`,
/// which are byte-identical.
pub fn check_endermite_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    if !check_any_light_monster_spawn_rules(entity_type, world, spawn_reason, pos, rng) {
        return false;
    }
    if spawn_reason.is_spawner() {
        return true;
    }
    !has_nearest_player_within(world, pos, 5.0)
}

/// Vanilla `Ghast.checkGhastSpawnRules`.
pub fn check_ghast_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    world.difficulty() != Difficulty::Peaceful
        && rng.random_range(0..20) == 0
        && check_mob_spawn_rules(entity_type, world, spawn_reason, pos, rng)
}

/// Vanilla `MagmaCube.checkMagmaCubeSpawnRules`.
pub fn check_magma_cube_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    _pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    world.difficulty() != Difficulty::Peaceful
}

/// Vanilla `SulfurCube.checkSulfurCubeSpawnRules`, which is unconditionally `true`.
pub fn check_sulfur_cube_spawn_rules(
    _entity_type: EntityTypeRef,
    _world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    _pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    true
}

/// Vanilla `Slime.checkSlimeSpawnRules`.
///
/// DIVERGENCE: the surface-swamp branch reads
/// `EnvironmentAttributes.SURFACE_SLIME_SPAWN_CHANCE`, whose overworld timeline sets it to
/// `MOON_BRIGHTNESS_PER_PHASE[phase] * 0.5`. Steel has no environment-attribute system, so
/// the moon-phase value is computed directly from day time here.
pub fn check_slime_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    if world.difficulty() == Difficulty::Peaceful {
        return false;
    }

    if spawn_reason.is_spawner() {
        return check_mob_spawn_rules(entity_type, world, spawn_reason, pos, rng);
    }

    if biome_has_tag(world, pos, &BiomeTag::ALLOWS_SURFACE_SLIME_SPAWNS)
        && pos.y() > 50
        && pos.y() < 70
    {
        let surface_chance = surface_slime_spawn_chance(world);
        if rng.random::<f32>() < surface_chance
            && i32::from(world.max_local_raw_brightness(pos, 0)) <= rng.random_range(0..8)
        {
            return check_mob_spawn_rules(entity_type, world, spawn_reason, pos, rng);
        }
    }

    let chunk = steel_utils::ChunkPos::from_block_pos(pos);
    let slime_chunk = seed_slime_chunk(chunk.0.x, chunk.0.y, world.seed(), 987_234_911) % 10 == 0;
    if rng.random_range(0..10) == 0 && slime_chunk && pos.y() < 40 {
        return check_mob_spawn_rules(entity_type, world, spawn_reason, pos, rng);
    }

    false
}

/// Vanilla `DimensionType.MOON_BRIGHTNESS_PER_PHASE[phase] * 0.5`.
fn surface_slime_spawn_chance(world: &Arc<World>) -> f32 {
    const MOON_BRIGHTNESS_PER_PHASE: [f32; 8] = [1.0, 0.75, 0.5, 0.25, 0.0, 0.25, 0.5, 0.75];
    // DIVERGENCE: vanilla reads `dayTime`; Steel tracks only total game time, so a
    // `/time set` that desyncs the two would shift the phase here.
    let phase = (world.game_time() / 24_000).rem_euclid(8) as usize;
    MOON_BRIGHTNESS_PER_PHASE[phase] * 0.5
}

/// Returns the first draw of vanilla `WorldgenRandom.seedSlimeChunk`.
///
/// Vanilla seeds a thread-local `LegacyRandomSource` and takes `nextInt(10)`; only the
/// scrambled seed matters here, so this returns the equivalent bounded draw directly.
fn seed_slime_chunk(x: i32, z: i32, seed: i64, salt: i64) -> i64 {
    let x = i64::from(x);
    let z = i64::from(z);
    let scrambled = seed
        .wrapping_add(x.wrapping_mul(x).wrapping_mul(4_987_142))
        .wrapping_add(x.wrapping_mul(5_947_611))
        .wrapping_add(z.wrapping_mul(z).wrapping_mul(4_392_871))
        .wrapping_add(z.wrapping_mul(389_711))
        ^ salt;
    let mut random = steel_utils::random::legacy_random::LegacyRandom::from_seed(scrambled as u64);
    i64::from(random.next_i32_bounded(10))
}

/// Vanilla `Stray.checkStraySpawnRules`.
pub fn check_stray_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    let mut check_sky_pos = pos;
    loop {
        check_sky_pos = check_sky_pos.above();
        if world.get_block_state(check_sky_pos).get_block() != &vanilla_blocks::POWDER_SNOW {
            break;
        }
    }

    check_monster_spawn_rules(entity_type, world, spawn_reason, pos, rng)
        && (spawn_reason.is_spawner() || world.can_see_sky(check_sky_pos.below()))
}

/// Vanilla `Strider.checkStriderSpawnRules`.
pub fn check_strider_spawn_rules(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    let mut check_pos = pos;
    loop {
        check_pos = check_pos.above();
        if !is_lava_at(world, check_pos) {
            break;
        }
    }
    world.get_block_state(check_pos).is_air()
}

fn is_lava_at(world: &Arc<World>, pos: BlockPos) -> bool {
    steel_registry::fluid::is_lava_fluid(world.get_block_state(pos).get_fluid_state().fluid_id)
}

/// Vanilla `Hoglin.checkHoglinSpawnRules` and `Piglin.checkPiglinSpawnRules`, which share a
/// body.
pub fn check_not_on_nether_wart_block(
    _entity_type: EntityTypeRef,
    world: &Arc<World>,
    _spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    _rng: &mut dyn rand::Rng,
) -> bool {
    world.get_block_state(pos.below()).get_block() != &vanilla_blocks::NETHER_WART_BLOCK
}

/// Vanilla `ZombifiedPiglin.checkZombifiedPiglinSpawnRules`.
pub fn check_zombified_piglin_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    world.difficulty() != Difficulty::Peaceful
        && check_not_on_nether_wart_block(entity_type, world, spawn_reason, pos, rng)
}
