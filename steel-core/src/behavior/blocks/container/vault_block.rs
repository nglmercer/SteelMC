//! Vault block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, VaultState};
use steel_registry::vanilla_block_entity_types;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BlockEntityTicker;
use crate::block_entity::entities::VaultBlockEntity;
use crate::player::Player;
use crate::world::World;

/// Vanilla `VaultBlock` behavior.
#[block_behavior]
pub struct VaultBlock {
    block: BlockRef,
}

impl VaultBlock {
    /// Creates a new vault behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for VaultBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state().set_value(
            &BlockStateProperties::HORIZONTAL_FACING,
            context.horizontal_direction().opposite(),
        ))
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        // Only an active vault takes a key, and only from a non-empty hand.
        if state.get_value(&BlockStateProperties::VAULT_STATE) != VaultState::Active {
            return InteractionResult::TryEmptyHandInteraction;
        }
        if inv.with_item(|held| held.is_empty()) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::TryEmptyHandInteraction;
        };
        let Some(vault) = block_entity.downcast_ref::<VaultBlockEntity>() else {
            return InteractionResult::TryEmptyHandInteraction;
        };

        inv.with_item(|held| vault.try_insert_key(world, state, player, held));
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(VaultBlockEntity::new(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::VAULT,
        )
    }
}
