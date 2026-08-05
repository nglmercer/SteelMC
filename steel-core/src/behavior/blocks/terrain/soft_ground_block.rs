//! Mud and soul sand: full blocks with a sunken collision shape that mobs cannot path over.
//!
//! Vanilla's other overrides on these classes (`getShadeBrightness`, `getVisualShape`,
//! `getBlockSupportShape`) are rendering-only or already covered by the generated shape
//! data, so only pathfinding is handled here.

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_utils::BlockStateId;

use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::entity::ai::path::PathComputationType;

/// Vanilla `MudBlock` behavior.
#[block_behavior]
pub struct MudBlock {
    block: BlockRef,
}

impl MudBlock {
    /// Creates a new mud block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for MudBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

/// Vanilla `SoulSandBlock` behavior.
#[block_behavior]
pub struct SoulSandBlock {
    block: BlockRef,
}

impl SoulSandBlock {
    /// Creates a new soul sand block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for SoulSandBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
