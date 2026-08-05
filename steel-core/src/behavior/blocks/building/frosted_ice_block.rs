//! Frosted ice block behavior (the trail left by Frost Walker boots).

use std::sync::Arc;

use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, IntProperty};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_dimension_types;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::blocks::IceBlock;
use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::chunk::light::LightLayer;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// How far the ice has melted; it turns to water past [`MAX_AGE`].
const AGE: IntProperty = BlockStateProperties::AGE_3;
/// Maximum melt age before the block disappears.
const MAX_AGE: u8 = 3;
/// Below this many frosted-ice neighbors, melting is forced on the scheduled tick.
const NEIGHBORS_TO_AGE: usize = 4;
/// Below this many frosted-ice neighbors, a neighbor change melts the block outright.
const NEIGHBORS_TO_MELT: usize = 2;
/// Base light level used for the melt threshold.
const BASE_MELT_LIGHT_LEVEL: u8 = 11;

/// Vanilla `FrostedIceBlock` behavior.
#[block_behavior]
pub struct FrostedIceBlock {
    block: BlockRef,
}

impl FrostedIceBlock {
    /// Creates a new frosted ice block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Advances the melt age, or melts the block once it is fully aged.
    ///
    /// Returns whether the block melted away.
    fn slightly_melt(state: BlockStateId, world: &Arc<World>, pos: BlockPos) -> bool {
        let age: u8 = state.get_value(&AGE);
        if age < MAX_AGE {
            world.set_block(
                pos,
                state.set_value(&AGE, age + 1),
                UpdateFlags::UPDATE_CLIENTS,
            );
            return false;
        }

        IceBlock::melt(state, world, pos);
        true
    }

    /// Vanilla `FrostedIceBlock.fewerNeigboursThan`.
    fn fewer_neighbors_than(&self, world: &dyn LevelReader, pos: BlockPos, limit: usize) -> bool {
        let mut found = 0;
        for direction in Direction::ALL {
            if world.get_block_state(pos.relative(direction)).get_block() == self.block {
                found += 1;
                if found >= limit {
                    return false;
                }
            }
        }

        true
    }

    /// Vanilla's melt light check, which uses block light in the End and local brightness
    /// elsewhere because the End has no meaningful sky light.
    fn brightness_for_melting(state: BlockStateId, world: &Arc<World>, pos: BlockPos) -> bool {
        let brightness = if world.dimension_type == &vanilla_dimension_types::THE_END {
            world.light_value_at(LightLayer::Block, pos)
        } else {
            world.max_local_raw_brightness(pos, 0)
        };

        let age: u8 = state.get_value(&AGE);
        let threshold = BASE_MELT_LIGHT_LEVEL
            .saturating_sub(age)
            .saturating_sub(state.get_light_dampening());

        brightness > threshold
    }
}

impl BlockBehavior for FrostedIceBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, rand::rng().random_range(60..=120));
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let forced = rand::rng().random_range(0..3) == 0
            || self.fewer_neighbors_than(world.as_ref(), pos, NEIGHBORS_TO_AGE);

        if forced
            && Self::brightness_for_melting(state, world, pos)
            && Self::slightly_melt(state, world, pos)
        {
            // Melting this block nudges its remaining neighbors along too.
            for direction in Direction::ALL {
                let neighbor_pos = pos.relative(direction);
                let neighbor = world.get_block_state(neighbor_pos);
                if neighbor.get_block() == self.block
                    && !Self::slightly_melt(neighbor, world, neighbor_pos)
                {
                    world.schedule_block_tick_default(
                        neighbor_pos,
                        self.block,
                        rand::rng().random_range(20..=40),
                    );
                }
            }

            return;
        }

        world.schedule_block_tick_default(pos, self.block, rand::rng().random_range(20..=40));
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        if source_block == self.block
            && self.fewer_neighbors_than(world.as_ref(), pos, NEIGHBORS_TO_MELT)
        {
            IceBlock::melt(state, world, pos);
        }
    }

    fn get_clone_item_stack(
        &self,
        _block: BlockRef,
        _state: BlockStateId,
        _include_data: bool,
    ) -> Option<ItemStack> {
        // Vanilla returns an empty stack: frosted ice has no item form.
        None
    }
}
