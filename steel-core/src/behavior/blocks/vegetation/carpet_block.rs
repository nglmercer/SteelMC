use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::BlockBehavior;
use crate::behavior::blocks::vegetation::vegetation_block::survival_update_shape;
use crate::behavior::context::BlockPlaceContext;
use crate::world::{LevelReader, ScheduledTickAccess};

use super::{BlockRef, default_surviving_state};

/// Vanilla `CarpetBlock` survival and shape updates.
#[block_behavior]
pub struct CarpetBlock {
    block: BlockRef,
}

impl CarpetBlock {
    /// Creates a new carpet block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for CarpetBlock {
    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        // Vanilla `CarpetBlock.updateShape`: break when support is removed.
        survival_update_shape(self, state, world, pos)
    }

    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        !world.get_block_state(pos.below()).is_air()
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        default_surviving_state(self.block, self, context)
    }
}

/// Vanilla `WoolCarpetBlock` behavior.
///
/// Identical to [`CarpetBlock`] server-side; vanilla only adds the dye color, which is
/// used for rendering and recipes rather than block behavior.
#[block_behavior]
pub struct WoolCarpetBlock {
    carpet: CarpetBlock,
}

impl WoolCarpetBlock {
    /// Creates a new wool carpet block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            carpet: CarpetBlock::new(block),
        }
    }
}

impl BlockBehavior for WoolCarpetBlock {
    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.carpet
            .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        self.carpet.can_survive(state, world, pos)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.carpet.get_state_for_placement(context)
    }
}
