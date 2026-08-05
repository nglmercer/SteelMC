//! Lectern block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::LecternBlockEntity;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess as _, SignalQueryContext, World};

/// Vanilla `LecternBlock.PAGE_CHANGE_IMPULSE_TICKS`: how long the page-turn pulse lasts.
const PAGE_CHANGE_IMPULSE_TICKS: i32 = 2;
/// Redstone strength of the page-turn pulse.
const PULSE_SIGNAL: i32 = 15;

/// Vanilla `LecternBlock` behavior.
///
/// Reading a placed book opens vanilla's lectern menu, which Steel does not have yet, so
/// right-clicking a lectern that holds a book currently does nothing. Placing and taking
/// books, the page-turn redstone pulse and the comparator output all work.
#[block_behavior]
pub struct LecternBlock {
    block: BlockRef,
}

impl LecternBlock {
    /// Creates a new lectern behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn with_block_entity<R>(
        world: &Arc<World>,
        pos: BlockPos,
        f: impl FnOnce(&LecternBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        block_entity.downcast_ref::<LecternBlockEntity>().map(f)
    }

    /// Vanilla `LecternBlock.resetBookState`.
    fn reset_book_state(state: BlockStateId, world: &Arc<World>, pos: BlockPos, has_book: bool) {
        world.set_block(
            pos,
            state
                .set_value(&BlockStateProperties::POWERED, false)
                .set_value(&BlockStateProperties::HAS_BOOK, has_book),
            UpdateFlags::UPDATE_ALL,
        );
        Self::update_below(state, world, pos);
    }

    /// Vanilla `LecternBlock.updateBelow`: the lectern powers the block underneath it.
    fn update_below(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        world.update_neighbors_at(pos.below(), state.get_block());
    }
}

impl BlockBehavior for LecternBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(
                    &BlockStateProperties::HORIZONTAL_FACING,
                    context.horizontal_direction().opposite(),
                )
                .set_value(&BlockStateProperties::HAS_BOOK, false),
        )
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if state.get_value(&BlockStateProperties::HAS_BOOK) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let is_book = inv.with_item(|stack| stack.item().has_tag(&ItemTag::LECTERN_BOOKS));
        if !is_book {
            let empty_main_hand =
                inv.with_item(|stack| stack.is_empty()) && hand == InteractionHand::MainHand;
            return if empty_main_hand {
                InteractionResult::Pass
            } else {
                InteractionResult::TryEmptyHandInteraction
            };
        }

        let book = inv.with_item(|stack| stack.split(1));
        if Self::with_block_entity(world, pos, |lectern| lectern.set_book(book)).is_none() {
            return InteractionResult::Pass;
        }

        Self::reset_book_state(state, world, pos, true);
        InteractionResult::Success
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !state.get_value(&BlockStateProperties::HAS_BOOK) {
            return InteractionResult::Pass;
        }

        // Vanilla opens the reading menu for non-owners; without that menu the only
        // faithful action left is handing the book back.
        let Some(book) = Self::with_block_entity(world, pos, LecternBlockEntity::take_book) else {
            return InteractionResult::Pass;
        };

        if !book.is_empty() {
            player.add_item_or_drop(book);
        }

        Self::reset_book_state(state, world, pos, false);
        InteractionResult::Success
    }

    /// Ends the page-turn pulse scheduled by [`Self::signal_page_change`].
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::POWERED, false),
            UpdateFlags::UPDATE_ALL,
        );
        Self::update_below(state, world, pos);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(LecternBlockEntity::new(level, pos, state)))
    }

    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    fn get_own_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        if state.get_value(&BlockStateProperties::POWERED) {
            PULSE_SIGNAL
        } else {
            0
        }
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        if direction == Direction::Up && state.get_value(&BlockStateProperties::POWERED) {
            PULSE_SIGNAL
        } else {
            0
        }
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        if !state.get_value(&BlockStateProperties::HAS_BOOK) {
            return 0;
        }

        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<LecternBlockEntity>()
                    .map(LecternBlockEntity::redstone_signal)
            })
            .unwrap_or(0)
    }
}

impl LecternBlock {
    /// Vanilla `LecternBlock.signalPageChange`: a two-tick redstone pulse on page turn.
    pub fn signal_page_change(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::POWERED, true),
            UpdateFlags::UPDATE_ALL,
        );
        Self::update_below(state, world, pos);
        world.schedule_block_tick_default(pos, state.get_block(), PAGE_CHANGE_IMPULSE_TICKS);
    }
}
