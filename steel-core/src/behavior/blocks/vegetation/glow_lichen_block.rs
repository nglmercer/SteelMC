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

/// Vanilla `GlowLichenBlock` survival and placement.
///
/// Placement, survival and shape updates are inherited from `MultifaceBlock`.
// DEFERRED (Phase 4-8): Spread and bonemeal both go through vanilla's `MultifaceSpreader`
// (`canSpreadInAnyDirection` / `spreadFromRandomFaceTowardRandomDirection`), which Steel does
// not have — the existing `multiface_*` helpers only cover placement, survival, and shape
// updates. Sculk vein and the other multiface blocks need the same helper.
#[block_behavior]
pub struct GlowLichenBlock {
    block: BlockRef,
}

impl GlowLichenBlock {
    /// Creates a new glow lichen block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for GlowLichenBlock {
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

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        multiface_can_survive(state, world, pos)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        multiface_placement_state(self.block, context)
    }

    fn can_be_replaced(&self, state: BlockStateId, context: &BlockPlaceContext<'_>) -> bool {
        multiface_can_be_replaced(state, context)
    }
}
