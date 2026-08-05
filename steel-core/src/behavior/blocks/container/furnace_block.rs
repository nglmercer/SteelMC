//! Furnace, smoker and blast furnace block behaviors.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_block_entity_types;
use steel_utils::locks::Shared;
use steel_utils::{BlockPos, BlockStateId, Direction, translations};
use text_components::TextComponent;

use crate::player::player_inventory::PlayerInventory;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::player::Player;
use crate::world::{LevelReader, World};

fn furnace_placement(block: BlockRef, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
    let facing = context.horizontal_direction().opposite();
    Some(block.default_state().set_value(&BlockStateProperties::HORIZONTAL_FACING, facing).set_value(&BlockStateProperties::LIT, false))
}

fn furnace_use(
    world: &Arc<World>,
    pos: BlockPos,
    player: &Player,
    menu_fn: fn(Shared<PlayerInventory>, u8, ContainerRef) -> crate::inventory::menu::Menu,
    title: TextComponent,
) -> InteractionResult {
    let Some(be) = world.get_block_entity(pos) else { return InteractionResult::Pass };
    let Some(container_ref) = ContainerRef::from_block_entity(be) else { return InteractionResult::Pass };
    let inventory = player.inventory.clone();
    player.open_menu(title, move |ctx| menu_fn(inventory, ctx.container_id, container_ref));
    InteractionResult::Success
}

/// Vanilla `FurnaceBlock` — opens the furnace menu and manages the lit state.
#[block_behavior]
pub struct FurnaceBlock { block: BlockRef }

impl FurnaceBlock {
    /// Creates a furnace block behavior.
    #[must_use] pub const fn new(block: BlockRef) -> Self { Self { block } }
}

impl BlockBehavior for FurnaceBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        furnace_placement(self.block, context)
    }
    fn use_without_item(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos, player: &Player, _hit: &BlockHitResult, _inv: &mut InventoryAccess) -> InteractionResult {
        furnace_use(world, pos, player, crate::inventory::menu::kinds::furnace, TextComponent::translated(translations::CONTAINER_FURNACE.msg()))
    }
    fn new_block_entity(&self, level: Weak<World>, pos: BlockPos, state: BlockStateId) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(&vanilla_block_entity_types::FURNACE, level, pos, state))
    }
    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool { true }
    fn get_analog_output_signal(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos, _dir: Direction) -> i32 {
        let Some(container_ref) = world.get_block_entity(pos).and_then(ContainerRef::from_block_entity) else { return 0; };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard.get(container_ref.container_id()).map_or(0, |c| calculate_redstone_signal_from_container(c))
    }
    fn get_block_entity_ticker(&self, _world: &Arc<World>, _state: BlockStateId, block_entity_type: BlockEntityTypeRef) -> Option<crate::block_entity::BlockEntityTicker> {
        crate::block_entity::BlockEntityTicker::for_matching_entity_tick(block_entity_type, &vanilla_block_entity_types::FURNACE)
    }
}

/// Vanilla `SmokerBlock` — cooks food twice as fast as a furnace.
#[block_behavior]
pub struct SmokerBlock { block: BlockRef }

impl SmokerBlock {
    /// Creates a smoker block behavior.
    #[must_use] pub const fn new(block: BlockRef) -> Self { Self { block } }
}

impl BlockBehavior for SmokerBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> { furnace_placement(self.block, context) }
    fn use_without_item(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos, player: &Player, _hit: &BlockHitResult, _inv: &mut InventoryAccess) -> InteractionResult {
        furnace_use(world, pos, player, crate::inventory::menu::kinds::smoker, TextComponent::translated(translations::CONTAINER_SMOKER.msg()))
    }
    fn new_block_entity(&self, level: Weak<World>, pos: BlockPos, state: BlockStateId) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(&vanilla_block_entity_types::SMOKER, level, pos, state))
    }
    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool { true }
    fn get_analog_output_signal(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos, _dir: Direction) -> i32 {
        let Some(container_ref) = world.get_block_entity(pos).and_then(ContainerRef::from_block_entity) else { return 0; };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard.get(container_ref.container_id()).map_or(0, |c| calculate_redstone_signal_from_container(c))
    }
    fn get_block_entity_ticker(&self, _world: &Arc<World>, _state: BlockStateId, block_entity_type: BlockEntityTypeRef) -> Option<crate::block_entity::BlockEntityTicker> {
        crate::block_entity::BlockEntityTicker::for_matching_entity_tick(block_entity_type, &vanilla_block_entity_types::SMOKER)
    }
}

/// Vanilla `BlastFurnaceBlock` — smelts ores and ingots twice as fast as a furnace.
#[block_behavior]
pub struct BlastFurnaceBlock { block: BlockRef }

impl BlastFurnaceBlock {
    /// Creates a blast furnace block behavior.
    #[must_use] pub const fn new(block: BlockRef) -> Self { Self { block } }
}

impl BlockBehavior for BlastFurnaceBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> { furnace_placement(self.block, context) }
    fn use_without_item(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos, player: &Player, _hit: &BlockHitResult, _inv: &mut InventoryAccess) -> InteractionResult {
        furnace_use(world, pos, player, crate::inventory::menu::kinds::blast_furnace, TextComponent::translated(translations::CONTAINER_BLAST_FURNACE.msg()))
    }
    fn new_block_entity(&self, level: Weak<World>, pos: BlockPos, state: BlockStateId) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(&vanilla_block_entity_types::BLAST_FURNACE, level, pos, state))
    }
    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool { true }
    fn get_analog_output_signal(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos, _dir: Direction) -> i32 {
        let Some(container_ref) = world.get_block_entity(pos).and_then(ContainerRef::from_block_entity) else { return 0; };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard.get(container_ref.container_id()).map_or(0, |c| calculate_redstone_signal_from_container(c))
    }
    fn get_block_entity_ticker(&self, _world: &Arc<World>, _state: BlockStateId, block_entity_type: BlockEntityTypeRef) -> Option<crate::block_entity::BlockEntityTicker> {
        crate::block_entity::BlockEntityTicker::for_matching_entity_tick(block_entity_type, &vanilla_block_entity_types::BLAST_FURNACE)
    }
}
