//! Huge mushroom block behavior (mushroom blocks and mushroom stem).

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::world::ScheduledTickAccess;

/// Vanilla `HugeMushroomBlock` behavior.
///
/// Each face property is set when the neighbor on that side is *not* the same block, so
/// the mushroom's outer skin only renders on exposed faces.
#[block_behavior]
pub struct HugeMushroomBlock {
    block: BlockRef,
}

impl HugeMushroomBlock {
    /// Creates a new huge mushroom block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `PipeBlock.PROPERTY_BY_DIRECTION`.
    const fn property_for(direction: Direction) -> BoolProperty {
        match direction {
            Direction::Down => BlockStateProperties::DOWN,
            Direction::Up => BlockStateProperties::UP,
            Direction::North => BlockStateProperties::NORTH,
            Direction::South => BlockStateProperties::SOUTH,
            Direction::West => BlockStateProperties::WEST,
            Direction::East => BlockStateProperties::EAST,
        }
    }
}

impl BlockBehavior for HugeMushroomBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let pos = context.place_pos();
        let mut state = self.block.default_state();

        for direction in Direction::ALL {
            let neighbor = context.world.get_block_state(pos.relative(direction));
            state = state.set_value(
                &Self::property_for(direction),
                neighbor.get_block() != self.block,
            );
        }

        Some(state)
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        _world: &dyn ScheduledTickAccess,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if neighbor_state.get_block() == self.block {
            state.set_value(&Self::property_for(direction), false)
        } else {
            state
        }
    }
}
