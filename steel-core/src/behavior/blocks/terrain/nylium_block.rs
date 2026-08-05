//! Nylium block behavior (crimson and warped nylium).

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::vanilla_blocks;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::chunk::light::get_light_block_into;
use crate::world::{LevelReader, World};

/// Light dampening at or above which nylium decays back into netherrack.
const LETHAL_LIGHT_DAMPENING: u8 = 15;

/// Vanilla `NyliumBlock` behavior.
///
/// Bonemealing nylium places nether vegetation features, which Steel cannot do outside
/// chunk generation yet, so only the decay-to-netherrack rule is implemented.
#[block_behavior]
pub struct NyliumBlock {
    block: BlockRef,
}

impl NyliumBlock {
    /// Creates a new nylium block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `NyliumBlock.canBeNylium`.
    fn can_be_nylium(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = world.get_block_state(pos.above());
        get_light_block_into(
            state,
            above_state,
            Direction::Up,
            above_state.get_light_dampening(),
        ) < LETHAL_LIGHT_DAMPENING
    }
}

impl BlockBehavior for NyliumBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !Self::can_be_nylium(state, world.as_ref(), pos) {
            world.set_block(
                pos,
                vanilla_blocks::NETHERRACK.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }
}
