//! Chiseled bookshelf block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use steel_registry::vanilla_item_tags::ItemTag;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::blocks::utils::selectable_slot_hit;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::{CHISELED_BOOKSHELF_SLOTS, ChiseledBookShelfBlockEntity};
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Vanilla `ChiseledBookShelfBlock.getRows`.
const BOOKSHELF_ROWS: usize = 2;
/// Vanilla `ChiseledBookShelfBlock.BOOKS_PER_ROW`.
const BOOKSHELF_COLUMNS: usize = 3;

/// Vanilla `ChiseledBookShelfBlock.SLOT_OCCUPIED_PROPERTIES`.
const SLOT_OCCUPIED: [BoolProperty; CHISELED_BOOKSHELF_SLOTS] = [
    BlockStateProperties::SLOT_0_OCCUPIED,
    BlockStateProperties::SLOT_1_OCCUPIED,
    BlockStateProperties::SLOT_2_OCCUPIED,
    BlockStateProperties::SLOT_3_OCCUPIED,
    BlockStateProperties::SLOT_4_OCCUPIED,
    BlockStateProperties::SLOT_5_OCCUPIED,
];

/// Vanilla `ChiseledBookShelfBlock` behavior.
#[block_behavior]
pub struct ChiseledBookShelfBlock {
    block: BlockRef,
}

impl ChiseledBookShelfBlock {
    /// Creates a new chiseled bookshelf behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Returns the slot the player clicked, if they clicked the shelf's front face.
    fn hit_slot(state: BlockStateId, hit_result: &BlockHitResult) -> Option<usize> {
        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        selectable_slot_hit(hit_result, facing, BOOKSHELF_ROWS, BOOKSHELF_COLUMNS)
    }

    fn set_slot_occupied(
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        slot: usize,
        occupied: bool,
    ) {
        world.set_block(
            pos,
            state.set_value(&SLOT_OCCUPIED[slot], occupied),
            UpdateFlags::UPDATE_ALL,
        );
    }

    fn with_block_entity<R>(
        world: &Arc<World>,
        pos: BlockPos,
        f: impl FnOnce(&ChiseledBookShelfBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        block_entity
            .downcast_ref::<ChiseledBookShelfBlockEntity>()
            .map(f)
    }
}

impl BlockBehavior for ChiseledBookShelfBlock {
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
        _player: &Player,
        _hand: InteractionHand,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !inv.with_item(|stack| stack.item().has_tag(&ItemTag::BOOKSHELF_BOOKS)) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let Some(slot) = Self::hit_slot(state, hit_result) else {
            return InteractionResult::Pass;
        };

        // An occupied slot falls through to the empty-hand path, which takes the book out.
        if state.get_value(&SLOT_OCCUPIED[slot]) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let book = inv.with_item(|stack| stack.split(1));
        if Self::with_block_entity(world, pos, |shelf| shelf.set_book(slot, book)).is_none() {
            return InteractionResult::Pass;
        }

        Self::set_slot_occupied(state, world, pos, slot, true);
        InteractionResult::Success
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(slot) = Self::hit_slot(state, hit_result) else {
            return InteractionResult::Pass;
        };

        if !state.get_value(&SLOT_OCCUPIED[slot]) {
            return InteractionResult::Consume;
        }

        let Some(book) = Self::with_block_entity(world, pos, |shelf| shelf.take_book(slot)) else {
            return InteractionResult::Pass;
        };

        if !book.is_empty() {
            player.add_item_or_drop(book);
        }

        Self::set_slot_occupied(state, world, pos, slot, false);
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(ChiseledBookShelfBlockEntity::new(
            level, pos, state,
        )))
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
        // Vanilla emits the last slot a player touched, plus one.
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<ChiseledBookShelfBlockEntity>()
                    .map(|shelf| shelf.last_interacted_slot() + 1)
            })
            .unwrap_or(0)
    }
}
