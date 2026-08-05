//! Copper golem statue behaviors.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, Pose};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::blocks::{WeatherState, WeatheringCopper};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::CopperGolemStatueBlockEntity;
use crate::entity::ai::path::PathComputationType;
use crate::player::Player;
use crate::world::{ScheduledTickAccess, World};

/// Vanilla `CopperGolemStatueBlock.Pose.getNextPose`: statues cycle through their poses.
const fn next_pose(pose: Pose) -> Pose {
    match pose {
        Pose::Standing => Pose::Sitting,
        Pose::Sitting => Pose::Running,
        Pose::Running => Pose::Star,
        Pose::Star => Pose::Standing,
    }
}

/// Vanilla `CopperGolemStatueBlock.getStateForPlacement`.
fn placement_state(block: BlockRef, context: &BlockPlaceContext<'_>) -> BlockStateId {
    block
        .default_state()
        .set_value(
            &BlockStateProperties::HORIZONTAL_FACING,
            context.horizontal_direction().opposite(),
        )
        .set_value(
            &BlockStateProperties::WATERLOGGED,
            context.is_water_source(),
        )
}

/// Vanilla `CopperGolemStatueBlock.useItemOn`: any non-axe item cycles the pose.
///
/// Axes pass through so they can still scrape or wax the statue.
fn cycle_pose(
    state: BlockStateId,
    world: &Arc<World>,
    pos: BlockPos,
    inv: &InventoryAccess,
) -> InteractionResult {
    if inv.with_item(|stack| stack.item().has_tag(&ItemTag::AXES)) {
        return InteractionResult::Pass;
    }

    let pose = state.get_value(&BlockStateProperties::COPPER_GOLEM_POSE);
    world.set_block(
        pos,
        state.set_value(&BlockStateProperties::COPPER_GOLEM_POSE, next_pose(pose)),
        UpdateFlags::UPDATE_ALL,
    );

    InteractionResult::Success
}

fn new_statue_block_entity(
    level: Weak<World>,
    pos: BlockPos,
    state: BlockStateId,
) -> BlockEntityCreation {
    BlockEntityCreation::Created(Arc::new(CopperGolemStatueBlockEntity::new(
        level, pos, state,
    )))
}

/// Vanilla `CopperGolemStatueBlock` behavior (the waxed variants).
///
/// Waxing a statue back into a live `CopperGolem` needs that entity, which Steel does not
/// have yet; the statue's stored name is kept so nothing is lost in the meantime.
#[block_behavior]
pub struct CopperGolemStatueBlock {
    block: BlockRef,
}

impl CopperGolemStatueBlock {
    /// Creates a new copper golem statue behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for CopperGolemStatueBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(placement_state(self.block, context))
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
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        cycle_pose(state, world, pos, inv)
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_statue_block_entity(level, pos, state)
    }

    fn should_keep_block_entity(&self, _old_state: BlockStateId, new_state: BlockStateId) -> bool {
        new_state
            .get_block()
            .has_tag(&BlockTag::COPPER_GOLEM_STATUES)
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        computation_type == PathComputationType::Water
            && state.get_fluid_state().fluid_id.has_tag(&FluidTag::WATER)
    }
}

/// Vanilla `WeatheringCopperGolemStatueBlock` behavior (the unwaxed variants).
#[block_behavior]
pub struct WeatheringCopperGolemStatueBlock {
    block: BlockRef,
    #[json_arg(r#enum = "WeatherState", json = "weathering_state")]
    weathering: WeatheringCopper,
}

impl WeatheringCopperGolemStatueBlock {
    /// Creates a new weathering copper golem statue behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weathering_state: WeatherState) -> Self {
        Self {
            block,
            weathering: WeatheringCopper::new(weathering_state),
        }
    }
}

impl BlockBehavior for WeatheringCopperGolemStatueBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(placement_state(self.block, context))
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
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        cycle_pose(state, world, pos, inv)
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
        new_statue_block_entity(level, pos, state)
    }

    fn should_keep_block_entity(&self, _old_state: BlockStateId, new_state: BlockStateId) -> bool {
        new_state
            .get_block()
            .has_tag(&BlockTag::COPPER_GOLEM_STATUES)
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        computation_type == PathComputationType::Water
            && state.get_fluid_state().fluid_id.has_tag(&FluidTag::WATER)
    }
}
