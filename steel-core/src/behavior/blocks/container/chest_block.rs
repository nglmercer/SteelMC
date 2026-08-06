//! Chest, trapped chest and copper chest behaviors.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, ChestType, Direction};
use steel_registry::vanilla_block_entity_types;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::{BlockPos, BlockStateId, axis::Axis, translations};
use text_components::TextComponent;

use steel_utils::Downcast as _;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::blocks::{WeatherState, WeatheringCopper};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::ChestBlockEntity;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::{chest, double_chest};
use crate::player::Player;
use crate::world::{
    LevelReader, ScheduledTickAccess, SignalQueryContext, World, is_redstone_conductor,
};

/// Rows shown for a single chest.
const SINGLE_CHEST_ROWS: usize = 3;

/// Which blocks a chest is allowed to pair with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChestPairing {
    /// Pairs only with the exact same block (plain and trapped chests).
    SameBlock,
    /// Pairs with any block in the `copper_chests` tag, regardless of oxidation.
    CopperChest,
}

/// Shared vanilla `ChestBlock` logic, parameterised by which chests may connect.
struct ChestBehavior {
    block: BlockRef,
    kind: ChestPairing,
    block_entity_type: BlockEntityTypeRef,
}

impl ChestBehavior {
    const fn new(
        block: BlockRef,
        kind: ChestPairing,
        block_entity_type: BlockEntityTypeRef,
    ) -> Self {
        Self {
            block,
            kind,
            block_entity_type,
        }
    }

    /// Vanilla `ChestBlock.chestCanConnectTo`.
    fn can_connect_to(&self, state: BlockStateId) -> bool {
        match self.kind {
            ChestPairing::SameBlock => state.get_block() == self.block,
            ChestPairing::CopperChest => {
                state.get_block().has_tag(&BlockTag::COPPER_CHESTS)
                    && state
                        .try_get_value(&BlockStateProperties::CHEST_TYPE)
                        .is_some()
            }
        }
    }

    /// Vanilla `ChestBlock.getConnectedDirection`: which side the paired half sits on.
    fn connected_direction(state: BlockStateId) -> Direction {
        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        if state.get_value(&BlockStateProperties::CHEST_TYPE) == ChestType::Left {
            facing.rotate_y_clockwise()
        } else {
            facing.rotate_y_counter_clockwise()
        }
    }

    /// Vanilla `ChestBlock.candidatePartnerFacing`.
    fn candidate_partner_facing(
        &self,
        world: &dyn LevelReader,
        pos: BlockPos,
        neighbor_direction: Direction,
    ) -> Option<Direction> {
        let state = world.get_block_state(pos.relative(neighbor_direction));
        if self.can_connect_to(state)
            && state.get_value(&BlockStateProperties::CHEST_TYPE) == ChestType::Single
        {
            Some(state.get_value(&BlockStateProperties::HORIZONTAL_FACING))
        } else {
            None
        }
    }

    /// Vanilla `ChestBlock.getChestType`.
    fn chest_type(&self, world: &dyn LevelReader, pos: BlockPos, facing: Direction) -> ChestType {
        if self.candidate_partner_facing(world, pos, facing.rotate_y_clockwise()) == Some(facing) {
            ChestType::Left
        } else if self.candidate_partner_facing(world, pos, facing.rotate_y_counter_clockwise())
            == Some(facing)
        {
            ChestType::Right
        } else {
            ChestType::Single
        }
    }

    fn placement_state(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        let pos = context.place_pos();
        let mut chest_type = ChestType::Single;
        let mut facing = context.horizontal_direction().opposite();

        let secondary_use = context.is_secondary_use_active();
        let clicked_face = context.clicked_face();

        // Sneak-placing against a chest's side forces a pair on that side.
        if clicked_face.get_axis() != Axis::Y && secondary_use {
            let neighbor_facing =
                self.candidate_partner_facing(context.world, pos, clicked_face.opposite());
            if let Some(neighbor_facing) = neighbor_facing
                && neighbor_facing.get_axis() != clicked_face.get_axis()
            {
                facing = neighbor_facing;
                chest_type = if facing.rotate_y_counter_clockwise() == clicked_face.opposite() {
                    ChestType::Right
                } else {
                    ChestType::Left
                };
            }
        }

        if chest_type == ChestType::Single && !secondary_use {
            chest_type = self.chest_type(context.world, pos, facing);
        }

        self.block
            .default_state()
            .set_value(&BlockStateProperties::HORIZONTAL_FACING, facing)
            .set_value(&BlockStateProperties::CHEST_TYPE, chest_type)
            .set_value(
                &BlockStateProperties::WATERLOGGED,
                context.is_water_source(),
            )
    }

    fn shape_update(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);

        if self.can_connect_to(neighbor_state) && direction.get_axis() != Axis::Y {
            let neighbor_type = neighbor_state.get_value(&BlockStateProperties::CHEST_TYPE);
            if state.get_value(&BlockStateProperties::CHEST_TYPE) == ChestType::Single
                && neighbor_type != ChestType::Single
                && state.get_value(&BlockStateProperties::HORIZONTAL_FACING)
                    == neighbor_state.get_value(&BlockStateProperties::HORIZONTAL_FACING)
                && Self::connected_direction(neighbor_state) == direction.opposite()
            {
                let paired_type = match neighbor_type {
                    ChestType::Left => ChestType::Right,
                    ChestType::Right => ChestType::Left,
                    ChestType::Single => ChestType::Single,
                };
                return state.set_value(&BlockStateProperties::CHEST_TYPE, paired_type);
            }
        } else if Self::connected_direction(state) == direction {
            return state.set_value(&BlockStateProperties::CHEST_TYPE, ChestType::Single);
        }

        state
    }

    /// Vanilla `ChestBlock.isChestBlockedAt` — solid block above.
    fn is_chest_blocked_at(world: &dyn LevelReader, pos: BlockPos) -> bool {
        let above = pos.above();
        let state = world.get_block_state(above);
        is_redstone_conductor(world, state, above)
        // TODO: cat sitting check — requires cat entity query, skipped for now.
    }

    /// Whether this chest (or double) can be opened.
    fn can_open(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        if Self::is_chest_blocked_at(world, pos) {
            return false;
        }
        let chest_type = state.get_value(&BlockStateProperties::CHEST_TYPE);
        if chest_type != ChestType::Single {
            let partner_pos = pos.relative(Self::connected_direction(state));
            if Self::is_chest_blocked_at(world, partner_pos) {
                return false;
            }
        }
        true
    }

    /// Opens the single or double chest menu, mirroring vanilla's `MENU_PROVIDER_COMBINER`.
    fn open(state: BlockStateId, world: &Arc<World>, pos: BlockPos, player: &Player) {
        if !Self::can_open(state, world.as_ref(), pos) {
            return;
        }
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return;
        };

        let chest_type = state.get_value(&BlockStateProperties::CHEST_TYPE);
        if chest_type == ChestType::Single {
            let inventory = player.inventory.clone();
            player.open_menu(
                TextComponent::translated(translations::CONTAINER_CHEST.msg()),
                move |context| {
                    chest(
                        inventory,
                        context.container_id,
                        container_ref,
                        SINGLE_CHEST_ROWS,
                    )
                },
            );
            return;
        }

        let partner_pos = pos.relative(Self::connected_direction(state));
        let Some(partner_ref) = world
            .get_block_entity(partner_pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return;
        };

        // Vanilla's `CompoundContainer` puts the RIGHT half first.
        let (top, bottom) = if chest_type == ChestType::Right {
            (container_ref, partner_ref)
        } else {
            (partner_ref, container_ref)
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_CHEST_DOUBLE.msg()),
            move |context| double_chest(inventory, context.container_id, top, bottom),
        );
    }

    fn analog_output(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> i32 {
        // For double chests, combine both halves like vanilla `getContainer(...).apply(CHEST_COMBINER)`.
        let chest_type = state.get_value(&BlockStateProperties::CHEST_TYPE);
        if chest_type != ChestType::Single {
            let partner_pos = pos.relative(Self::connected_direction(state));
            let Some(a) = world
                .get_block_entity(pos)
                .and_then(ContainerRef::from_block_entity)
            else {
                return 0;
            };
            let Some(b) = world
                .get_block_entity(partner_pos)
                .and_then(ContainerRef::from_block_entity)
            else {
                return Self::analog_single(world, pos);
            };
            // Check blocked — if blocked, signal is 0 like vanilla's `getContainer(..., false)` returns empty.
            if Self::is_chest_blocked_at(world, pos)
                || Self::is_chest_blocked_at(world, partner_pos)
            {
                return 0;
            }
            let guard = ContainerLockGuard::lock_all(&[&a, &b]);
            let ca = guard.get(a.container_id());
            let cb = guard.get(b.container_id());
            match (ca, cb) {
                (Some(ca), Some(cb)) => {
                    // Vanilla's CompoundContainer calculates signal over combined slots.
                    // Approximate by combined fullness: weighted average of both halves.
                    // For correctness, compute total items proportion.
                    let signal_a = calculate_redstone_signal_from_container(ca);
                    let signal_b = calculate_redstone_signal_from_container(cb);
                    // If either half has items, use max; if both empty, 0. This matches vanilla's
                    // `calculateRedstoneSignalFromContainer` over 54 slots.
                    // Compute exact combined by merging item counts.
                    let total_slots = (ca.get_container_size() + cb.get_container_size()) as f32;
                    let filled_a = signal_a as f32 / 15.0 * ca.get_container_size() as f32;
                    let filled_b = signal_b as f32 / 15.0 * cb.get_container_size() as f32;
                    let combined = ((filled_a + filled_b) / total_slots * 15.0).floor() as i32;
                    combined.clamp(0, 15)
                }
                _ => 0,
            }
        } else {
            Self::analog_single(world, pos)
        }
    }

    fn analog_single(world: &dyn LevelReader, pos: BlockPos) -> i32 {
        if Self::is_chest_blocked_at(world, pos) {
            // Vanilla `getContainer(..., false)` returns empty when blocked, so signal is 0.
            // However trapped chest signal is openCount, not container fill. This path is for
            // normal chest; trapped chest will override via signal source. Return 0 when blocked.
            return 0;
        }
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, |container| {
                calculate_redstone_signal_from_container(container)
            })
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(ChestBlockEntity::new(
            self.block_entity_type,
            level,
            pos,
            state,
        )))
    }
}

/// Generates the `BlockBehavior` impl shared by every chest variant.
macro_rules! chest_block_behavior {
    ($ty:ident) => {
        impl BlockBehavior for $ty {
            fn get_state_for_placement(
                &self,
                context: &BlockPlaceContext<'_>,
            ) -> Option<BlockStateId> {
                Some(self.chest.placement_state(context))
            }

            fn update_shape(
                &self,
                state: BlockStateId,
                world: &dyn ScheduledTickAccess,
                pos: BlockPos,
                direction: Direction,
                _neighbor_pos: BlockPos,
                neighbor_state: BlockStateId,
            ) -> BlockStateId {
                self.chest
                    .shape_update(state, world, pos, direction, neighbor_state)
            }

            fn use_without_item(
                &self,
                state: BlockStateId,
                world: &Arc<World>,
                pos: BlockPos,
                player: &Player,
                _hit_result: &BlockHitResult,
                _inv: &mut InventoryAccess,
            ) -> InteractionResult {
                ChestBehavior::open(state, world, pos, player);
                InteractionResult::Success
            }

            fn new_block_entity(
                &self,
                level: Weak<World>,
                pos: BlockPos,
                state: BlockStateId,
            ) -> BlockEntityCreation {
                self.chest.new_block_entity(level, pos, state)
            }

            fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
                true
            }

            fn get_analog_output_signal(
                &self,
                state: BlockStateId,
                world: &dyn LevelReader,
                pos: BlockPos,
                _direction: Direction,
            ) -> i32 {
                ChestBehavior::analog_output(state, world, pos)
            }

            fn tick(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
                if let Some(entity) = world.get_block_entity(pos) {
                    if let Some(chest) = entity.downcast_ref::<ChestBlockEntity>() {
                        chest.recheck_open();
                    }
                }
            }

            fn trigger_event(
                &self,
                state: BlockStateId,
                world: &Arc<World>,
                pos: BlockPos,
                param_a: i32,
                param_b: i32,
            ) -> bool {
                if let Some(entity) = world.get_block_entity(pos) {
                    return entity.trigger_event(param_a, param_b);
                }
                let _ = state;
                false
            }
        }
    };
}

/// Vanilla `ChestBlock` behavior.
#[block_behavior]
pub struct ChestBlock {
    chest: ChestBehavior,
}

impl ChestBlock {
    /// Creates a new chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::SameBlock,
                &vanilla_block_entity_types::CHEST,
            ),
        }
    }
}

chest_block_behavior!(ChestBlock);

/// Vanilla `TrappedChestBlock` behavior.
#[block_behavior]
pub struct TrappedChestBlock {
    chest: ChestBehavior,
}

impl TrappedChestBlock {
    /// Creates a new trapped chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::SameBlock,
                &vanilla_block_entity_types::TRAPPED_CHEST,
            ),
        }
    }
}

impl BlockBehavior for TrappedChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.chest.placement_state(context))
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.chest
            .shape_update(state, world, pos, direction, neighbor_state)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        ChestBehavior::open(state, world, pos, player);
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.chest.new_block_entity(level, pos, state)
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        ChestBehavior::analog_output(state, world, pos)
    }

    fn tick(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if let Some(entity) = world.get_block_entity(pos) {
            if let Some(chest) = entity.downcast_ref::<ChestBlockEntity>() {
                chest.recheck_open();
            }
        }
    }

    fn trigger_event(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        param_a: i32,
        param_b: i32,
    ) -> bool {
        if let Some(entity) = world.get_block_entity(pos) {
            return entity.trigger_event(param_a, param_b);
        }
        let _ = state;
        false
    }

    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    fn get_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        world
            .get_block_entity(pos)
            .and_then(|e| {
                e.downcast_ref::<ChestBlockEntity>()
                    .map(|c| c.open_count().clamp(0, 15))
            })
            .unwrap_or(0)
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
        context: SignalQueryContext,
    ) -> i32 {
        if direction == Direction::Up {
            self.get_signal(state, world, pos, direction, context)
        } else {
            0
        }
    }
}

/// Vanilla `CopperChestBlock` behavior (the waxed variants).
///
/// Vanilla additionally rewrites a newly placed copper chest to the least oxidized of the
/// two connected halves; that mapping needs the copper-block weathering chain and is not
/// applied here.
#[block_behavior]
pub struct CopperChestBlock {
    chest: ChestBehavior,
}

impl CopperChestBlock {
    /// Creates a new copper chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::CopperChest,
                &vanilla_block_entity_types::CHEST,
            ),
        }
    }
}

chest_block_behavior!(CopperChestBlock);

/// Vanilla `WeatheringCopperChestBlock` behavior (the unwaxed variants).
#[block_behavior]
pub struct WeatheringCopperChestBlock {
    chest: ChestBehavior,
    #[json_arg(r#enum = "WeatherState", json = "weather_state")]
    weathering: WeatheringCopper,
}

impl WeatheringCopperChestBlock {
    /// Creates a new weathering copper chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weather_state: WeatherState) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::CopperChest,
                &vanilla_block_entity_types::CHEST,
            ),
            weathering: WeatheringCopper::new(weather_state),
        }
    }
}

impl BlockBehavior for WeatheringCopperChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.chest.placement_state(context))
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.chest
            .shape_update(state, world, pos, direction, neighbor_state)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        ChestBehavior::open(state, world, pos, player);
        InteractionResult::Success
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.weathering.change_over_time(state, world, pos);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.chest.new_block_entity(level, pos, state)
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        ChestBehavior::analog_output(state, world, pos)
    }

    fn tick(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if let Some(entity) = world.get_block_entity(pos) {
            if let Some(chest) = entity.downcast_ref::<ChestBlockEntity>() {
                chest.recheck_open();
            }
        }
    }

    fn trigger_event(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        param_a: i32,
        param_b: i32,
    ) -> bool {
        if let Some(entity) = world.get_block_entity(pos) {
            return entity.trigger_event(param_a, param_b);
        }
        let _ = state;
        false
    }
}
