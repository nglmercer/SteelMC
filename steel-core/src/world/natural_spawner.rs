//! Vanilla `NaturalSpawner`.
//!
//! Spawn eligibility is decided per `EntityType`, not per `MobCategory`: the placement type
//! and spawn predicate both come from [`crate::entity::spawn_placements`], keyed by the same
//! table vanilla builds in `SpawnPlacements`. This module owns the parts around them — the
//! weighted biome selection, the nether-fortress mob-list override, the spawn-cost energy
//! budget, the global and per-player caps, and the two entry points
//! (`tick_natural_spawning` and `spawn_mobs_for_chunk_generation`).

use std::{
    iter,
    sync::{Arc, LazyLock},
};

use glam::DVec3;
use rand::{RngExt, SeedableRng, rngs::StdRng, seq::SliceRandom};
use rustc_hash::{FxHashMap, FxHashSet};
use steel_registry::{
    REGISTRY, RegistryExt,
    biome::{BiomeRef, SpawnerData},
    blocks::block_state_ext::BlockStateExt,
    entity_type::{EntityType, EntityTypeRef, MobCategory},
    vanilla_blocks, vanilla_game_rules,
};
use steel_utils::{BlockPos, ChunkPos, Identifier, types::Difficulty};
use uuid::Uuid;

use crate::{
    chunk::{heightmap::HeightmapType, status::ChunkStatus},
    entity::{
        ENTITIES, Entity, EntitySpawnReason, Mob, SpawnGroupData, next_entity_id, spawn_placements,
    },
    physics::collision::no_collision,
    world::{SignalGetter as _, World, level_reader::LevelReader},
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
    const fn new() -> Self {
        Self {
            charges: Vec::new(),
        }
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
    local_caps: LocalMobCapCalculator,
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
    fn can_spawn_cost(&self, world: &World, entity_type: &Identifier, pos: BlockPos) -> bool {
        if let Some(biome) = world.biome_at(pos)
            && let Some(cost) = biome.spawn_costs.get(entity_type)
        {
            let change = self.potential.potential_change(pos, cost.charge);
            if change > cost.energy_budget {
                return false;
            }
        }
        true
    }
    fn after_spawn(
        &mut self,
        world: &World,
        entity_type: &Identifier,
        pos: BlockPos,
        category: MobCategory,
        chunk: ChunkPos,
    ) {
        *self.mob_counts.entry(category).or_insert(0) += 1;
        self.local_caps.add_mob(chunk, category);
        if let Some(biome) = world.biome_at(pos)
            && let Some(cost) = biome.spawn_costs.get(entity_type)
        {
            self.potential.add_charge(pos, cost.charge);
        }
    }
}

/// Returns vanilla `NaturalSpawner.getTopNonCollidingPos`.
///
/// Uses the entity type's registered heightmap, walks below the nether ceiling in a
/// dimension that has one, then applies the placement type's position adjustment.
fn top_non_colliding_pos(
    world: &Arc<World>,
    entity_type: &'static EntityType,
    x: i32,
    z: i32,
) -> BlockPos {
    let heightmap = spawn_placements::heightmap_type_for(entity_type);
    let height = world
        .height_at(heightmap, x, z)
        .unwrap_or_else(|| world.get_min_y());
    let mut pos = BlockPos::new(x, height, z);

    if world.dimension_type.has_ceiling {
        // Descend out of the bedrock roof, then down through the open air below it.
        loop {
            pos = pos.below();
            if world.get_block_state(pos).is_air() {
                break;
            }
        }
        while world.get_block_state(pos).is_air() && pos.y() > world.get_min_y() {
            pos = pos.below();
        }
    }

    spawn_placements::placement_type_for(entity_type).adjust_spawn_position(world, pos)
}

// ---------------------------------------------------------------------------
// Biome weighted selection

fn spawner_entries<'a>(biome: BiomeRef, category: MobCategory) -> &'a [SpawnerData] {
    biome
        .spawners
        .get(category.biome_spawner_key())
        .map_or(&[][..], Vec::as_slice)
}

fn pick_weighted_spawner<'a>(
    entries: &'a [SpawnerData],
    rng: &mut impl rand::Rng,
) -> Option<&'a SpawnerData> {
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

/// Vanilla `NetherFortressStructure.FORTRESS_ENEMIES`.
///
/// A fortress replaces the biome's monster list wholesale, which is why blazes and wither
/// skeletons spawn there and nowhere else in the nether.
static FORTRESS_ENEMIES: LazyLock<Vec<SpawnerData>> = LazyLock::new(|| {
    vec![
        SpawnerData {
            entity_type: Identifier::vanilla_static("blaze"),
            weight: 10,
            min_count: 2,
            max_count: 3,
        },
        SpawnerData {
            entity_type: Identifier::vanilla_static("zombified_piglin"),
            weight: 5,
            min_count: 4,
            max_count: 4,
        },
        SpawnerData {
            entity_type: Identifier::vanilla_static("wither_skeleton"),
            weight: 8,
            min_count: 5,
            max_count: 5,
        },
        SpawnerData {
            entity_type: Identifier::vanilla_static("skeleton"),
            weight: 2,
            min_count: 5,
            max_count: 5,
        },
        SpawnerData {
            entity_type: Identifier::vanilla_static("magma_cube"),
            weight: 3,
            min_count: 4,
            max_count: 4,
        },
    ]
});

/// Vanilla `NaturalSpawner.isInNetherFortressBounds`.
fn is_in_nether_fortress_bounds(world: &Arc<World>, pos: BlockPos, category: MobCategory) -> bool {
    if category != MobCategory::Monster {
        return false;
    }
    if world.get_block_state(pos.below()).get_block() != &vanilla_blocks::NETHER_BRICKS {
        return false;
    }

    let fortress = Identifier::vanilla_static("fortress");
    let chunk_pos = ChunkPos::from_block_pos(pos);
    let Some(holder) = world
        .chunk_map
        .chunks
        .read_sync(&chunk_pos, |_, holder| Arc::clone(holder))
    else {
        return false;
    };
    let Some(chunk) = holder.try_chunk(ChunkStatus::StructureStarts) else {
        return false;
    };

    // Vanilla resolves the start through this chunk's structure references, then tests the
    // start's bounding box; a fortress spans more chunks than the one holding its start.
    let origins: Vec<ChunkPos> = chunk
        .structure_references()
        .get(&fortress)
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();

    for origin in origins.into_iter().chain(iter::once(chunk_pos)) {
        let Some(origin_holder) = world
            .chunk_map
            .chunks
            .read_sync(&origin, |_, holder| Arc::clone(holder))
        else {
            continue;
        };
        let Some(origin_chunk) = origin_holder.try_chunk(ChunkStatus::StructureStarts) else {
            continue;
        };
        let starts = origin_chunk.structure_starts();
        if let Some(start) = starts.get(&fortress)
            && start
                .bounding_box
                .is_some_and(|bounds| bounds.contains_blockpos(pos))
        {
            return true;
        }
    }

    false
}

/// Vanilla `NaturalSpawner.mobsAt`.
fn mobs_at(world: &Arc<World>, category: MobCategory, pos: BlockPos) -> Vec<SpawnerData> {
    if is_in_nether_fortress_bounds(world, pos, category) {
        return (*FORTRESS_ENEMIES).clone();
    }
    world
        .biome_at(pos)
        .map(|biome| spawner_entries(biome, category).to_vec())
        .unwrap_or_default()
}

/// Vanilla `NaturalSpawner.getRandomSpawnMobAt`.
fn random_spawn_mob_at(
    world: &Arc<World>,
    category: MobCategory,
    pos: BlockPos,
    rng: &mut impl rand::Rng,
) -> Option<SpawnerData> {
    // DIVERGENCE: vanilla skips 98% of water-ambient attempts in biomes tagged
    // `reduced_water_ambient_spawns`. That tag is not present in the generated biome tags,
    // so the roll is omitted and those biomes get full-rate water ambient spawns.
    let entries = mobs_at(world, category, pos);
    pick_weighted_spawner(&entries, rng).cloned()
}

/// Vanilla `NaturalSpawner.canSpawnMobAt`.
///
/// Re-validates the chosen spawner entry at the *moved* position: the attempt loop walks up
/// to five blocks per step, which can cross a biome or fortress boundary.
fn can_spawn_mob_at(
    world: &Arc<World>,
    category: MobCategory,
    spawn_data: &SpawnerData,
    pos: BlockPos,
) -> bool {
    mobs_at(world, category, pos)
        .iter()
        .any(|entry| entry.entity_type == spawn_data.entity_type)
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
    let dy = f64::from(pos.y()) - sy;
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

// ---------------------------------------------------------------------------
// Per-category spawn attempt

/// Vanilla `NaturalSpawner.spawnCategoryForPosition`.
///
/// The nested loop shape is load-bearing: vanilla reassigns the inner loop bound `max` from
/// the chosen `SpawnerData` group count the first time a spawner is picked, so the attempt
/// budget becomes the group budget. `groupData` also threads through the whole cluster,
/// which is what drives `AgeableMobGroupData`'s baby-spawn rolls.
fn spawn_category_for_position(
    world: &Arc<World>,
    chunk: ChunkPos,
    start: BlockPos,
    category: MobCategory,
    state: &mut SpawnState,
) -> usize {
    if world.is_redstone_conductor(world.get_block_state(start), start) {
        return 0;
    }

    let mut rng = rand::rng();
    let y_start = start.y();
    let mut cluster_size = 0_usize;

    for _ in 0..3 {
        let mut x = start.x();
        let mut z = start.z();
        let mut current_spawn_data: Option<SpawnerData> = None;
        let mut group_data: Option<SpawnGroupData> = None;
        // Vanilla `int max = Mth.ceil(level.random.nextFloat() * 4.0F)`, later reassigned to
        // the picked spawner's group count.
        let mut max = (rng.random::<f32>() * 4.0).ceil() as i32;
        let mut group_size = 0;

        let mut attempt = 0;
        while attempt < max {
            attempt += 1;
            x += rng.random_range(0..6) - rng.random_range(0..6);
            z += rng.random_range(0..6) - rng.random_range(0..6);
            let pos = BlockPos::new(x, y_start, z);
            let xx = f64::from(x) + 0.5;
            let zz = f64::from(z) + 0.5;

            let Some(nearest_player_distance_sqr) =
                nearest_player_distance_sq(world, xx, f64::from(y_start), zz)
            else {
                continue;
            };
            if !is_right_distance(world, chunk, pos, nearest_player_distance_sqr) {
                continue;
            }

            if current_spawn_data.is_none() {
                let Some(picked) = random_spawn_mob_at(world, category, pos, &mut rng) else {
                    break;
                };
                max = picked.min_count
                    + rng.random_range(0..(1 + picked.max_count - picked.min_count).max(1));
                current_spawn_data = Some(picked);
            }

            let Some(ref spawn_data) = current_spawn_data else {
                break;
            };
            let Some(entity_type) = REGISTRY.entity_types.by_key(&spawn_data.entity_type) else {
                break;
            };

            if !is_valid_spawn_position_for_type(
                world,
                category,
                spawn_data,
                entity_type,
                pos,
                nearest_player_distance_sqr,
                &mut rng,
            ) {
                continue;
            }
            if !state.can_spawn_cost(world, &spawn_data.entity_type, pos) {
                continue;
            }

            let Some(entity) = ENTITIES.create(
                entity_type,
                next_entity_id(),
                DVec3::new(xx, f64::from(y_start), zz),
                Arc::downgrade(world),
            ) else {
                // Vanilla `getMobForSpawn` returning null aborts the whole position.
                return cluster_size;
            };
            // Vanilla `mob.snapTo(xx, yStart, zz, random.nextFloat() * 360.0F, 0.0F)`.
            entity.set_rotation((rng.random::<f32>() * 360.0, 0.0));

            let Some(mob) = entity.as_mob() else {
                continue;
            };
            if !is_valid_position_for_mob(world, mob, nearest_player_distance_sqr) {
                continue;
            }

            group_data = mob.finalize_spawn(world, EntitySpawnReason::Natural, group_data);
            if world.try_add_entity(Arc::clone(&entity)).is_err() {
                continue;
            }

            cluster_size += 1;
            group_size += 1;
            state.after_spawn(world, &spawn_data.entity_type, pos, category, chunk);

            if i32::try_from(cluster_size).unwrap_or(i32::MAX) >= mob.max_spawn_cluster_size() {
                return cluster_size;
            }
            if mob.is_max_group_size_reached(group_size) {
                break;
            }
        }
    }

    cluster_size
}

/// Vanilla `NaturalSpawner.isValidSpawnPostitionForType`.
fn is_valid_spawn_position_for_type(
    world: &Arc<World>,
    category: MobCategory,
    spawn_data: &SpawnerData,
    entity_type: EntityTypeRef,
    pos: BlockPos,
    nearest_player_distance_sqr: f64,
    rng: &mut impl rand::Rng,
) -> bool {
    if entity_type.mob_category == MobCategory::Misc {
        return false;
    }
    if !entity_type.can_spawn_far_from_player {
        let despawn_distance = f64::from(entity_type.mob_category.despawn_distance());
        if nearest_player_distance_sqr > despawn_distance * despawn_distance {
            return false;
        }
    }
    if !entity_type.summonable || !can_spawn_mob_at(world, category, spawn_data, pos) {
        return false;
    }
    if !spawn_placements::is_spawn_position_ok(world, pos, entity_type) {
        return false;
    }
    if !spawn_placements::check_spawn_rules(
        entity_type,
        world,
        EntitySpawnReason::Natural,
        pos,
        rng,
    ) {
        return false;
    }
    no_collision(
        world,
        entity_type.spawn_aabb(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
        ),
    )
}

/// Vanilla `NaturalSpawner.isValidPositionForMob`.
fn is_valid_position_for_mob(
    world: &Arc<World>,
    mob: &dyn Mob,
    nearest_player_distance_sqr: f64,
) -> bool {
    let despawn_distance = f64::from(mob.entity_type().mob_category.despawn_distance());
    if nearest_player_distance_sqr > despawn_distance * despawn_distance
        && mob.remove_when_far_away(nearest_player_distance_sqr)
    {
        return false;
    }
    mob.check_spawn_rules(world, EntitySpawnReason::Natural) && mob.check_spawn_obstruction(world)
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
    world.get_game_rule(&vanilla_game_rules::SPAWN_MOBS)
}

/// Vanilla `NaturalSpawner.getFilteredSpawningCategories`.
///
/// `spawn_enemies` mirrors `ServerChunkCache.spawnEnemies` (false on Peaceful) and
/// `spawn_persistent` mirrors the `gameTime % 400 == 0` gate, which is what makes passive
/// mobs top up only every 20 seconds while monsters are considered every tick.
pub(crate) fn filtered_spawning_categories(
    state: &SpawnState,
    spawn_enemies: bool,
    spawn_persistent: bool,
) -> Vec<MobCategory> {
    SPAWNING_CATEGORIES
        .iter()
        .copied()
        .filter(|category| {
            (spawn_enemies || category.is_friendly())
                && (spawn_persistent || !category.is_persistent())
                && state.can_spawn_global(*category)
        })
        .collect()
}

/// Vanilla `LocalMobCapCalculator`.
///
/// The local cap is per *player*, not per chunk: a mob counts against every player close
/// enough to the chunk it is in, and a chunk may be spawned into as long as at least one
/// nearby player is under the category cap.
struct LocalMobCapCalculator {
    players_near_chunk: FxHashMap<ChunkPos, Vec<Uuid>>,
    player_mob_counts: FxHashMap<Uuid, FxHashMap<MobCategory, i32>>,
    player_positions: Vec<(Uuid, DVec3)>,
}

impl LocalMobCapCalculator {
    /// Vanilla `ChunkMap.playerIsCloseEnoughForSpawning` uses a squared euclidean distance
    /// from the chunk to the player of less than 16384 (128 blocks).
    const SPAWN_DISTANCE_SQR: f64 = 16_384.0;

    fn new(world: &Arc<World>) -> Self {
        let mut player_positions = Vec::new();
        world.players.iter_players(|uuid, player| {
            if !player.is_spectator() {
                player_positions.push((*uuid, player.position()));
            }
            true
        });
        Self {
            players_near_chunk: FxHashMap::default(),
            player_mob_counts: FxHashMap::default(),
            player_positions,
        }
    }

    fn players_near(&mut self, chunk: ChunkPos) -> &[Uuid] {
        let positions = &self.player_positions;
        self.players_near_chunk.entry(chunk).or_insert_with(|| {
            positions
                .iter()
                .filter(|(_, position)| {
                    euclidean_distance_sqr(chunk, *position) < Self::SPAWN_DISTANCE_SQR
                })
                .map(|(uuid, _)| *uuid)
                .collect()
        })
    }

    fn add_mob(&mut self, chunk: ChunkPos, category: MobCategory) {
        let nearby: Vec<Uuid> = self.players_near(chunk).to_vec();
        for uuid in nearby {
            *self
                .player_mob_counts
                .entry(uuid)
                .or_default()
                .entry(category)
                .or_insert(0) += 1;
        }
    }

    fn can_spawn(&mut self, category: MobCategory, chunk: ChunkPos) -> bool {
        let nearby: Vec<Uuid> = self.players_near(chunk).to_vec();
        nearby.iter().any(|uuid| {
            self.player_mob_counts.get(uuid).is_none_or(|counts| {
                counts.get(&category).copied().unwrap_or(0) < category.max_instances_per_chunk()
            })
        })
    }
}

/// Vanilla `ChunkMap.euclideanDistanceSquared(ChunkPos, Vec3)`.
fn euclidean_distance_sqr(chunk: ChunkPos, position: DVec3) -> f64 {
    let dx = f64::from(chunk.0.x * 16 + 8) - position.x;
    let dz = f64::from(chunk.0.y * 16 + 8) - position.z;
    dx.mul_add(dx, dz * dz)
}

pub fn tick_natural_spawning(world: &Arc<World>) {
    if !can_natural_spawn(world) {
        return;
    }

    // Vanilla's spawnable chunk count comes from the distance manager's natural-spawn
    // ticket level; Steel derives the same set from the 8-chunk radius around each player.
    let mut spawnable: Vec<ChunkPos> = Vec::new();
    let mut seen = FxHashSet::default();
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
        return;
    }

    let mut state = create_state(world, spawnable.len() as i32);

    // Vanilla `ServerChunkCache.tickChunks`: enemies are gated on difficulty, persistent
    // categories only every 400 ticks.
    let spawn_enemies = world.difficulty() != Difficulty::Peaceful;
    let spawn_persistent = world.game_time() % 400 == 0;
    let categories = filtered_spawning_categories(&state, spawn_enemies, spawn_persistent);
    if categories.is_empty() {
        return;
    }

    let mut rng = StdRng::seed_from_u64(world.game_time() as u64);
    spawnable.shuffle(&mut rng);

    for chunk in spawnable {
        for &category in &categories {
            if !state.can_spawn_global(category) || !state.local_caps.can_spawn(category, chunk) {
                continue;
            }
            spawn_category_for_chunk(world, chunk, category, &mut state);
        }
    }
}

/// Vanilla `NaturalSpawner.createState`.
fn create_state(world: &Arc<World>, spawnable_chunk_count: i32) -> SpawnState {
    let mut mob_counts: FxHashMap<MobCategory, i32> = FxHashMap::default();
    let mut potential = PotentialCalculator::new();
    let mut local_caps = LocalMobCapCalculator::new(world);

    for entity in world.entity_manager.all_live_entities() {
        if let Some(mob) = entity.as_mob()
            && (mob.is_persistence_required() || mob.requires_custom_persistence())
        {
            continue;
        }
        let entity_type = entity.entity_type();
        let category = entity_type.mob_category;
        if category == MobCategory::Misc {
            continue;
        }

        let pos = entity.block_position();
        if let Some(biome) = world.biome_at(pos)
            && let Some(cost) = biome.spawn_costs.get(&entity_type.key)
        {
            potential.add_charge(pos, cost.charge);
        }
        if entity.as_mob().is_some() {
            local_caps.add_mob(ChunkPos::from_block_pos(pos), category);
        }
        *mob_counts.entry(category).or_insert(0) += 1;
    }

    SpawnState {
        spawnable_chunk_count,
        mob_counts,
        potential,
        local_caps,
    }
}

/// Worldgen creature spawn (vanilla `spawnMobsForChunkGeneration`).
#[expect(
    dead_code,
    reason = "wired via worldgen/stages/spawn.rs once WorldGenRegion placement is available"
)]
pub fn spawn_mobs_for_chunk_generation(
    world: &Arc<World>,
    chunk: ChunkPos,
    rng: &mut impl rand::Rng,
) {
    let center = BlockPos::new(
        (chunk.0.x << 4) + 8,
        world.get_min_y(),
        (chunk.0.y << 4) + 8,
    );
    let Some(biome) = world.biome_at(center) else {
        return;
    };
    let prob = biome.creature_spawn_probability;
    if prob <= 0.0 {
        return;
    }
    if !world.get_game_rule(&vanilla_game_rules::SPAWN_MOBS) {
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
        let count = picked.min_count
            + rng.random_range(0..(1 + picked.max_count - picked.min_count).max(1));
        // Vanilla threads `groupSpawnData` across the whole count, not per-attempt.
        let mut group_data: Option<SpawnGroupData> = None;
        let x_origin = chunk.0.x << 4;
        let z_origin = chunk.0.y << 4;
        let mut x = x_origin + rng.random_range(0..16);
        let mut z = z_origin + rng.random_range(0..16);
        let start_x = x;
        let start_z = z;

        for _ in 0..count {
            let mut success = false;
            let mut attempts = 0;
            while !success && attempts < 4 {
                attempts += 1;
                let pos = top_non_colliding_pos(world, et, x, z);
                if et.summonable && spawn_placements::is_spawn_position_ok(world, pos, et) {
                    let width = f64::from(et.dimensions.width);
                    let fx = f64::from(x).clamp(
                        f64::from(x_origin) + width,
                        f64::from(x_origin) + 16.0 - width,
                    );
                    let fz = f64::from(z).clamp(
                        f64::from(z_origin) + width,
                        f64::from(z_origin) + 16.0 - width,
                    );
                    let spawn_pos = BlockPos::new(fx.floor() as i32, pos.y(), fz.floor() as i32);
                    if no_collision(world, et.spawn_aabb(fx, f64::from(pos.y()), fz))
                        && spawn_placements::check_spawn_rules(
                            et,
                            world,
                            EntitySpawnReason::ChunkGeneration,
                            spawn_pos,
                            rng,
                        )
                        && let Some(entity) = ENTITIES.create(
                            et,
                            next_entity_id(),
                            DVec3::new(fx, f64::from(pos.y()), fz),
                            Arc::downgrade(world),
                        )
                    {
                        entity.set_rotation((rng.random::<f32>() * 360.0, 0.0));
                        if let Some(mob) = entity.as_mob()
                            && mob.check_spawn_rules(world, EntitySpawnReason::ChunkGeneration)
                            && mob.check_spawn_obstruction(world)
                        {
                            group_data = mob.finalize_spawn(
                                world,
                                EntitySpawnReason::ChunkGeneration,
                                group_data,
                            );
                            if world.try_add_entity(Arc::clone(&entity)).is_ok() {
                                success = true;
                            }
                        }
                    }
                }

                // Vanilla re-rolls in a `while` until both axes land back inside the chunk,
                // not a single `if`.
                x += rng.random_range(0..5) - rng.random_range(0..5);
                z += rng.random_range(0..5) - rng.random_range(0..5);
                while x < x_origin || x >= x_origin + 16 || z < z_origin || z >= z_origin + 16 {
                    x = start_x + rng.random_range(0..5) - rng.random_range(0..5);
                    z = start_z + rng.random_range(0..5) - rng.random_range(0..5);
                }
            }
        }
    }
}
