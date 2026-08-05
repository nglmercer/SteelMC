//! End rod block behavior.

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_utils::BlockStateId;

use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::entity::ai::path::PathComputationType;

/// Vanilla `EndRodBlock` behavior.
///
/// Vanilla's `RodBlock` base only contributes shape, rotation and pathfinding; the shapes
/// come from generated data, so the rod facing rules live here directly.
#[block_behavior]
pub struct EndRodBlock {
    block: BlockRef,
}

impl EndRodBlock {
    /// Creates a new end rod block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for EndRodBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let clicked_face = context.clicked_face();
        let behind = context
            .world
            .get_block_state(context.place_pos().relative(clicked_face.opposite()));

        // Placing an end rod against the tip of another one chains it in the same line.
        let facing = if behind.get_block() == self.block
            && behind.get_value(&BlockStateProperties::FACING) == clicked_face
        {
            clicked_face.opposite()
        } else {
            clicked_face
        };

        Some(
            self.block
                .default_state()
                .set_value(&BlockStateProperties::FACING, facing),
        )
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
