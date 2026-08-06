//! Vanilla `BaseSpawner` — the spawn loop shared by the spawner block entity.

use std::io::Cursor;
use std::sync::Arc;

use glam::DVec3;
use rand::RngExt as _;
use simdnbt::borrow::{NbtCompound as NbtCompoundView, read_compound as read_borrowed_compound};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use std::str::FromStr as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::{
    REGISTRY, RegistryExt, vanilla_blocks, vanilla_game_events, vanilla_game_rules,
};
use steel_utils::locks::SyncMutex;

use steel_utils::{BlockPos, Identifier, WorldAabb, types::Difficulty};
use uuid::Uuid;

use super::spawn_data::{SpawnData, SpawnPotentials};
use crate::entity::{
    ENTITIES, Entity as _, EntityBaseSaveData, EntityFireFreezeState, EntityLoadRequest,
    EntitySpawnReason, SharedEntity, spawn_placements::check_spawn_rules,
};
use crate::physics::collision::no_collision;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla `BaseSpawner.EVENT_SPAWN`.
pub(super) const EVENT_SPAWN: i32 = 1;

/// Vanilla level event `2004` (mob-spawner spawn particles).
const LEVEL_EVENT_SPAWNER_SPAWN: i32 = 2004;

const DEFAULT_SPAWN_DELAY: i16 = 20;
const DEFAULT_MIN_SPAWN_DELAY: i32 = 200;
const DEFAULT_MAX_SPAWN_DELAY: i32 = 800;
const DEFAULT_SPAWN_COUNT: i32 = 4;
const DEFAULT_MAX_NEARBY_ENTITIES: i32 = 6;
const DEFAULT_REQUIRED_PLAYER_RANGE: i32 = 16;
const DEFAULT_SPAWN_RANGE: i32 = 4;

/// Vanilla `BaseSpawner`.
///
/// Vanilla's client-only fields (`spin`, `oSpin`, `displayEntity`) and `clientTick` are
/// omitted: they only drive the spinning miniature mob rendered inside the cage.
pub(super) struct BaseSpawner {
    state: SyncMutex<SpawnerState>,
}

struct SpawnerState {
    spawn_delay: i32,
    spawn_potentials: SpawnPotentials,
    next_spawn_data: Option<SpawnData>,
    min_spawn_delay: i32,
    max_spawn_delay: i32,
    spawn_count: i32,
    max_nearby_entities: i32,
    required_player_range: i32,
    spawn_range: i32,
}

impl Default for SpawnerState {
    fn default() -> Self {
        Self {
            spawn_delay: i32::from(DEFAULT_SPAWN_DELAY),
            spawn_potentials: SpawnPotentials::default(),
            next_spawn_data: None,
            min_spawn_delay: DEFAULT_MIN_SPAWN_DELAY,
            max_spawn_delay: DEFAULT_MAX_SPAWN_DELAY,
            spawn_count: DEFAULT_SPAWN_COUNT,
            max_nearby_entities: DEFAULT_MAX_NEARBY_ENTITIES,
            required_player_range: DEFAULT_REQUIRED_PLAYER_RANGE,
            spawn_range: DEFAULT_SPAWN_RANGE,
        }
    }
}

impl SpawnerState {
    /// Returns vanilla `BaseSpawner.getOrCreateNextSpawnData`.
    fn get_or_create_next_spawn_data(&mut self, rng: &mut impl rand::Rng) -> SpawnData {
        if let Some(next) = &self.next_spawn_data {
            return next.clone();
        }

        let next = self
            .spawn_potentials
            .random(rng)
            .cloned()
            .unwrap_or_default();
        self.next_spawn_data = Some(next.clone());
        next
    }
}

impl BaseSpawner {
    pub(super) fn new() -> Self {
        Self {
            state: SyncMutex::new(SpawnerState::default()),
        }
    }

    /// Returns vanilla `BaseSpawner.setEntityId`.
    ///
    /// Returns whether the next spawn data changed and the block entity must be
    /// re-synchronized.
    pub(super) fn set_entity_id(&self, entity_type: EntityTypeRef, rng: &mut impl rand::Rng) {
        let mut state = self.state.lock();
        let mut next = state.get_or_create_next_spawn_data(rng);
        next.entity_to_spawn_mut()
            .insert("id", entity_type.key.to_string());
        state.next_spawn_data = Some(next);
    }

    /// Returns vanilla `BaseSpawner.isNearPlayer` via `EntityGetter.hasNearbyAlivePlayer`.
    fn is_near_player(&self, world: &Arc<World>, pos: BlockPos) -> bool {
        let range = f64::from(self.state.lock().required_player_range);
        let center = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );

        let mut found = false;
        world.players.iter_players(|_, player| {
            if player.is_spectator() || !player.is_alive() {
                return true;
            }
            let delta = player.position() - center;
            if range < 0.0 || delta.length_squared() < range * range {
                found = true;
                return false;
            }
            true
        });
        found
    }

    /// Returns vanilla `BaseSpawner.onEventTriggered`.
    ///
    /// The client-side branch that resets the visual spin has no server-side effect.
    pub(super) const fn on_event_triggered(id: i32) -> bool {
        id == EVENT_SPAWN
    }

    /// Runs vanilla `BaseSpawner.serverTick`.
    #[expect(
        clippy::too_many_lines,
        reason = "mirrors the shape of vanilla BaseSpawner.serverTick"
    )]
    pub(super) fn server_tick(&self, world: &Arc<World>, pos: BlockPos) {
        if !self.is_near_player(world, pos)
            || !world.get_game_rule(&vanilla_game_rules::SPAWNER_BLOCKS_WORK)
        {
            return;
        }

        {
            let mut state = self.state.lock();
            if state.spawn_delay == -1 {
                drop(state);
                self.delay(world, pos);
                state = self.state.lock();
            }

            if state.spawn_delay > 0 {
                state.spawn_delay -= 1;
                return;
            }
        }

        let mut rng = rand::rng();
        let (next_spawn_data, spawn_count, spawn_range, max_nearby_entities) = {
            let mut state = self.state.lock();
            let next = state.get_or_create_next_spawn_data(&mut rng);
            (
                next,
                state.spawn_count,
                state.spawn_range,
                state.max_nearby_entities,
            )
        };

        let mut spawned_any = false;
        for _ in 0..spawn_count {
            let Some(entity_type) = spawn_data_entity_type(&next_spawn_data) else {
                self.delay(world, pos);
                return;
            };

            let spawn_pos = read_saved_pos(&next_spawn_data).unwrap_or_else(|| {
                DVec3::new(
                    f64::from(pos.x())
                        + (rng.random::<f64>() - rng.random::<f64>()) * f64::from(spawn_range)
                        + 0.5,
                    f64::from(pos.y()) + f64::from(rng.random_range(0..3)) - 1.0,
                    f64::from(pos.z())
                        + (rng.random::<f64>() - rng.random::<f64>()) * f64::from(spawn_range)
                        + 0.5,
                )
            });

            if !no_collision(
                world,
                entity_type.spawn_aabb(spawn_pos.x, spawn_pos.y, spawn_pos.z),
            ) {
                continue;
            }

            let spawn_block_pos = BlockPos::containing(spawn_pos.x, spawn_pos.y, spawn_pos.z);
            if let Some(custom_rules) = next_spawn_data.custom_spawn_rules() {
                if !entity_type.mob_category.is_friendly()
                    && world.difficulty() == Difficulty::Peaceful
                {
                    continue;
                }
                if !custom_rules.is_valid_position(spawn_block_pos, world) {
                    continue;
                }
            } else if !check_spawn_rules(
                entity_type,
                world,
                EntitySpawnReason::Spawner,
                spawn_block_pos,
                &mut rng,
            ) {
                continue;
            }

            let Some(entity) = load_entity(
                world,
                entity_type,
                next_spawn_data.entity_to_spawn(),
                spawn_pos,
            ) else {
                self.delay(world, pos);
                return;
            };

            // Vanilla counts entities of the spawned entity's own type inside the
            // spawner block inflated by `spawnRange`.
            let count_box = WorldAabb::new(
                f64::from(pos.x()),
                f64::from(pos.y()),
                f64::from(pos.z()),
                f64::from(pos.x()) + 1.0,
                f64::from(pos.y()) + 1.0,
                f64::from(pos.z()) + 1.0,
            )
            .inflate(f64::from(spawn_range));
            let nearby = world
                .get_entities_in_aabb_matching(&count_box, |other| {
                    other.entity_type() == entity_type
                        && !other.as_player().is_some_and(Player::is_spectator)
                })
                .len();
            if i32::try_from(nearby).unwrap_or(i32::MAX) >= max_nearby_entities {
                self.delay(world, pos);
                return;
            }

            entity.set_rotation((rng.random::<f32>() * 360.0, 0.0));
            entity.set_old_position_to_current();

            if let Some(mob) = entity.as_mob() {
                if next_spawn_data.custom_spawn_rules().is_none()
                    && !mob.check_spawn_rules(world, EntitySpawnReason::Spawner)
                    || !mob.check_spawn_obstruction(world)
                {
                    continue;
                }

                // Vanilla only finalizes when the stored entity tag carries nothing but
                // its `id`, so hand-configured spawners keep their authored data.
                let has_no_configuration = next_spawn_data.entity_to_spawn().len() == 1
                    && next_spawn_data.entity_to_spawn().get("id").is_some();
                if has_no_configuration {
                    let _ = mob.finalize_spawn(world, EntitySpawnReason::Spawner, None);
                }
            }

            if world.try_add_entity(Arc::clone(&entity)).is_err() {
                self.delay(world, pos);
                return;
            }

            world.level_event(LEVEL_EVENT_SPAWNER_SPAWN, pos, 0, None);
            world.game_event(
                &vanilla_game_events::ENTITY_PLACE,
                spawn_block_pos,
                &GameEventContext::new(None, None),
            );
            if let Some(mob) = entity.as_mob() {
                mob.spawn_anim();
            }

            spawned_any = true;
        }

        if spawned_any {
            self.delay(world, pos);
        }
    }

    /// Runs vanilla `BaseSpawner.delay`.
    fn delay(&self, world: &Arc<World>, pos: BlockPos) {
        let mut rng = rand::rng();
        let next_data = {
            let mut state = self.state.lock();
            state.spawn_delay = if state.max_spawn_delay <= state.min_spawn_delay {
                state.min_spawn_delay
            } else {
                state.min_spawn_delay
                    + rng.random_range(0..(state.max_spawn_delay - state.min_spawn_delay))
            };
            let next = state.spawn_potentials.random(&mut rng).cloned();
            if let Some(next) = next.clone() {
                state.next_spawn_data = Some(next);
            }
            next
        };

        if next_data.is_some() {
            // Vanilla `SpawnerBlockEntity.setNextSpawnData` re-sends the block entity.
            world.send_block_updated(pos);
        }
        world.block_event(pos, &vanilla_blocks::SPAWNER, EVENT_SPAWN, 0);
    }

    /// Runs vanilla `BaseSpawner.load`.
    pub(super) fn load(&self, nbt: &NbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        state.spawn_delay = i32::from(nbt.short("Delay").unwrap_or(DEFAULT_SPAWN_DELAY));
        state.next_spawn_data = nbt
            .compound("SpawnData")
            .and_then(|data| SpawnData::read(&NbtTag::Compound(data.to_owned())));
        state.spawn_potentials =
            SpawnPotentials::read(nbt, "SpawnPotentials").unwrap_or_else(|| {
                SpawnPotentials::of(state.next_spawn_data.clone().unwrap_or_default())
            });
        state.min_spawn_delay = read_int(nbt, "MinSpawnDelay", DEFAULT_MIN_SPAWN_DELAY);
        state.max_spawn_delay = read_int(nbt, "MaxSpawnDelay", DEFAULT_MAX_SPAWN_DELAY);
        state.spawn_count = read_int(nbt, "SpawnCount", DEFAULT_SPAWN_COUNT);
        state.max_nearby_entities = read_int(nbt, "MaxNearbyEntities", DEFAULT_MAX_NEARBY_ENTITIES);
        state.required_player_range =
            read_int(nbt, "RequiredPlayerRange", DEFAULT_REQUIRED_PLAYER_RANGE);
        state.spawn_range = read_int(nbt, "SpawnRange", DEFAULT_SPAWN_RANGE);
    }

    /// Runs vanilla `BaseSpawner.save`.
    pub(super) fn save(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert("Delay", truncate_to_short(state.spawn_delay));
        nbt.insert("MinSpawnDelay", truncate_to_short(state.min_spawn_delay));
        nbt.insert("MaxSpawnDelay", truncate_to_short(state.max_spawn_delay));
        nbt.insert("SpawnCount", truncate_to_short(state.spawn_count));
        nbt.insert(
            "MaxNearbyEntities",
            truncate_to_short(state.max_nearby_entities),
        );
        nbt.insert(
            "RequiredPlayerRange",
            truncate_to_short(state.required_player_range),
        );
        nbt.insert("SpawnRange", truncate_to_short(state.spawn_range));
        if let Some(next) = &state.next_spawn_data {
            nbt.insert("SpawnData", NbtTag::Compound(next.write()));
        }
        nbt.insert("SpawnPotentials", state.spawn_potentials.write());
    }
}

/// Vanilla saves these counters as shorts.
#[expect(
    clippy::cast_possible_truncation,
    reason = "vanilla stores these spawner counters as NBT shorts"
)]
const fn truncate_to_short(value: i32) -> i16 {
    value as i16
}

fn read_int(nbt: &NbtCompoundView<'_, '_>, key: &str, fallback: i32) -> i32 {
    nbt.int(key)
        .or_else(|| nbt.short(key).map(i32::from))
        .unwrap_or(fallback)
}

/// Returns vanilla `EntityType.by(input)` for the stored spawn tag.
fn spawn_data_entity_type(data: &SpawnData) -> Option<EntityTypeRef> {
    let id = data.entity_to_spawn().get("id")?.string()?.to_string();
    REGISTRY
        .entity_types
        .by_key(&Identifier::from_str(&id).ok()?)
}

/// Reads the optional `Pos` override vanilla honors on a stored spawn tag.
fn read_saved_pos(data: &SpawnData) -> Option<DVec3> {
    let list = data.entity_to_spawn().get("Pos")?;
    let NbtList::Double(values) = list.list()? else {
        return None;
    };
    (values.len() == 3).then(|| DVec3::new(values[0], values[1], values[2]))
}

/// Returns vanilla `EntityType.loadEntityRecursive` for a spawner's stored entity tag.
///
/// Passenger stacks are not reconstructed: vanilla only produces them for spawners whose
/// stored tag carries `Passengers`, which no vanilla spawner does.
fn load_entity(
    world: &Arc<World>,
    entity_type: EntityTypeRef,
    entity_tag: &NbtCompound,
    pos: DVec3,
) -> Option<SharedEntity> {
    let mut bytes = Vec::new();
    entity_tag.clone().write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).ok()?;

    Some(ENTITIES.create_and_load_or_raw(
        EntityLoadRequest {
            entity_type,
            position: pos,
            uuid: Uuid::new_v4(),
            velocity: DVec3::ZERO,
            rotation: (0.0, 0.0),
            fall_distance: 0.0,
            fire_freeze: EntityFireFreezeState::new(),
            on_ground: false,
            save_data: EntityBaseSaveData::new(),
            world: Arc::downgrade(world),
        },
        &borrowed,
    ))
}
