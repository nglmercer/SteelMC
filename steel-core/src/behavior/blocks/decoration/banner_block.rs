//! Standing and wall banner behaviors.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::data_components::vanilla_components::{BANNER_PATTERNS, CUSTOM_NAME};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_blocks;
use steel_utils::Downcast as _;
use steel_utils::axis::Axis;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::blocks::utils::convert_to_rotation_segment;
use crate::behavior::{BlockBehavior, BlockEntityCreation, BlockPlaceContext, PlacementSource};
use crate::block_entity::entities::BannerBlockEntity;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Copies the placed item's banner components onto the freshly created block entity.
///
/// Vanilla routes this through `BlockEntity.applyImplicitComponents`; Steel has no generic
/// item-to-block-entity component bridge yet, so the banner blocks apply their own two
/// components directly.
fn apply_banner_components(world: &Arc<World>, pos: BlockPos, stack: &ItemStack) {
    let Some(block_entity) = world.get_block_entity(pos) else {
        return;
    };
    let Some(banner) = block_entity.downcast_ref::<BannerBlockEntity>() else {
        return;
    };

    if let Some(patterns) = stack.get(BANNER_PATTERNS) {
        banner.set_patterns(patterns.clone());
    }
    banner.set_custom_name(stack.get(CUSTOM_NAME).cloned());
}

// Vanilla `AbstractBannerBlock.getCloneItemStack` returns the banner with its stored
// patterns. Steel's `BlockBehavior::get_clone_item_stack` receives no world or position,
// so pick-block currently yields a plain banner; giving block entities a say there is a
// trait-level change that affects every block.

/// Vanilla `BannerBlock` behavior (free-standing banners).
#[block_behavior]
pub struct BannerBlock {
    block: BlockRef,
}

impl BannerBlock {
    /// Creates a new standing banner behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for BannerBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state().set_value(
            &BlockStateProperties::ROTATION_16,
            convert_to_rotation_segment(context.rotation() + 180.0),
        ))
    }

    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        world.get_block_state(pos.below()).is_solid()
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
        if direction == Direction::Down && !self.can_survive(state, world, pos) {
            return vanilla_blocks::AIR.default_state();
        }

        state
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(BannerBlockEntity::new(level, pos, state)))
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        source.with_item(|stack| apply_banner_components(world, pos, stack));
    }
}

/// Vanilla `WallBannerBlock` behavior.
#[block_behavior]
pub struct WallBannerBlock {
    block: BlockRef,
}

impl WallBannerBlock {
    /// Creates a new wall banner behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for WallBannerBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis() == Axis::Y {
                continue;
            }

            let state = self.block.default_state().set_value(
                &BlockStateProperties::HORIZONTAL_FACING,
                direction.opposite(),
            );
            if self.can_survive(state, context.world, context.place_pos()) {
                return Some(state);
            }
        }

        None
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        world
            .get_block_state(pos.relative(facing.opposite()))
            .is_solid()
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
        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        if direction == facing.opposite() && !self.can_survive(state, world, pos) {
            return vanilla_blocks::AIR.default_state();
        }

        state
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(BannerBlockEntity::new(level, pos, state)))
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        source.with_item(|stack| apply_banner_components(world, pos, stack));
    }
}
