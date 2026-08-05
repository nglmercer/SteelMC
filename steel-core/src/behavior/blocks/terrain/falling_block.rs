//! Blocks that fall when unsupported: sand, gravel, concrete powder.

use std::sync::Arc;

use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::shapes::SupportType;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::{
    BlockBehavior, BlockHitResult, BlockPlaceContext, Fallable, InteractionResult, InventoryAccess,
};
use crate::entity::ai::path::PathComputationType;
use crate::entity::entities::FallingBlockEntity;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Vanilla `FallingBlock.getDelayAfterPlace`.
const DELAY_AFTER_PLACE: i32 = 2;

/// Vanilla `FallingBlock.isFree`: whether a block state lets a falling block pass through.
#[must_use]
pub fn is_free(state: BlockStateId) -> bool {
    state.is_air()
        || state.get_block().has_tag(&BlockTag::FIRE)
        || state.get_block().config.liquid
        || state.is_replaceable()
}

/// Shared `FallingBlock` scheduling: a falling block re-checks its support after placement
/// and whenever a neighbor changes.
fn schedule_fall_check(world: &dyn ScheduledTickAccess, pos: BlockPos, block: BlockRef) {
    world.schedule_block_tick_default(pos, block, DELAY_AFTER_PLACE);
}

/// `on_place` receives an `Arc<World>`, which reaches [`ScheduledTickAccess`] through the
/// blanket impl on `Arc<World>` rather than on `World` itself.
fn schedule_fall_check_in_world(world: &Arc<World>, pos: BlockPos, block: BlockRef) {
    schedule_fall_check(world, pos, block);
}

/// Shared `FallingBlock.tick`: start falling when nothing supports the block.
fn try_start_falling(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
    if is_free(world.get_block_state(pos.below())) && pos.y() >= world.min_y() {
        FallingBlockEntity::fall(world, pos, state);
    }
}

/// Vanilla `ColoredFallingBlock` behavior (gravel).
///
/// Vanilla's `dustColor` only drives client-side falling particles.
#[block_behavior]
pub struct ColoredFallingBlock {
    block: BlockRef,
}

impl ColoredFallingBlock {
    /// Creates a new coloured falling block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for ColoredFallingBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        schedule_fall_check_in_world(world, pos, self.block);
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_fall_check(world, pos, self.block);
        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        try_start_falling(state, world, pos);
    }
}

/// Vanilla `SandBlock` behavior.
///
/// Identical to [`ColoredFallingBlock`] server-side; vanilla only adds ambient desert
/// sounds, which are client-local.
#[block_behavior]
pub struct SandBlock {
    falling: ColoredFallingBlock,
}

impl SandBlock {
    /// Creates a new sand block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            falling: ColoredFallingBlock::new(block),
        }
    }
}

impl BlockBehavior for SandBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.falling.get_state_for_placement(context)
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        moved_by_piston: bool,
    ) {
        self.falling
            .on_place(state, world, pos, old_state, moved_by_piston);
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.falling
            .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.falling.tick(state, world, pos);
    }
}

/// Vanilla `ConcretePowderBlock` behavior.
#[block_behavior]
pub struct ConcretePowderBlock {
    block: BlockRef,
    #[json_arg(vanilla_blocks, json = "concrete")]
    concrete: BlockRef,
}

impl ConcretePowderBlock {
    /// Creates a new concrete powder behavior solidifying into `concrete`.
    #[must_use]
    pub const fn new(block: BlockRef, concrete: BlockRef) -> Self {
        Self { block, concrete }
    }

    /// Vanilla `ConcretePowderBlock.canSolidify`.
    fn can_solidify(state: BlockStateId) -> bool {
        state.get_fluid_state().fluid_id.has_tag(&FluidTag::WATER)
    }

    /// Vanilla `ConcretePowderBlock.touchesLiquid`.
    fn touches_liquid(world: &dyn LevelReader, pos: BlockPos) -> bool {
        for direction in Direction::ALL {
            // Vanilla only considers water below when the block itself is already in water.
            if direction == Direction::Down && !Self::can_solidify(world.get_block_state(pos)) {
                continue;
            }

            let neighbor_pos = pos.relative(direction);
            let neighbor_state = world.get_block_state(neighbor_pos);
            if Self::can_solidify(neighbor_state)
                && !world.is_face_sturdy_for(
                    neighbor_state,
                    neighbor_pos,
                    direction.opposite(),
                    SupportType::Full,
                )
            {
                return true;
            }
        }

        false
    }

    /// Vanilla `ConcretePowderBlock.shouldSolidify`.
    fn should_solidify(world: &dyn LevelReader, pos: BlockPos, replaced: BlockStateId) -> bool {
        Self::can_solidify(replaced) || Self::touches_liquid(world, pos)
    }
}

impl BlockBehavior for ConcretePowderBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let pos = context.place_pos();
        let replaced = context.world.get_block_state(pos);
        if Self::should_solidify(context.world, pos, replaced) {
            return Some(self.concrete.default_state());
        }

        Some(self.block.default_state())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        schedule_fall_check_in_world(world, pos, self.block);
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if Self::touches_liquid(world, pos) {
            return self.concrete.default_state();
        }

        schedule_fall_check(world, pos, self.block);
        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        try_start_falling(state, world, pos);
    }

    fn as_fallable(&self) -> Option<&dyn Fallable> {
        Some(self)
    }
}

impl Fallable for ConcretePowderBlock {
    fn on_land(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        _state: BlockStateId,
        replaced: BlockStateId,
    ) {
        if Self::should_solidify(world.as_ref(), pos, replaced) {
            world.set_block(pos, self.concrete.default_state(), UpdateFlags::UPDATE_ALL);
        }
    }
}

/// Vanilla `DragonEggBlock.getDelayAfterPlace`.
const DRAGON_EGG_DELAY_AFTER_PLACE: i32 = 5;
/// Vanilla's attempt budget for finding a teleport destination.
const DRAGON_EGG_TELEPORT_ATTEMPTS: u32 = 1000;

/// Vanilla `DragonEggBlock` behavior.
#[block_behavior]
pub struct DragonEggBlock {
    block: BlockRef,
}

impl DragonEggBlock {
    /// Creates a new dragon egg behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `DragonEggBlock.teleport`: hops the egg to a nearby supported air block.
    fn teleport(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let mut rng = rand::rng();

        for _ in 0..DRAGON_EGG_TELEPORT_ATTEMPTS {
            let target = pos.offset(
                rng.random_range(0..16) - rng.random_range(0..16),
                rng.random_range(0..8) - rng.random_range(0..8),
                rng.random_range(0..16) - rng.random_range(0..16),
            );

            if !world.get_block_state(target).is_air()
                || world.get_block_state(target.below()).is_air()
                || !world.is_block_within_world_border(target)
                || world.is_outside_build_height(target.y())
            {
                continue;
            }

            world.set_block(target, state, UpdateFlags::UPDATE_CLIENTS);
            world.set_block(
                pos,
                vanilla_blocks::AIR.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
            return;
        }
    }
}

impl BlockBehavior for DragonEggBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, DRAGON_EGG_DELAY_AFTER_PLACE);
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        world.schedule_block_tick_default(pos, self.block, DRAGON_EGG_DELAY_AFTER_PLACE);
        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        try_start_falling(state, world, pos);
    }

    fn attack(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos, _player: &Player) {
        Self::teleport(state, world, pos);
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        Self::teleport(state, world, pos);
        InteractionResult::Success
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
