use steel_macros::block_behavior;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::world::LevelReader;

use super::{BlockRef, default_surviving_state, survives_on_tag};

/// Vanilla `SaplingBlock` survival.
// DEFERRED (Phase 4-8): Growth needs two foundations Steel does not have yet: a `TreeGrower`
// mapping each sapling to its configured tree feature(s), and the ability to place a
// configured feature at runtime (outside the worldgen pipeline). `random_tick` and the
// `Bonemealable` trait already exist, so only those two are missing.
#[block_behavior]
pub struct SaplingBlock {
    block: BlockRef,
}

impl SaplingBlock {
    /// Creates a new sapling block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for SaplingBlock {
    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        survives_on_tag(world, pos, &BlockTag::SUPPORTS_VEGETATION)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        default_surviving_state(self.block, self, context)
    }
}
