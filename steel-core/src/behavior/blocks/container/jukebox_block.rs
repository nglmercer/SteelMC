//! Jukebox block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::data_components::vanilla_components::JUKEBOX_PLAYABLE;
use steel_registry::vanilla_block_entity_types;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BlockEntityTicker;
use crate::block_entity::entities::JukeboxBlockEntity;
use crate::player::Player;
use crate::world::{LevelReader, SignalQueryContext, World};

/// Redstone strength a jukebox emits while a record is playing.
const PLAYING_SIGNAL: i32 = 15;

/// Vanilla `JukeboxBlock` behavior.
#[block_behavior]
pub struct JukeboxBlock {
    block: BlockRef,
}

impl JukeboxBlock {
    /// Creates a new jukebox behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn with_block_entity<R>(
        world: &Arc<World>,
        pos: BlockPos,
        f: impl FnOnce(&JukeboxBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        block_entity.downcast_ref::<JukeboxBlockEntity>().map(f)
    }

    fn set_has_record(state: BlockStateId, world: &Arc<World>, pos: BlockPos, has_record: bool) {
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::HAS_RECORD, has_record),
            UpdateFlags::UPDATE_CLIENTS,
        );
    }
}

impl BlockBehavior for JukeboxBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(&BlockStateProperties::HAS_RECORD, false),
        )
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if state.get_value(&BlockStateProperties::HAS_RECORD) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        // Only items carrying a jukebox song can be inserted.
        let record =
            inv.with_item(|held| held.get(JUKEBOX_PLAYABLE).is_some().then(|| held.split(1)));
        let Some(record) = record else {
            return InteractionResult::TryEmptyHandInteraction;
        };

        if Self::with_block_entity(world, pos, |jukebox| jukebox.play(world, record)).is_none() {
            return InteractionResult::Pass;
        }

        Self::set_has_record(state, world, pos, true);
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
        if !state.get_value(&BlockStateProperties::HAS_RECORD) {
            return InteractionResult::Pass;
        }

        let Some(record) =
            Self::with_block_entity(world, pos, |jukebox| jukebox.take_record(world))
        else {
            return InteractionResult::Pass;
        };

        if !record.is_empty() {
            player.add_item_or_drop(record);
        }

        Self::set_has_record(state, world, pos, false);
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(JukeboxBlockEntity::new(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::JUKEBOX,
        )
    }

    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    fn get_own_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        // Vanilla powers neighbours only while a song is actually playing.
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<JukeboxBlockEntity>()
                    .map(|jukebox| i32::from(jukebox.is_playing()) * PLAYING_SIGNAL)
            })
            .unwrap_or(0)
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
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<JukeboxBlockEntity>()
                    .map(JukeboxBlockEntity::comparator_output)
            })
            .unwrap_or(0)
    }
}
