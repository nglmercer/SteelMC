//! Shelf block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, SideChainPart};
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::blocks::utils::selectable_slot_hit;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::{SHELF_SLOTS, ShelfBlockEntity};
use crate::entity::ai::path::PathComputationType;
use crate::player::Player;
use crate::world::{ScheduledTickAccess, SignalGetter as _, World};

/// Vanilla `ShelfBlock.getRows`.
const SHELF_ROWS: usize = 1;
/// Vanilla `ShelfBlock.getColumns`.
const SHELF_COLUMNS: usize = SHELF_SLOTS;

/// Vanilla `ShelfBlock` behavior.
///
/// The powered "swap the whole hotbar across connected shelves" interaction needs
/// vanilla's `SideChainPartBlock` chaining, which Steel does not model yet; a powered
/// shelf therefore only refuses the single-slot swap, and `SIDE_CHAIN_PART` stays
/// `unconnected`.
#[block_behavior]
pub struct ShelfBlock {
    block: BlockRef,
}

impl ShelfBlock {
    /// Creates a new shelf behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `ShelfBlock.swapSingleItem`.
    ///
    /// Returns whether an item was taken off the shelf.
    fn swap_single_item(
        shelf: &ShelfBlockEntity,
        slot: usize,
        player: &Player,
        inv: &mut InventoryAccess,
    ) -> bool {
        let held = inv.with_item(|stack| stack.clone());
        let removed = shelf.swap_item(slot, held.clone());

        // In creative, taking from an empty slot leaves the held stack untouched.
        let new_held = if player.has_infinite_materials() && removed.is_empty() {
            held
        } else {
            removed.clone()
        };
        inv.with_item(|stack| *stack = new_held);

        !removed.is_empty()
    }
}

impl BlockBehavior for ShelfBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let pos = context.place_pos();
        Some(
            self.block
                .default_state()
                .set_value(
                    &BlockStateProperties::HORIZONTAL_FACING,
                    context.horizontal_direction().opposite(),
                )
                .set_value(
                    &BlockStateProperties::POWERED,
                    context.world.has_neighbor_signal(pos),
                )
                .set_value(
                    &BlockStateProperties::WATERLOGGED,
                    context.is_water_source(),
                ),
        )
    }

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
        state
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let signal = world.has_neighbor_signal(pos);
        if state.get_value(&BlockStateProperties::POWERED) == signal {
            return;
        }

        let mut new_state = state.set_value(&BlockStateProperties::POWERED, signal);
        if !signal {
            new_state = new_state.set_value(
                &BlockStateProperties::SIDE_CHAIN_PART,
                SideChainPart::Unconnected,
            );
        }

        world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hand: InteractionHand,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if hand == InteractionHand::OffHand {
            return InteractionResult::Pass;
        }

        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(shelf_entity) = block_entity.downcast_ref::<ShelfBlockEntity>() else {
            return InteractionResult::Pass;
        };

        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        let Some(slot) = selectable_slot_hit(hit_result, facing, SHELF_ROWS, SHELF_COLUMNS) else {
            return InteractionResult::Pass;
        };

        if state.get_value(&BlockStateProperties::POWERED) {
            // Vanilla swaps the whole hotbar across the connected shelf chain here.
            return InteractionResult::Consume;
        }

        let held_is_empty = inv.with_item(|stack| stack.is_empty());
        if !Self::swap_single_item(shelf_entity, slot, player, inv) && held_is_empty {
            return InteractionResult::Pass;
        }

        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(ShelfBlockEntity::new(level, pos, state)))
    }

    fn is_pathfindable(&self, state: BlockStateId, computation_type: PathComputationType) -> bool {
        computation_type == PathComputationType::Water
            && state.get_fluid_state().fluid_id.has_tag(&FluidTag::WATER)
    }
}
