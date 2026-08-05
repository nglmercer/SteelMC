//! Shulker box block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, Direction};
use steel_registry::data_components::vanilla_components::CONTAINER;
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, RegistryExt as _};
use steel_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;

use steel_utils::Downcast as _;

use crate::behavior::block::{BlockBehavior, BlockEntityCreation, BlockLootContext};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{InventoryAccess, PlacementSource};
use crate::block_entity::entities::ShulkerBoxBlockEntity;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::chest;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Vanilla `ShulkerBoxBlock` behavior.
///
/// Vanilla also refuses to open the lid when a block would obstruct it and animates the
/// lid through the block entity; Steel does not model the lid collision volume yet, so
/// opening always succeeds.
#[block_behavior]
pub struct ShulkerBoxBlock {
    block: BlockRef,
}

impl ShulkerBoxBlock {
    /// Creates a new shulker box behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn with_block_entity<R>(
        world: &Arc<World>,
        pos: BlockPos,
        f: impl FnOnce(&ShulkerBoxBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        block_entity.downcast_ref::<ShulkerBoxBlockEntity>().map(f)
    }
}

impl BlockBehavior for ShulkerBoxBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(&BlockStateProperties::FACING, context.clicked_face()),
        )
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return InteractionResult::Pass;
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_SHULKER_BOX.msg()),
            move |context| chest(inventory, context.container_id, container_ref, 3),
        );

        InteractionResult::Success
    }

    fn get_drops(
        &self,
        state: BlockStateId,
        context: &BlockLootContext<'_>,
    ) -> Option<Vec<ItemStack>> {
        // Vanilla's shulker box loot table copies the container contents and custom name
        // onto the dropped item instead of scattering the items.
        let contents = Self::with_block_entity(
            context.world(),
            context.pos(),
            ShulkerBoxBlockEntity::contents_component,
        )?;
        let item = REGISTRY.items.by_key(&state.get_block().key)?;

        let mut stack = ItemStack::new(item);
        if let Some(contents) = contents {
            stack.set(CONTAINER, contents);
        }

        Some(vec![stack])
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(ShulkerBoxBlockEntity::new(level, pos, state)))
    }

    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        Self::with_block_entity(world, pos, |block_entity| {
            source.with_item(|stack| {
                if let Some(contents) = stack.get(CONTAINER) {
                    block_entity.set_contents_from_component(contents);
                }
            });
        });
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

    fn trigger_event(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        param_a: i32,
        param_b: i32,
    ) -> bool {
        if let Some(entity) = world.get_block_entity(pos) {
            return entity.trigger_event(param_a, param_b);
        }
        false
    }
}
