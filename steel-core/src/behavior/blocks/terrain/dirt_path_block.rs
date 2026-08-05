//! Dirt path block behavior.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::push_entities_up;
use crate::behavior::blocks::vegetation::FarmlandBlock;
use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::entity::ai::path::PathComputationType;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Vanilla `DirtPathBlock` behavior.
#[block_behavior]
pub struct DirtPathBlock {
    block: BlockRef,
}

impl DirtPathBlock {
    /// Creates a new dirt path block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for DirtPathBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let state = self.block.default_state();
        if self.can_survive(state, context.world, context.place_pos()) {
            return Some(state);
        }

        // Vanilla places dirt instead when the path could not survive where it was placed.
        Some(push_entities_up(
            state,
            vanilla_blocks::DIRT.default_state(),
            context.world,
            context.place_pos(),
        ))
    }

    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = world.get_block_state(pos.above());
        !above_state.is_solid() || above_state.get_block().has_tag(&BlockTag::FENCE_GATES)
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if direction == Direction::Up && !self.can_survive(state, world, pos) {
            world.schedule_block_tick_default(pos, self.block, 1);
        }

        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        FarmlandBlock::turn_to_dirt(state, world, pos, None);
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
