//! Beacon block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BlockEntityTicker;
use crate::block_entity::entities::BeaconBlockEntity;
use crate::player::Player;
use crate::world::World;

/// Vanilla `BeaconBlock` behavior.
///
/// Vanilla opens the beacon menu to choose its powers; Steel has no beacon menu yet, so a
/// beacon applies only the powers already stored in its block entity.
#[block_behavior]
pub struct BeaconBlock {
    block: BlockRef,
}

impl BeaconBlock {
    /// Creates a new beacon behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for BeaconBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn use_without_item(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos, player: &Player, _hit: &BlockHitResult, _inv: &mut InventoryAccess) -> InteractionResult {
        let inventory = player.inventory.clone();
        let world = Arc::clone(world);
        player.open_menu(TextComponent::translated(translations::CONTAINER_BEACON.msg()), move |ctx| crate::inventory::menu::kinds::beacon(inventory, ctx.container_id, pos, &world));
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(BeaconBlockEntity::new(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::BEACON,
        )
    }
}
