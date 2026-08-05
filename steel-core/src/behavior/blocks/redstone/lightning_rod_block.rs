//! Lightning rod block behavior.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::level_events;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::{BlockBehavior, schedule_water_tick_if_waterlogged};
use crate::behavior::blocks::building::{WeatherState, WeatheringCopper};
use crate::behavior::context::BlockPlaceContext;
use crate::entity::ai::path::PathComputationType;
use crate::world::{LevelReader, ScheduledTickAccess, SignalQueryContext, World};

/// Vanilla `LightningRodBlock.ACTIVATION_TICKS`: how long a struck rod stays powered.
const ACTIVATION_TICKS: i32 = 8;
/// Redstone strength a struck rod emits.
const POWERED_SIGNAL: i32 = 15;

/// Shared vanilla `LightningRodBlock` logic, reused by the weathering variants.
pub struct LightningRod {
    block: BlockRef,
}

impl LightningRod {
    /// Creates the shared lightning rod logic for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `LightningRodBlock.onLightningStrike`: power the rod for a few ticks.
    ///
    /// Lightning bolts are not simulated yet, so nothing calls this during play.
    pub fn on_lightning_strike(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let facing = state.get_value(&BlockStateProperties::FACING);
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::POWERED, true),
            UpdateFlags::UPDATE_ALL,
        );
        self.update_neighbours(state, world, pos);
        world.schedule_block_tick_default(pos, self.block, ACTIVATION_TICKS);
        world.level_event(
            level_events::PARTICLES_ELECTRIC_SPARK,
            pos,
            facing.get_axis() as i32,
            None,
        );
    }

    /// Vanilla `LightningRodBlock.updateNeighbours`: the block behind the rod tip.
    fn update_neighbours(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let front = state.get_value(&BlockStateProperties::FACING).opposite();
        world.update_neighbors_at(pos.relative(front), self.block);
    }

    fn placement_state(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        self.block
            .default_state()
            .set_value(&BlockStateProperties::FACING, context.clicked_face())
            .set_value(
                &BlockStateProperties::WATERLOGGED,
                context.is_water_source(),
            )
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::POWERED, false),
            UpdateFlags::UPDATE_ALL,
        );
        self.update_neighbours(state, world, pos);
    }

    fn on_place(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos, old: BlockStateId) {
        // A rod restored while still powered has to finish its activation window.
        if state.get_block() == old.get_block()
            || !state.get_value(&BlockStateProperties::POWERED)
            || world.has_scheduled_block_tick(pos, self.block)
        {
            return;
        }
        world.schedule_block_tick_default(pos, self.block, ACTIVATION_TICKS);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
    ) {
        if state.get_value(&BlockStateProperties::POWERED) {
            self.update_neighbours(state, world, pos);
        }
    }

    fn get_direct_signal(state: BlockStateId, direction: Direction) -> i32 {
        if state.get_value(&BlockStateProperties::POWERED)
            && state.get_value(&BlockStateProperties::FACING) == direction
        {
            POWERED_SIGNAL
        } else {
            0
        }
    }

    fn get_own_signal(state: BlockStateId) -> i32 {
        i32::from(state.get_value(&BlockStateProperties::POWERED)) * POWERED_SIGNAL
    }
}

/// Vanilla `LightningRodBlock` behavior.
#[block_behavior]
pub struct LightningRodBlock {
    rod: LightningRod,
}

impl LightningRodBlock {
    /// Creates a new lightning rod behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            rod: LightningRod::new(block),
        }
    }

    /// Vanilla `LightningRodBlock.onLightningStrike`.
    pub fn on_lightning_strike(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.rod.on_lightning_strike(state, world, pos);
    }
}

impl BlockBehavior for LightningRodBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.rod.placement_state(context))
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
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.rod.tick(state, world, pos);
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        self.rod.on_place(state, world, pos, old_state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        self.rod.affect_neighbors_after_removal(state, world, pos);
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
        LightningRod::get_own_signal(state)
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        LightningRod::get_direct_signal(state, direction)
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

/// Vanilla `WeatheringLightningRodBlock` behavior: a lightning rod that oxidizes.
#[block_behavior]
pub struct WeatheringLightningRodBlock {
    rod: LightningRod,
    #[json_arg(r#enum = "WeatherState", json = "weather_state")]
    weathering: WeatheringCopper,
}

impl WeatheringLightningRodBlock {
    /// Creates a new weathering lightning rod behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weather_state: WeatherState) -> Self {
        Self {
            rod: LightningRod::new(block),
            weathering: WeatheringCopper::new(weather_state),
        }
    }

    /// Vanilla `LightningRodBlock.onLightningStrike`.
    pub fn on_lightning_strike(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.rod.on_lightning_strike(state, world, pos);
    }
}

impl BlockBehavior for WeatheringLightningRodBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.rod.placement_state(context))
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
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.rod.tick(state, world, pos);
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.weathering.change_over_time(state, world, pos);
    }

    fn on_place(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        self.rod.on_place(state, world, pos, old_state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        self.rod.affect_neighbors_after_removal(state, world, pos);
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
        LightningRod::get_own_signal(state)
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        LightningRod::get_direct_signal(state, direction)
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
