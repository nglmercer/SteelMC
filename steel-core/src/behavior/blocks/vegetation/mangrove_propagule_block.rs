use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, Direction};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_utils::{BlockPos, BlockStateId};

use std::sync::Arc;

use rand::Rng;
use steel_utils::types::UpdateFlags;

use crate::behavior::block::{BlockBehavior, schedule_water_tick_if_waterlogged};
use crate::behavior::blocks::vegetation::bonemealable::Bonemealable;
use crate::world::World;

/// Vanilla `MangrovePropaguleBlock.MAX_AGE`.
const MAX_AGE: u8 = 4;
use crate::behavior::context::BlockPlaceContext;
use crate::world::{LevelReader, ScheduledTickAccess};

use super::{BlockRef, default_surviving_state};

/// Vanilla `MangrovePropaguleBlock` survival.
///
/// - Hanging: block above must be in `SUPPORTS_HANGING_MANGROVE_PROPAGULE`.
/// - Planted: block below must be in `SUPPORTS_MANGROVE_PROPAGULE` (vanilla's
///   `mayPlaceOn` override applied to the `VegetationBlock` survival rule).
#[block_behavior]
pub struct MangrovePropaguleBlock {
    block: BlockRef,
}

impl MangrovePropaguleBlock {
    /// Creates a new mangrove propagule block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `MangrovePropaguleBlock.isHanging`.
    fn is_hanging(state: BlockStateId) -> bool {
        state.get_value(&BlockStateProperties::HANGING)
    }

    /// Vanilla `MangrovePropaguleBlock.isFullyGrown`.
    fn is_fully_grown(state: BlockStateId) -> bool {
        {
            let age: u8 = state.get_value(&BlockStateProperties::AGE_4);
            age >= MAX_AGE
        }
    }

    /// Advances a hanging propagule one age step.
    fn advance_age(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let age: u8 = state.get_value(&BlockStateProperties::AGE_4);
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::AGE_4, age + 1),
            UpdateFlags::UPDATE_CLIENTS,
        );
    }

    /// Creates vanilla's initial hanging propagule state.
    pub(crate) fn create_new_hanging_propagule() -> BlockStateId {
        vanilla_blocks::MANGROVE_PROPAGULE
            .default_state()
            .set_value(&BlockStateProperties::HANGING, true)
            .set_value(&BlockStateProperties::AGE_4, 0)
    }
}

impl BlockBehavior for MangrovePropaguleBlock {
    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);
        if self.can_survive(state, world, pos) {
            state
        } else {
            vanilla_blocks::AIR.default_state()
        }
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        if state.get_value(&BlockStateProperties::HANGING) {
            let above = world.get_block_state(pos.above());
            return above
                .get_block()
                .has_tag(&BlockTag::SUPPORTS_HANGING_MANGROVE_PROPAGULE);
        }

        let below = world.get_block_state(pos.below());
        below
            .get_block()
            .has_tag(&BlockTag::SUPPORTS_MANGROVE_PROPAGULE)
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        default_surviving_state(self.block, self, context)
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        // Vanilla `MangrovePropaguleBlock.randomTick`: a hanging propagule ripens by one age
        // step; a planted one has a 1-in-7 chance to grow into a tree.
        if Self::is_hanging(state) {
            if !Self::is_fully_grown(state) {
                Self::advance_age(state, world, pos);
            }
            return;
        }
        // DEFERRED (Phase 4-8): planted propagules call vanilla's `advanceTree`, which needs
        // the `TreeGrower` + runtime configured-feature placement that saplings also wait on.
    }
}

impl Bonemealable for MangrovePropaguleBlock {
    fn is_valid_bonemeal_target(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
    ) -> bool {
        !Self::is_hanging(state) || !Self::is_fully_grown(state)
    }

    fn is_bonemeal_success(
        &self,
        state: BlockStateId,
        _world: &Arc<World>,
        _rng: &mut dyn Rng,
        _pos: BlockPos,
    ) -> bool {
        // Vanilla only guarantees success for the hanging (ripening) case.
        !Self::is_hanging(state) || !Self::is_fully_grown(state)
    }

    fn perform_bonemeal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        _rng: &mut dyn Rng,
        pos: BlockPos,
    ) {
        if Self::is_hanging(state) && !Self::is_fully_grown(state) {
            Self::advance_age(state, world, pos);
        }
        // The planted branch grows a tree; see `random_tick`.
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::test_support::init_test_registry;

    use super::*;
    use crate::test_support::TestLevel;

    #[test]
    fn new_hanging_propagule_starts_at_age_zero() {
        init_test_registry();

        let state = MangrovePropaguleBlock::create_new_hanging_propagule();

        assert_eq!(state.get_block(), &vanilla_blocks::MANGROVE_PROPAGULE);
        assert!(state.get_value(&BlockStateProperties::HANGING));
        assert_eq!(state.get_value(&BlockStateProperties::AGE_4), 0);
    }

    #[test]
    fn unsupported_waterlogged_propagule_schedules_water_before_breaking() {
        init_test_registry();
        let behavior = MangrovePropaguleBlock::new(&vanilla_blocks::MANGROVE_PROPAGULE);
        let state = vanilla_blocks::MANGROVE_PROPAGULE
            .default_state()
            .set_value(&BlockStateProperties::WATERLOGGED, true);
        let level = TestLevel::default();

        assert!(
            behavior
                .update_shape(
                    state,
                    &level,
                    BlockPos::ZERO,
                    Direction::Down,
                    BlockPos::ZERO.below(),
                    vanilla_blocks::AIR.default_state(),
                )
                .is_air()
        );
        assert!(level.scheduled_water_tick());
    }
}
