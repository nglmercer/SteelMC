//! Enchanting table block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{REGISTRY, TaggedRegistryExt as _};
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::EnchantingTableBlockEntity;
use crate::entity::ai::path::PathComputationType;
use crate::inventory::menu::kinds::enchantment;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Vanilla `EnchantingTableBlock.BOOKSHELF_OFFSETS`: the ring of positions around a table
/// that can supply enchanting power, on both the table's level and the one above.
fn bookshelf_offsets() -> impl Iterator<Item = BlockPos> {
    (-2..=2_i32).flat_map(|x| {
        (0..=1_i32).flat_map(move |y| {
            // Only the outer ring of the 5×5 counts.
            (-2..=2_i32)
                .filter(move |z| x.abs() == 2 || z.abs() == 2)
                .map(move |z| BlockPos::new(x, y, z))
        })
    })
}

/// Vanilla `EnchantingTableBlock.isValidBookShelf`: a power provider at `offset` counts only
/// while the block halfway towards the table transmits it.
#[must_use]
pub fn is_valid_enchanting_bookshelf(
    world: &dyn LevelReader,
    pos: BlockPos,
    offset: BlockPos,
) -> bool {
    let provider = world.get_block_state(pos.offset(offset.x(), offset.y(), offset.z()));
    if !REGISTRY
        .blocks
        .is_in_tag(provider.get_block(), &BlockTag::ENCHANTMENT_POWER_PROVIDER)
    {
        return false;
    }

    let transmitter = world.get_block_state(pos.offset(offset.x() / 2, offset.y(), offset.z() / 2));
    REGISTRY.blocks.is_in_tag(
        transmitter.get_block(),
        &BlockTag::ENCHANTMENT_POWER_TRANSMITTER,
    )
}

/// Counts the bookshelves powering the enchanting table at `pos`.
#[must_use]
pub fn valid_enchanting_bookshelf_count(world: &dyn LevelReader, pos: BlockPos) -> i32 {
    bookshelf_offsets()
        .filter(|offset| is_valid_enchanting_bookshelf(world, pos, *offset))
        .count()
        .try_into()
        .unwrap_or(i32::MAX)
}

/// Vanilla `EnchantingTableBlock` behavior.
#[block_behavior]
pub struct EnchantingTableBlock {
    block: BlockRef,
}

impl EnchantingTableBlock {
    /// Creates a new enchanting table behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for EnchantingTableBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(EnchantingTableBlockEntity::new(level, pos, state)))
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
        let inventory = player.inventory.clone();
        let world = Arc::clone(world);
        let seed = player.enchantment_seed();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_ENCHANT.msg()),
            move |context| enchantment(inventory, context.container_id, pos, &world, seed),
        );
        InteractionResult::Success
    }

    fn use_item_on(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        _player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        InteractionResult::TryEmptyHandInteraction
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
