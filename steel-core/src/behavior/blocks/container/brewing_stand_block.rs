//! Brewing stand block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, Direction, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Vanilla `BrewingStandBlock` — opens the brewing menu and provides comparator output.
#[block_behavior]
pub struct BrewingStandBlock { block: BlockRef }

impl BrewingStandBlock {
    /// Creates a brewing stand block behavior.
    #[must_use] pub const fn new(block: BlockRef) -> Self { Self { block } }
}

impl BlockBehavior for BrewingStandBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }
    fn use_without_item(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos, player: &Player, _hit: &BlockHitResult, _inv: &mut InventoryAccess) -> InteractionResult {
        let Some(be) = world.get_block_entity(pos) else { return InteractionResult::Pass };
        let Some(container_ref) = ContainerRef::from_block_entity(be) else { return InteractionResult::Pass };
        let inventory = player.inventory.clone();
        player.open_menu(TextComponent::translated(translations::CONTAINER_BREWING.msg()), move |ctx| crate::inventory::menu::kinds::brewing_stand(inventory, ctx.container_id, container_ref));
        InteractionResult::Success
    }
    fn new_block_entity(&self, level: Weak<World>, pos: BlockPos, state: BlockStateId) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(&vanilla_block_entity_types::BREWING_STAND, level, pos, state))
    }
    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool { true }
    fn get_analog_output_signal(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos, _dir: Direction) -> i32 {
        let Some(container_ref) = world.get_block_entity(pos).and_then(ContainerRef::from_block_entity) else { return 0; };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard.get(container_ref.container_id()).map_or(0, |c| calculate_redstone_signal_from_container(c))
    }
}
