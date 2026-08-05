//! Generic multiface block behavior (resin clumps).

use steel_macros::block_behavior;
use steel_registry::blocks::properties::Direction;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::world::{LevelReader, ScheduledTickAccess};

use super::{
    BlockRef, multiface_can_be_replaced, multiface_can_survive, multiface_placement_state,
    update_multiface_shape,
};

/// Vanilla `MultifaceBlock` behavior.
///
/// Vanilla's `canRotate` / `canMirrorX` / `canMirrorZ` constructor flags only affect
/// structure rotation and mirroring, which Steel applies through structure placement
/// rather than block behavior, so they are not carried here.
#[block_behavior]
pub struct MultifaceBlock {
    block: BlockRef,
}

impl MultifaceBlock {
    /// Creates a new multiface block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for MultifaceBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        multiface_placement_state(self.block, context)
    }

    fn can_be_replaced(&self, state: BlockStateId, context: &BlockPlaceContext<'_>) -> bool {
        multiface_can_be_replaced(state, context)
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        multiface_can_survive(state, world, pos)
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
        update_multiface_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }
}
