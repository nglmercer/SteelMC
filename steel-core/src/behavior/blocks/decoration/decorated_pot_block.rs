//! Decorated pot block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::data_components::vanilla_components::POT_DECORATIONS;
use steel_registry::item_stack::ItemStack;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::PlacementSource;
use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::DecoratedPotBlockEntity;
use crate::entity::ai::path::PathComputationType;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Vanilla `DecoratedPotBlock` behavior.
///
/// Vanilla also cracks the pot when a projectile hits it and wobbles it on interaction;
/// the wobble is a client-side block event and cracking needs projectile hit routing.
#[block_behavior]
pub struct DecoratedPotBlock {
    block: BlockRef,
}

impl DecoratedPotBlock {
    /// Creates a new decorated pot behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn with_block_entity<R>(
        world: &Arc<World>,
        pos: BlockPos,
        f: impl FnOnce(&DecoratedPotBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        block_entity
            .downcast_ref::<DecoratedPotBlockEntity>()
            .map(f)
    }
}

impl BlockBehavior for DecoratedPotBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                // Vanilla faces the pot toward the player, not away like most blocks.
                .set_value(
                    &BlockStateProperties::HORIZONTAL_FACING,
                    context.horizontal_direction(),
                )
                .set_value(
                    &BlockStateProperties::WATERLOGGED,
                    context.is_water_source(),
                )
                .set_value(&BlockStateProperties::CRACKED, false),
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

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        source.with_item(|stack| {
            if let Some(decorations) = stack.get(POT_DECORATIONS) {
                Self::with_block_entity(world, pos, |pot| {
                    pot.set_decorations(decorations.clone());
                });
            }
        });
    }

    fn use_item_on(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(stored) = Self::with_block_entity(world, pos, DecoratedPotBlockEntity::the_item)
        else {
            return InteractionResult::Pass;
        };

        // Vanilla accepts one item at a time, and only when the pot is empty or already
        // holds a matching, non-full stack.
        let accepted = inv.with_item(|held| {
            if held.is_empty() {
                return None;
            }
            let matches = stored.is_empty()
                || (ItemStack::is_same_item_same_components(&stored, held)
                    && stored.count() < stored.max_stack_size());
            matches.then(|| held.split(1))
        });

        let Some(accepted) = accepted else {
            return InteractionResult::TryEmptyHandInteraction;
        };

        Self::with_block_entity(world, pos, |pot| {
            let mut item = pot.the_item();
            if item.is_empty() {
                pot.set_the_item(accepted);
            } else {
                item.set_count(item.count() + 1);
                pot.set_the_item(item);
            }
        });

        InteractionResult::Success
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        _player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        // Vanilla only plays the failure sound and wobbles the pot here.
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(DecoratedPotBlockEntity::new(level, pos, state)))
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, |container| {
                calculate_redstone_signal_from_container(container)
            })
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
