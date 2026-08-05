//! Heavy core block behavior.

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::schedule_water_tick_if_waterlogged;
use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::entity::ai::path::PathComputationType;
use crate::world::ScheduledTickAccess;

/// Waterlogged property.
const WATERLOGGED: BoolProperty = BlockStateProperties::WATERLOGGED;

/// Vanilla `HeavyCoreBlock` behavior.
#[block_behavior]
pub struct HeavyCoreBlock {
    block: BlockRef,
}

impl HeavyCoreBlock {
    /// Creates a new heavy core block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for HeavyCoreBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(&WATERLOGGED, context.is_water_source()),
        )
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

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
