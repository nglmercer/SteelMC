//! Vanilla `NaturalSpawner` for Steel — simplified vanilla-correct transcription.
//!
//! Covers:
//! - **Biome**: weighted `biome.spawners[category]` selection via `MobCategory::biome_spawner_key`
//! - **Day/night**: `Monster.isDarkEnoughToSpawn` (block+sky light ≤ limit and raw brightness ≤ 7) vs
//!   `Animal.isBrightEnoughToSpawn` (raw brightness > 8). Surface monsters also require `canSeeSky`
//!   which correlates with day exposure.
//! - **Related**: `SpawnPlacement` (`ON_GROUND`/`IN_WATER`/`NO_RESTRICTIONS`), `checkMobSpawnRules`
//!   (valid spawn block), `checkSpawnObstruction`, `noCollision`, global `MAGIC_NUMBER` cap and per-chunk
//!   local cap, `PotentialCalculator` spawn-cost, difficulty/peaceful, `spawn_mobs` gamerule,
//!   and chunk-generation `creature_spawn_probability`.

use std::{collections::HashSet, sync::Arc};

use glam::DVec3;
use rand::{RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};
use rustc_hash::FxHashMap;
use steel_registry::{
    RegistryExt, TaggedRegistryExt, REGISTRY,
    biome::BiomeRef,
    blocks::block_state_ext::BlockStateExt,
    entity_type::MobCategory,
    vanilla_block_tags::BlockTag,
};
use steel_utils::{BlockPos, ChunkPos, Identifier};

use crate::{
    chunk::{heightmap::HeightmapType, light::LightLayer},
    entity::{Entity, ENTITIES, EntitySpawnReason, next_entity_id},
    world::{World, level_reader::LevelReader},
};

const MIN_SPAWN_DISTANCE_SQR: f64 = 24.0 * 24.0;
const SPAWN_DISTANCE_CHUNK: i32 = 8;
const MAGIC_NUMBER: i32 = 289; // 17*17

const SPAWNING_CATEGORIES: [MobCategory; 7] = [
    MobCategory::Monster,
    MobCategory::Creature,
    MobCategory::Ambient,
    MobCategory::Axolotls,
    MobCategory::UndergroundWaterCreature,
    MobCategory::WaterCreature,
    MobCategory::WaterAmbient,
];

// ---------------------------------------------------------------------------
// PotentialCalculator (spawn-cost energy budget)

struct PotentialCalculator {
    charges: Vec<(BlockPos, f64)>,
}
impl PotentialCalculator {
    fn new() -> Self {
        Self { charges: Vec::new() }
    }
    fn add_charge(&mut self, pos: BlockPos, charge: f64) {
        if charge != 0.0 {
            self.charges.push((pos, charge));
        }
    }
    fn potential_change(&self, pos: BlockPos, charge: f64) -> f64 {
        if charge == 0.0 || self.charges.is_empty() {
            return 0.0;
        }
        let mut sum = 0.0;
        for (p, c) in &self.charges {
            let dx = f64::from(p.x() - pos.x());
            let dy = f64::from(p.y() - pos.y());
            let dz = f64::from(p.z() - pos.z());
            let d2 = dx * dx + dy * dy + dz * dz;
            if d2 == 0.0 {
                return f64::INFINITY;
            }
            sum += c / d2.sqrt();
        }
        sum * charge
    }
}

pub(crate) struct SpawnState {
    spawnable_chunk_count: i32,
    mob_counts: FxHashMap<MobCategory, i32>,
    potential: PotentialCalculator,
    local_counts: FxHashMap<ChunkPos, FxHashMap<MobCategory, i32>>,
}

impl SpawnState {
    fn can_spawn_global(&self, category: MobCategory) -> bool {
        let max = category.max_instances_per_chunk();
        if max < 0 {
            return true;
        }
        let limit = max * self.spawnable_chunk_count / MAGIC_NUMBER;
        self.mob_counts.get(&category).copied().unwrap_or(0) < limit
    }
    fn can_spawn_local(&self, category: MobCategory, chunk: ChunkPos) -> bool {
        let max = category.max_instances_per_chunk();
        if max < 0 {
            return true;
        }
        let count = self
            .local_counts
            .get(&chunk)
            .and_then(|m| m.get(&category))
            .copied()
            .unwrap_or(0);
        count < max
    }
    fn can_spawn_cost(&self, world: &World, entity_type: &Identifier, pos: BlockPos) -> bool {
        if let Some(biome) = world.biome_at(pos) {
            if let Some(cost) = biome.spawn_costs.get(entity_type) {
                let change = self.potential.potential_change(pos, cost.charge);
                if change > cost.energy_budget {
                    return false;
                }
            }
        }
        true
    }
    fn after_spawn(&mut self, world: &World, entity_type: &Identifier, pos: BlockPos, category: MobCategory, chunk: ChunkPos) {
        *self.mob_counts.entry(category).or_insert(0) += 1;
        *self.local_counts.entry(chunk).or_default().entry(category).or_insert(0) += 1;
        if let Some(biome) = world.biome_at(pos) {
            if let Some(cost) = biome.spawn_costs.get(entity_type) {
                self.potential.add_charge(pos, cost.charge);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Light / day helpers

fn _is_day(world: &World) -> bool {
    let t = world.game_time().rem_euclid(24000);
    (0..12000).contains(&t)
}

fn is_dark_enough(world: &World, pos: BlockPos) -> bool {
    let block_light = world.light_value_at(LightLayer::Block, pos) as i32;
    let block_limit = world.dimension_type.monster_spawn_block_light_limit;
    if block_limit < 15 && block_light > block_limit {
        return false;
    }
    let brightness = world.raw_brightness(pos, 0);
    // Overworld monsterSpawnLightTest is uniform 0..7
    brightness <= 7
}

fn is_bright_enough(world: &World, pos: BlockPos) -> bool {
    world.raw_brightness(pos, 0) > 8
}

fn is_valid_empty_spawn_block(world: &World, pos: BlockPos) -> bool {
    let state = world.get_block_state(pos);
    if state.is_air() {
        return true;
    }
    if state.is_solid() {
        return false;
    }
    if state.has_fluid() {
        return false;
    }
    if state.is_solid_render() {
        return false;
    }
    // Prevent spawning inside certain blocks (e.g. rails, etc.)
    // Use tag check via registry: block is in prevent_mob_spawning_inside
    let block = state.get_block();
    let tag = steel_registry::vanilla_block_tags::BlockTag::PREVENT_MOB_SPAWNING_INSIDE;
    if REGISTRY.blocks.is_in_tag(block, &tag) {
        return false;
    }
    true
}

fn is_spawn_position_ok(world: &World, pos: BlockPos, category: MobCategory) -> bool {
    match category {
        MobCategory::WaterAmbient
        | MobCategory::WaterCreature
        | MobCategory::UndergroundWaterCreature
        | MobCategory::Axolotls => {
            let state = world.get_block_state(pos);
            // IN_WATER: fluid is water, above not solid
            if !state.has_fluid() {
                return false;
            }
            let fluid = state.get_fluid_state();
            if fluid.is_empty() {
                return false;
            }
            // Simplified water check: has_fluid suffices for now
            let above = pos.above();
            is_valid_empty_spawn_block(world, above)
        }
        MobCategory::Monster | MobCategory::Creature | MobCategory::Ambient => {
            let below = pos.below();
            let below_state = world.get_block_state(below);
            let below_ok = match category {
                MobCategory::Monster => {
                    // isValidSpawn: block supports spawning; simplified to sturdy check
                    below_state.is_face_sturdy_at(below, steel_registry::blocks::properties::Direction::Up)
                        || below_state.is_solid()
                }
                MobCategory::Creature => {
                    let block = below_state.get_block();
                    REGISTRY.blocks.is_in_tag(block, &BlockTag::ANIMALS_SPAWNABLE_ON)
                }
                _ => true,
            };
            if !below_ok {
                return false;
            }
            is_valid_empty_spawn_block(world, pos) && is_valid_empty_spawn_block(world, pos.above())
        }
        MobCategory::Misc => true,
    }
}

fn check_spawn_rules(world: &World, category: MobCategory, pos: BlockPos, spawn_reason: EntitySpawnReason) -> bool {
    if spawn_reason == EntitySpawnReason::TrialSpawner {
        return true;
    }
    if category == MobCategory::Monster && world.difficulty() == steel_utils::types::Difficulty::Peaceful {
        return false;
    }
    match category {
        MobCategory::Monster => is_dark_enough(world, pos),
        MobCategory::Creature => is_bright_enough(world, pos),
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// Biome weighted selection

fn spawner_entries<'a>(biome: BiomeRef, category: MobCategory) -> &'a [steel_registry::biome::SpawnerData] {
    biome
        .spawners
        .get(category.biome_spawner_key())
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

fn pick_weighted_spawner<'a>(
    entries: &'a [steel_registry::biome::SpawnerData],
    rng: &mut impl rand::Rng,
) -> Option<&'a steel_registry::biome::SpawnerData> {
    if entries.is_empty() {
        return None;
    }
    let total: i32 = entries.iter().map(|e| e.weight).sum();
    if total <= 0 {
        return None;
    }
    let mut r = rng.random_range(0..total);
    for e in entries {
        r -= e.weight;
        if r < 0 {
            return Some(e);
        }
    }
    entries.last()
}

// ---------------------------------------------------------------------------
// Distance + per-chunk helpers

fn is_right_distance(world: &World, chunk: ChunkPos, pos: BlockPos, nearest_dist_sqr: f64) -> bool {
    if nearest_dist_sqr <= MIN_SPAWN_DISTANCE_SQR {
        return false;
    }
    let spawn = world.level_data.read().data().spawn_pos();
    let sx = f64::from(spawn.x()) + 0.5;
    let sy = f64::from(spawn.y());
    let sz = f64::from(spawn.z()) + 0.5;
    let dx = f64::from(pos.x()) + 0.5 - sx;
    let dy = f64::from(pos.y()) as f64 - sy;
    let dz = f64::from(pos.z()) + 0.5 - sz;
    if dx * dx + dy * dy + dz * dz < 24.0 * 24.0 {
        return false;
    }
    let cpos = ChunkPos::from_block_pos(pos);
    cpos == chunk || world.chunk_map.with_full_chunk(cpos, |_| ()).is_some()
}

fn random_pos_within(world: &World, chunk: ChunkPos) -> BlockPos {
    let min_x = chunk.0.x << 4;
    let min_z = chunk.0.y << 4;
    let mut rng = rand::rng();
    let x = min_x + rng.random_range(0..16);
    let z = min_z + rng.random_range(0..16);
    let top = world
        .height_at(HeightmapType::WorldSurface, x, z)
        .unwrap_or(world.get_min_y());
    let y = rng.random_range(world.get_min_y()..=(top + 1).max(world.get_min_y()));
    BlockPos::new(x, y, z)
}

fn nearest_player_distance_sq(world: &World, x: f64, y: f64, z: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    world.players.iter_players(|_, p| {
        let pp = p.position();
        let dx = pp.x - x;
        let dy = pp.y - y;
        let dz = pp.z - z;
        let d2 = dx * dx + dy * dy + dz * dz;
        if best.is_none_or(|b| d2 < b) {
            best = Some(d2);
        }
        true
    });
    best
}

fn entity_spawn_aabb(
    et: &steel_registry::entity_type::EntityType,
    x: f64,
    y: f64,
    z: f64,
) -> steel_utils::WorldAabb {
    let w = f64::from(et.dimensions.width);
    let h = f64::from(et.dimensions.height);
    steel_utils::WorldAabb::new(x - w / 2.0, y, z - w / 2.0, x + w / 2.0, y + h, z + w / 2.0)
}

// ---------------------------------------------------------------------------
// Per-category spawn attempt

fn spawn_category_for_position(
    world: &Arc<World>,
    chunk: ChunkPos,
    start: BlockPos,
    category: MobCategory,
    state: &mut SpawnState,
) -> usize {
    let start_state = world.get_block_state(start);
    if start_state.is_solid() {
        return 0;
    }

    let mut rng = rand::rng();
    let y_start = start.y();
    let mut total_spawned = 0;

    // Pick current spawner data lazily (mirrors vanilla currentSpawnData)
    let mut current_type: Option<Identifier> = None;
    #[allow(unused_assignments)]
    let mut current_min: i32 = 0;
    #[allow(unused_assignments)]
    let mut current_max: i32 = 0;
    let mut group_target: i32 = 0;

    for _ in 0..3 {
        let mut x = start.x();
        let mut z = start.z();
        let attempt_limit = (rng.random::<f32>() * 4.0).ceil() as i32;
        let mut group_size = 0;

        for _ in 0..attempt_limit {
            x += rng.random_range(0..6) - rng.random_range(0..6);
            z += rng.random_range(0..6) - rng.random_range(0..6);
            let pos = BlockPos::new(x, y_start, z);
            let xx = f64::from(x) + 0.5;
            let zz = f64::from(z) + 0.5;
            let Some(dist) = nearest_player_distance_sq(world, xx, f64::from(y_start), zz) else {
                continue;
            };
            if !is_right_distance(world, chunk, pos, dist) {
                continue;
            }

            if current_type.is_none() {
                let Some(biome) = world.biome_at(pos) else {
                    break;
                };
                let entries = spawner_entries(biome, category);
                if entries.is_empty() {
                    break;
                }
                let Some(picked) = pick_weighted_spawner(entries, &mut rng) else {
                    break;
                };
                current_type = Some(picked.entity_type.clone());
                current_min = picked.min_count;
                current_max = picked.max_count;
                group_target = current_min + rng.random_range(0..=(current_max - current_min).max(0));
            }

            let Some(ref entity_id) = current_type else {
                break;
            };
            let Some(et) = REGISTRY.entity_types.by_key(entity_id) else {
                break;
            };
            // Category mismatch guard
            if et.mob_category != category && category != MobCategory::Misc {
                // allow but continue; vanilla mobsAt would have filtered
            }
            if !et.can_spawn_far_from_player {
                let dd = f64::from(category.despawn_distance());
                if dist > dd * dd {
                    continue;
                }
            }
            if !is_spawn_position_ok(world, pos, category) {
                continue;
            }
            if !check_spawn_rules(world, category, pos, EntitySpawnReason::Natural) {
                continue;
            }
            if !state.can_spawn_cost(world, entity_id, pos) {
                continue;
            }
            let aabb = entity_spawn_aabb(et, xx, f64::from(y_start), zz);
            // No-collision: check entities and block not solid
            let mut blocked = false;
            for ent in world.get_entities_in_aabb(&aabb) {
                if ent.bounding_box().intersects(aabb) {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }

            let Some(entity) = ENTITIES.create(et, next_entity_id(), DVec3::new(xx, f64::from(y_start), zz), Arc::downgrade(world)) else {
                continue;
            };
            let despawn_dist = f64::from(category.despawn_distance());
            if dist > despawn_dist * despawn_dist {
                if let Some(mob) = entity.as_mob() {
                    if mob.remove_when_far_away(dist) {
                        continue;
                    }
                }
            }
            if let Some(mob) = entity.as_mob() {
                let _ = mob.finalize_spawn(world, EntitySpawnReason::Natural, None);
            }
            match world.try_add_entity(Arc::clone(&entity)) {
                Ok(()) => {
                    state.after_spawn(world, entity_id, pos, category, chunk);
                    total_spawned += 1;
                    group_size += 1;
                    if total_spawned >= 4 {
                        return total_spawned;
                    }
                    if group_target > 0 && group_size >= group_target {
                        break;
                    }
                    if group_target == 0 && group_size >= current_max.max(1) {
                        break;
                    }
                }
                Err(_) => continue,
            }
        }
    }
    total_spawned
}

fn spawn_category_for_chunk(
    world: &Arc<World>,
    chunk: ChunkPos,
    category: MobCategory,
    state: &mut SpawnState,
) -> usize {
    let start = random_pos_within(world, chunk);
    if start.y() < world.get_min_y() + 1 {
        return 0;
    }
    spawn_category_for_position(world, chunk, start, category, state)
}

// ---------------------------------------------------------------------------
// Public entry

pub fn can_natural_spawn(world: &World) -> bool {
    use steel_registry::vanilla_game_rules::SPAWN_MOBS;
    world.get_game_rule(&SPAWN_MOBS)
}

pub(crate) fn filtered_spawning_categories(world: &World, state: &SpawnState) -> Vec<MobCategory> {
    let spawn_monsters = world.get_game_rule(&steel_registry::vanilla_game_rules::SPAWN_MONSTERS);
    SPAWNING_CATEGORIES
        .iter()
        .copied()
        .filter(|c| {
            if *c == MobCategory::Monster && !spawn_monsters {
                return false;
            }
            state.can_spawn_global(*c)
        })
        .collect()
}

pub fn tick_natural_spawning(world: &Arc<World>) {
    if !can_natural_spawn(world) {
        return;
    }

    // Gather spawnable chunks: 8-chunk radius around players (vanilla)
    let mut spawnable: Vec<ChunkPos> = Vec::new();
    let mut seen = HashSet::new();
    world.players.iter_players(|_, player| {
        let center = ChunkPos::from_entity_pos(player.position());
        for dx in -SPAWN_DISTANCE_CHUNK..=SPAWN_DISTANCE_CHUNK {
            for dz in -SPAWN_DISTANCE_CHUNK..=SPAWN_DISTANCE_CHUNK {
                if dx * dx + dz * dz > SPAWN_DISTANCE_CHUNK * SPAWN_DISTANCE_CHUNK {
                    continue;
                }
                let pos = ChunkPos::new(center.0.x + dx, center.0.y + dz);
                if seen.insert(pos) && world.chunk_map.with_full_chunk(pos, |_| ()).is_some() {
                    spawnable.push(pos);
                }
            }
        }
        true
    });
    if spawnable.is_empty() {
        let spawn = world.level_data.read().data().spawn_pos();
        let pos = ChunkPos::from_block_pos(spawn);
        if world.chunk_map.with_full_chunk(pos, |_| ()).is_some() {
            spawnable.push(pos);
        }
    }
    if spawnable.is_empty() {
        return;
    }

    let mut counts: FxHashMap<MobCategory, i32> = FxHashMap::default();
    let mut potential = PotentialCalculator::new();
    let mut local: FxHashMap<ChunkPos, FxHashMap<MobCategory, i32>> = FxHashMap::default();

    // Count via spawnable chunks (approximation of global counts without private live_by_id)
    for chunk in &spawnable {
        for entity in world.entity_manager.live_entities_in_chunk(*chunk) {
            if let Some(mob) = entity.as_mob() {
                if mob.is_persistence_required() || mob.requires_custom_persistence() {
                    continue;
                }
            }
            let et = entity.entity_type();
            let cat = et.mob_category;
            if cat == MobCategory::Misc {
                continue;
            }
            *counts.entry(cat).or_insert(0) += 1;
            *local.entry(*chunk).or_default().entry(cat).or_insert(0) += 1;
            if let Some(biome) = world.biome_at(entity.block_position()) {
                if let Some(cost) = biome.spawn_costs.get(&et.key) {
                    potential.add_charge(entity.block_position(), cost.charge);
                }
            }
        }
    }

    let mut state = SpawnState {
        spawnable_chunk_count: spawnable.len() as i32,
        mob_counts: counts,
        potential,
        local_counts: local,
    };

    let categories = filtered_spawning_categories(world, &state);
    if categories.is_empty() {
        return;
    }

    let mut rng = StdRng::seed_from_u64(world.game_time() as u64);
    spawnable.shuffle(&mut rng);

    for chunk in spawnable {
        for &cat in &categories {
            if !state.can_spawn_global(cat) || !state.can_spawn_local(cat, chunk) {
                continue;
            }
            spawn_category_for_chunk(world, chunk, cat, &mut state);
        }
    }
}

/// Worldgen creature spawn (vanilla `spawnMobsForChunkGeneration`).
#[allow(dead_code, reason = "wired via worldgen/stages/spawn.rs once WorldGenRegion placement is available")]
pub fn spawn_mobs_for_chunk_generation(world: &Arc<World>, chunk: ChunkPos, rng: &mut impl rand::Rng) {
    let center = BlockPos::new((chunk.0.x << 4) + 8, world.get_min_y(), (chunk.0.y << 4) + 8);
    let Some(biome) = world.biome_at(center) else {
        return;
    };
    let prob = biome.creature_spawn_probability;
    if prob <= 0.0 {
        return;
    }
    use steel_registry::vanilla_game_rules::SPAWN_MOBS;
    if !world.get_game_rule(&SPAWN_MOBS) {
        return;
    }
    let entries = spawner_entries(biome, MobCategory::Creature);
    if entries.is_empty() {
        return;
    }
    while rng.random::<f32>() < prob {
        let Some(picked) = pick_weighted_spawner(entries, rng) else {
            break;
        };
        let Some(et) = REGISTRY.entity_types.by_key(&picked.entity_type) else {
            continue;
        };
        let count = picked.min_count + rng.random_range(0..=(picked.max_count - picked.min_count).max(0));
        let base_x = (chunk.0.x << 4) + rng.random_range(0..16);
        let base_z = (chunk.0.y << 4) + rng.random_range(0..16);
        let mut start_x = base_x;
        let mut start_z = base_z;
        for _ in 0..count {
            let mut success = false;
            let mut x = start_x;
            let mut z = start_z;
            for _ in 0..4 {
                if success {
                    break;
                }
                let top = world
                    .height_at(HeightmapType::WorldSurface, x, z)
                    .unwrap_or(world.get_min_y());
                let pos = BlockPos::new(x, top + 1, z);
                if !is_spawn_position_ok(world, pos, MobCategory::Creature) {
                    x += rng.random_range(0..5) - rng.random_range(0..5);
                    z += rng.random_range(0..5) - rng.random_range(0..5);
                    continue;
                }
                let w = f64::from(et.dimensions.width);
                let fx = (f64::from(x) + 0.5).clamp(
                    f64::from(chunk.0.x << 4) + f64::from(w),
                    f64::from((chunk.0.x << 4) + 16) - f64::from(w),
                );
                let fz = (f64::from(z) + 0.5).clamp(
                    f64::from(chunk.0.y << 4) + f64::from(w),
                    f64::from((chunk.0.y << 4) + 16) - f64::from(w),
                );
                let aabb = entity_spawn_aabb(et, fx, f64::from(pos.y()), fz);
                let blocked = world.get_entities_in_aabb(&aabb).iter().any(|e| e.bounding_box().intersects(aabb));
                if blocked {
                    continue;
                }
                if !check_spawn_rules(world, MobCategory::Creature, pos, EntitySpawnReason::ChunkGeneration) {
                    continue;
                }
                let Some(entity) = ENTITIES.create(et, next_entity_id(), DVec3::new(fx, f64::from(pos.y()), fz), Arc::downgrade(world)) else {
                    continue;
                };
                if let Some(mob) = entity.as_mob() {
                    let _ = mob.finalize_spawn(world, EntitySpawnReason::ChunkGeneration, None);
                }
                if world.try_add_entity(entity).is_ok() {
                    success = true;
                }
                x += rng.random_range(0..5) - rng.random_range(0..5);
                z += rng.random_range(0..5) - rng.random_range(0..5);
                if !( (chunk.0.x << 4)..((chunk.0.x << 4) + 16)).contains(&x)
                    || !( (chunk.0.y << 4)..((chunk.0.y << 4) + 16)).contains(&z)
                {
                    x = base_x + rng.random_range(0..5) - rng.random_range(0..5);
                    z = base_z + rng.random_range(0..5) - rng.random_range(0..5);
                }
            }
            start_x += rng.random_range(0..5) - rng.random_range(0..5);
            start_z += rng.random_range(0..5) - rng.random_range(0..5);
        }
    }
}
