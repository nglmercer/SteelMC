//! Standing and wall skull/head behaviors.
//!
//! Vanilla's `WitherSkullBlock`, `PlayerHeadBlock` and their wall variants add wither
//! summoning and profile resolution on top of these classes and are not registered here.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::data_components::vanilla_components::{CUSTOM_NAME, NOTE_BLOCK_SOUND, PROFILE};
use steel_registry::item_stack::ItemStack;
use steel_utils::axis::Axis;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::blocks::utils::convert_to_rotation_segment;
use crate::behavior::{
    BLOCK_BEHAVIORS, BlockBehavior, BlockEntityCreation, BlockPlaceContext, PlacementSource,
};
use crate::block_entity::entities::SkullBlockEntity;
use crate::entity::ai::path::PathComputationType;
use crate::world::{SignalGetter as _, World};

/// Copies the placed item's skull components onto the freshly created block entity.
///
/// Vanilla routes this through `BlockEntity.applyImplicitComponents`; Steel has no generic
/// item-to-block-entity component bridge yet.
fn apply_skull_components(world: &Arc<World>, pos: BlockPos, stack: &ItemStack) {
    let Some(block_entity) = world.get_block_entity(pos) else {
        return;
    };
    let Some(skull) = block_entity.downcast_ref::<SkullBlockEntity>() else {
        return;
    };

    skull.apply_item_components(
        stack.get(PROFILE).cloned(),
        stack.get(NOTE_BLOCK_SOUND).cloned(),
        stack.get(CUSTOM_NAME).cloned(),
    );
}

/// Vanilla `AbstractSkullBlock.getStateForPlacement`: skulls track redstone power so the
/// client can animate dragon and piglin heads.
fn powered_placement_state(block: BlockRef, context: &BlockPlaceContext<'_>) -> BlockStateId {
    block.default_state().set_value(
        &BlockStateProperties::POWERED,
        context.world.has_neighbor_signal(context.place_pos()),
    )
}

/// Vanilla `AbstractSkullBlock.neighborChanged`.
fn update_powered(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
    let signal = world.has_neighbor_signal(pos);
    if signal != state.get_value(&BlockStateProperties::POWERED) {
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::POWERED, signal),
            UpdateFlags::UPDATE_CLIENTS,
        );
    }
}

fn new_skull_block_entity(
    level: Weak<World>,
    pos: BlockPos,
    state: BlockStateId,
) -> BlockEntityCreation {
    BlockEntityCreation::Created(Arc::new(SkullBlockEntity::new(level, pos, state)))
}

/// Vanilla `SkullBlock` behavior (skulls placed on top of a block).
#[block_behavior]
pub struct SkullBlock {
    block: BlockRef,
}

impl SkullBlock {
    /// Creates a new standing skull behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for SkullBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(powered_placement_state(self.block, context).set_value(
            &BlockStateProperties::ROTATION_16,
            convert_to_rotation_segment(context.rotation()),
        ))
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        update_powered(state, world, pos);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_skull_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        source.with_item(|stack| apply_skull_components(world, pos, stack));
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

/// Vanilla `WallSkullBlock` behavior.
#[block_behavior]
pub struct WallSkullBlock {
    block: BlockRef,
}

impl WallSkullBlock {
    /// Creates a new wall skull behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn placement_state(block: BlockRef, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let base_state = powered_placement_state(block, context);

        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis() == Axis::Y {
                continue;
            }

            let state = base_state.set_value(
                &BlockStateProperties::HORIZONTAL_FACING,
                direction.opposite(),
            );

            // Vanilla requires the block the skull hangs on to not be replaceable.
            let support_pos = context.place_pos().relative(direction);
            let support_state = context.world.get_block_state(support_pos);
            if !BLOCK_BEHAVIORS
                .get_behavior_for_state(support_state)
                .is_some_and(|behavior| behavior.can_be_replaced(support_state, context))
            {
                return Some(state);
            }
        }

        None
    }
}

impl BlockBehavior for WallSkullBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Self::placement_state(self.block, context)
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        update_powered(state, world, pos);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_skull_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        source.with_item(|stack| apply_skull_components(world, pos, stack));
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

/// Vanilla `PiglinWallSkullBlock` behavior.
///
/// Identical to [`WallSkullBlock`] server-side; vanilla only overrides the collision shape,
/// which comes from generated data.
#[block_behavior]
pub struct PiglinWallSkullBlock {
    wall_skull: WallSkullBlock,
}

impl PiglinWallSkullBlock {
    /// Creates a new piglin wall skull behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            wall_skull: WallSkullBlock::new(block),
        }
    }
}

impl BlockBehavior for PiglinWallSkullBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.wall_skull.get_state_for_placement(context)
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source_block: BlockRef,
        moved_by_piston: bool,
    ) {
        self.wall_skull
            .handle_neighbor_changed(state, world, pos, source_block, moved_by_piston);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        new_skull_block_entity(level, pos, state)
    }

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        self.wall_skull.set_placed_by(state, world, pos, source);
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
