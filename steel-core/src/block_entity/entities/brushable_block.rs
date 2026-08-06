//! Brushable block entity (suspicious sand and suspicious gravel).

use std::mem;
use std::str::FromStr as _;
use std::sync::{Arc, Weak};

use steel_utils::locks::SyncMutex;
use steel_utils::random::{Random as _, RandomSource, legacy_random::LegacyRandom};
use steel_utils::types::UpdateFlags;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::LootContext;
use steel_registry::{
    REGISTRY, RegistryExt as _, level_events, vanilla_attributes, vanilla_block_entity_types,
    vanilla_blocks, vanilla_entities,
};
use steel_utils::{BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, Identifier};

use crate::behavior::{BLOCK_BEHAVIORS, Brushable};
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::entities::ItemEntity;
use crate::entity::{LivingEntity as _, entity_loot_ref, next_entity_id};
use crate::player::Player;
use crate::world::World;

/// Vanilla `BrushableBlockEntity.BRUSH_COOLDOWN_TICKS`.
const BRUSH_COOLDOWN_TICKS: i64 = 10;
/// Vanilla `BrushableBlockEntity.BRUSH_RESET_TICKS`.
const BRUSH_RESET_TICKS: i64 = 40;
/// Vanilla `BrushableBlockEntity.REQUIRED_BRUSHES_TO_BREAK`.
const REQUIRED_BRUSHES_TO_BREAK: i32 = 10;
/// Vanilla `BrushableBlockEntity.checkReset` retraction interval.
const RETRACTION_TICKS: i64 = 4;
/// Vanilla `BrushableBlock.TICK_DELAY`.
const TICK_DELAY: i32 = 2;

/// Mutable state of a brushable block.
struct BrushableState {
    brush_count: i32,
    brush_count_resets_at_tick: i64,
    cool_down_ends_at_tick: i64,
    item: ItemStack,
    hit_direction: Option<Direction>,
    loot_table: Option<Identifier>,
    loot_table_seed: i64,
}

/// Vanilla `BrushableBlockEntity`.
pub struct BrushableBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<BrushableState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BrushableBlockEntity`.
unsafe impl DowncastType for BrushableBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/brushable_block");
}

impl BrushableBlockEntity {
    /// Creates an empty brushable block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::BRUSHABLE_BLOCK,
                world,
                pos,
                state,
            ),
            state: SyncMutex::new(BrushableState {
                brush_count: 0,
                brush_count_resets_at_tick: 0,
                cool_down_ends_at_tick: 0,
                item: ItemStack::empty(),
                hit_direction: None,
                loot_table: None,
                loot_table_seed: 0,
            }),
        }
    }

    /// Vanilla `BrushableBlockEntity.setLootTable`: assigns deferred structure loot.
    pub fn set_loot_table(&self, loot_table: Identifier, seed: i64) {
        let mut state = self.state.lock();
        state.loot_table = Some(loot_table);
        state.loot_table_seed = seed;
    }

    /// Vanilla `BrushableBlockEntity.brush`: advances the brushing progress by one stroke.
    ///
    /// Returns whether brushing completed and the block turned into its unpacked form.
    pub fn brush(
        &self,
        game_time: i64,
        world: &Arc<World>,
        user: &Player,
        direction: Direction,
        brush: &ItemStack,
    ) -> bool {
        let pos = self.get_block_pos();
        let (completed, dusted_change) = {
            let mut state = self.state.lock();
            if state.hit_direction.is_none() {
                state.hit_direction = Some(direction);
            }

            state.brush_count_resets_at_tick = game_time + BRUSH_RESET_TICKS;
            if game_time < state.cool_down_ends_at_tick {
                return false;
            }

            state.cool_down_ends_at_tick = game_time + BRUSH_COOLDOWN_TICKS;
            Self::unpack_loot_table(&mut state, user, brush, pos);

            let previous = Self::get_completion_state(state.brush_count);
            state.brush_count += 1;
            if state.brush_count >= REQUIRED_BRUSHES_TO_BREAK {
                (true, None)
            } else {
                let current = Self::get_completion_state(state.brush_count);
                (false, (previous != current).then_some(current))
            }
        };
        self.set_changed();

        if completed {
            self.brushing_completed(world, user, brush);
            return true;
        }

        let block_state = self.get_block_state();
        world.schedule_block_tick_default(pos, block_state.get_block(), TICK_DELAY);
        if let Some(dusted) = dusted_change {
            world.set_block(
                pos,
                block_state.set_value(&BlockStateProperties::DUSTED, dusted),
                UpdateFlags::UPDATE_ALL,
            );
        }

        false
    }

    /// Vanilla `BrushableBlockEntity.brushingCompleted`.
    fn brushing_completed(&self, world: &Arc<World>, user: &Player, brush: &ItemStack) {
        self.drop_content(world, user, brush);

        let pos = self.get_block_pos();
        let block_state = self.get_block_state();
        world.level_event(
            level_events::PARTICLES_AND_SOUND_BRUSH_BLOCK_COMPLETE,
            pos,
            level_events::encode_block_state_data(u32::from(block_state.0)),
            None,
        );

        // Vanilla falls back to air if the state is somehow not a brushable block.
        let turns_into = BLOCK_BEHAVIORS
            .get_behavior_for_state(block_state)
            .and_then(|behavior| behavior.as_brushable())
            .map_or(&vanilla_blocks::AIR, Brushable::turns_into);
        world.set_block(pos, turns_into.default_state(), UpdateFlags::UPDATE_ALL);
    }

    /// Vanilla `BrushableBlockEntity.dropContent`.
    fn drop_content(&self, world: &Arc<World>, user: &Player, brush: &ItemStack) {
        let (mut item, hit_direction) = {
            let mut state = self.state.lock();
            Self::unpack_loot_table(&mut state, user, brush, self.get_block_pos());
            let item = mem::replace(&mut state.item, ItemStack::empty());
            (item, state.hit_direction)
        };
        self.set_changed();

        if item.is_empty() {
            return;
        }

        let dimensions = vanilla_entities::ITEM.dimensions;
        let size = f64::from(dimensions.width);
        let center_range = 1.0 - size;
        let half_size = size / 2.0;
        let drop_direction = hit_direction.unwrap_or(Direction::Up);
        let drop_pos = self.get_block_pos().relative(drop_direction);
        let position = glam::DVec3::new(
            f64::from(drop_pos.x()) + 0.5 * center_range + half_size,
            f64::from(drop_pos.y()) + 0.5 + f64::from(dimensions.height) / 2.0,
            f64::from(drop_pos.z()) + 0.5 * center_range + half_size,
        );

        // Vanilla: `item.split(level.getRandom().nextInt(21) + 10)`.
        let mut rng = RandomSource::create_thread_safe();
        let dropped = item.split(rng.next_i32_bounded(21) + 10);

        let entity = Arc::new(ItemEntity::with_item_and_velocity(
            &vanilla_entities::ITEM,
            next_entity_id(),
            position,
            dropped,
            glam::DVec3::ZERO,
            Arc::downgrade(world),
        ));
        if let Err(error) = world.try_add_entity(entity) {
            log::warn!("failed to spawn brushed item: {error}");
        }
    }

    /// Vanilla `BrushableBlockEntity.checkReset`: retracts brushing progress over time.
    pub fn check_reset(&self, world: &Arc<World>) {
        let game_time = world.game_time();
        let pos = self.get_block_pos();
        let block_state = self.get_block_state();

        let (dusted_change, still_brushing) = {
            let mut state = self.state.lock();
            let mut dusted_change = None;
            if state.brush_count != 0 && game_time >= state.brush_count_resets_at_tick {
                let previous = Self::get_completion_state(state.brush_count);
                state.brush_count = (state.brush_count - 2).max(0);
                let current = Self::get_completion_state(state.brush_count);
                if previous != current {
                    dusted_change = Some(current);
                }

                state.brush_count_resets_at_tick = game_time + RETRACTION_TICKS;
            }

            if state.brush_count == 0 {
                state.hit_direction = None;
                state.brush_count_resets_at_tick = 0;
                state.cool_down_ends_at_tick = 0;
            }

            (dusted_change, state.brush_count != 0)
        };

        if let Some(dusted) = dusted_change {
            world.set_block(
                pos,
                block_state.set_value(&BlockStateProperties::DUSTED, dusted),
                UpdateFlags::UPDATE_ALL,
            );
        }
        if still_brushing {
            world.schedule_block_tick_default(pos, block_state.get_block(), TICK_DELAY);
        }
    }

    /// Vanilla `BrushableBlockEntity.unpackLootTable`.
    fn unpack_loot_table(
        state: &mut BrushableState,
        user: &Player,
        brush: &ItemStack,
        pos: BlockPos,
    ) {
        let Some(key) = state.loot_table.take() else {
            return;
        };
        // Vanilla fires the `GENERATE_LOOT` advancement criterion here; advancements are not
        // implemented yet.
        let Some(table) = REGISTRY.loot_tables.by_key(&key) else {
            log::warn!("brushable block at {pos:?} references unknown loot table {key}");
            return;
        };

        let luck = user
            .attributes()
            .lock()
            .get_value(vanilla_attributes::LUCK)
            .unwrap_or(0.0);
        // Vanilla resolves the roll with `lootTable.getRandomItems(params, lootTableSeed)`:
        // a non-zero seed uses a legacy LCG, seed 0 falls back to the level random.
        let seed = state.loot_table_seed;
        let mut rng = if seed != 0 {
            RandomSource::Legacy(LegacyRandom::from_seed(seed as u64))
        } else {
            RandomSource::create_thread_safe()
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "luck is bounded to [-1024, 1024]"
        )]
        let luck = luck as f32;
        let mut context = LootContext::new(&mut rng)
            .with_luck(luck)
            .with_origin(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 0.5,
                f64::from(pos.z()) + 0.5,
            )
            .with_this_entity(entity_loot_ref(user))
            .with_tool(brush);
        let loot = table.get_random_items(&mut context);

        if loot.len() > 1 {
            log::warn!(
                "expected max 1 loot from loot table {key}, but got {}",
                loot.len()
            );
        }
        state.item = loot.into_iter().next().unwrap_or_else(ItemStack::empty);
    }

    /// Vanilla `BrushableBlockEntity.getCompletionState`.
    const fn get_completion_state(brush_count: i32) -> u8 {
        if brush_count == 0 {
            0
        } else if brush_count < 3 {
            1
        } else if brush_count < 6 {
            2
        } else {
            3
        }
    }

    /// Vanilla `BrushableBlockEntity.tryLoadLootTable`.
    fn try_load_loot_table(state: &mut BrushableState, nbt: &NbtCompoundView<'_, '_>) -> bool {
        state.loot_table = nbt
            .string("LootTable")
            .and_then(|value| Identifier::from_str(&value.to_string()).ok());
        state.loot_table_seed = nbt.long("LootTableSeed").unwrap_or(0);
        state.loot_table.is_some()
    }

    /// Vanilla `BrushableBlockEntity.trySaveLootTable`.
    fn try_save_loot_table(state: &BrushableState, nbt: &mut NbtCompound) -> bool {
        let Some(loot_table) = state.loot_table.as_ref() else {
            return false;
        };

        nbt.insert("LootTable", loot_table.to_string());
        if state.loot_table_seed != 0 {
            nbt.insert("LootTableSeed", state.loot_table_seed);
        }

        true
    }
}

/// Encodes a direction using vanilla's legacy ids (used for the `hit_direction` NBT tag).
const fn direction_legacy_id(direction: Direction) -> i8 {
    match direction {
        Direction::Down => 0,
        Direction::Up => 1,
        Direction::North => 2,
        Direction::South => 3,
        Direction::West => 4,
        Direction::East => 5,
    }
}

/// Decodes a direction stored with vanilla's legacy ids.
const fn direction_from_legacy_id(id: i8) -> Option<Direction> {
    match id {
        0 => Some(Direction::Down),
        1 => Some(Direction::Up),
        2 => Some(Direction::North),
        3 => Some(Direction::South),
        4 => Some(Direction::West),
        5 => Some(Direction::East),
        _ => None,
    }
}

impl BlockEntity for BrushableBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();

        if Self::try_load_loot_table(&mut state, &nbt_view) {
            state.item = ItemStack::empty();
        } else {
            state.item = nbt_view
                .compound("item")
                .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
                .unwrap_or_else(ItemStack::empty);
        }

        state.hit_direction = nbt_view
            .byte("hit_direction")
            .and_then(direction_from_legacy_id);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        if Self::try_save_loot_table(&state, nbt) {
            return;
        }

        if !state.item.is_empty()
            && let NbtTag::Compound(item_nbt) = state.item.to_nbt_tag_ref()
        {
            nbt.insert("item", item_nbt);
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        let state = self.state.lock();
        let mut nbt = NbtCompound::new();

        if let Some(direction) = state.hit_direction {
            nbt.insert("hit_direction", direction_legacy_id(direction));
        }
        if !state.item.is_empty()
            && let NbtTag::Compound(item_nbt) = state.item.to_nbt_tag_ref()
        {
            nbt.insert("item", item_nbt);
        }

        Some(nbt)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use std::string::ToString;
    use steel_registry::test_support::init_test_registry;
    use steel_utils::Identifier;

    use super::*;

    fn test_entity() -> Arc<BrushableBlockEntity> {
        init_test_registry();
        Arc::new(BrushableBlockEntity::new(
            Weak::new(),
            BlockPos::new(0, 64, 0),
            vanilla_blocks::SUSPICIOUS_SAND.default_state(),
        ))
    }

    fn reload(nbt: &NbtCompound) -> Arc<BrushableBlockEntity> {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("test NBT should reborrow");
        let loaded = test_entity();
        loaded.load_additional(&borrowed);
        loaded
    }

    #[test]
    fn completion_state_matches_vanilla_thresholds() {
        assert_eq!(BrushableBlockEntity::get_completion_state(0), 0);
        assert_eq!(BrushableBlockEntity::get_completion_state(1), 1);
        assert_eq!(BrushableBlockEntity::get_completion_state(2), 1);
        assert_eq!(BrushableBlockEntity::get_completion_state(3), 2);
        assert_eq!(BrushableBlockEntity::get_completion_state(5), 2);
        assert_eq!(BrushableBlockEntity::get_completion_state(6), 3);
        assert_eq!(BrushableBlockEntity::get_completion_state(9), 3);
    }

    #[test]
    fn loot_table_state_round_trips_through_nbt() {
        let entity = test_entity();
        entity.set_loot_table(
            Identifier::from_str("minecraft:archaeology/desert_pyramid").expect("valid identifier"),
            42,
        );

        let mut nbt = NbtCompound::new();
        entity.save_additional(&mut nbt);
        assert_eq!(
            nbt.string("LootTable").map(ToString::to_string),
            Some("minecraft:archaeology/desert_pyramid".to_string())
        );
        assert_eq!(nbt.long("LootTableSeed"), Some(42));

        let loaded = reload(&nbt);
        let state = loaded.state.lock();
        assert_eq!(
            state.loot_table.as_ref().map(ToString::to_string),
            Some("minecraft:archaeology/desert_pyramid".to_string())
        );
        assert_eq!(state.loot_table_seed, 42);
    }

    #[test]
    fn hit_direction_round_trips_through_update_tag() {
        let entity = test_entity();
        entity.state.lock().hit_direction = Some(Direction::West);

        let nbt = entity.get_update_tag().expect("update tag expected");
        assert_eq!(nbt.byte("hit_direction"), Some(4));

        let loaded = reload(&nbt);
        assert_eq!(loaded.state.lock().hit_direction, Some(Direction::West));
    }

    #[test]
    fn legacy_direction_ids_match_vanilla_order() {
        for (id, direction) in [
            Direction::Down,
            Direction::Up,
            Direction::North,
            Direction::South,
            Direction::West,
            Direction::East,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(direction_legacy_id(direction), id as i8);
            assert_eq!(direction_from_legacy_id(id as i8), Some(direction));
        }
        assert_eq!(direction_from_legacy_id(6), None);
    }
}
