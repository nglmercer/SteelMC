//! Brushable block behavior (suspicious sand and suspicious gravel).

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::level_events;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_game_events;
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _};

use crate::behavior::blocks::terrain::falling_block::is_free;
use crate::behavior::{BlockBehavior, BlockEntityCreation, BlockPlaceContext, Brushable, Fallable};
use crate::block_entity::entities::BrushableBlockEntity;
use crate::entity::entities::FallingBlockEntity;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Vanilla `BrushableBlock.TICK_DELAY`.
const TICK_DELAY: i32 = 2;

/// Vanilla `BrushableBlock` behavior.
#[block_behavior]
pub struct BrushableBlock {
    block: BlockRef,
    #[json_arg(vanilla_blocks, json = "turns_into")]
    turns_into: BlockRef,
    #[json_arg(sound_events, json = "brush_sound")]
    brush_sound: SoundEventRef,
    #[json_arg(sound_events, json = "brush_completed_sound")]
    brush_completed_sound: SoundEventRef,
}

impl BrushableBlock {
    /// Creates a new brushable block behavior.
    #[must_use]
    pub const fn new(
        block: BlockRef,
        turns_into: BlockRef,
        brush_sound: SoundEventRef,
        brush_completed_sound: SoundEventRef,
    ) -> Self {
        Self {
            block,
            turns_into,
            brush_sound,
            brush_completed_sound,
        }
    }
}

impl BlockBehavior for BrushableBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, TICK_DELAY);
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
        world.schedule_block_tick_default(pos, self.block, TICK_DELAY);
        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if let Some(block_entity) = world.get_block_entity(pos)
            && let Some(brushable) = block_entity.downcast_ref::<BrushableBlockEntity>()
        {
            brushable.check_reset(world);
        }

        if is_free(world.get_block_state(pos.below()))
            && pos.y() >= world.min_y()
            && let Some(entity) = FallingBlockEntity::fall(world, pos, state)
        {
            // Vanilla: brushed blocks never drop themselves as falling-block items.
            entity.set_drop_item(false);
        }
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(BrushableBlockEntity::new(level, pos, state)))
    }

    fn as_fallable(&self) -> Option<&dyn Fallable> {
        Some(self)
    }

    fn as_brushable(&self) -> Option<&dyn Brushable> {
        Some(self)
    }
}

impl Fallable for BrushableBlock {
    /// Vanilla `BrushableBlock.onBrokenAfterFall`: a brushable block that cannot land is
    /// destroyed with break particles and a block-destroy vibration.
    fn on_broken_after_fall(&self, world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        world.level_event(
            level_events::PARTICLES_DESTROY_BLOCK,
            pos,
            level_events::encode_block_state_data(u32::from(state.0)),
            None,
        );
        world.game_event(
            &vanilla_game_events::BLOCK_DESTROY,
            pos,
            &GameEventContext::default(),
        );
    }
}

impl Brushable for BrushableBlock {
    fn brush_sound(&self) -> SoundEventRef {
        self.brush_sound
    }

    fn brush_completed_sound(&self) -> SoundEventRef {
        self.brush_completed_sound
    }

    fn turns_into(&self) -> BlockRef {
        self.turns_into
    }
}
