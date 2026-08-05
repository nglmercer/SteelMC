//! Lantern block behavior.
//!
//! Lanterns either stand on top of a supporting block or hang from one above,
//! and can be waterlogged.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use steel_registry::blocks::shapes::SupportType;
use steel_registry::vanilla_blocks;
use steel_utils::axis::Axis;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::schedule_water_tick_if_waterlogged;
use crate::behavior::blocks::{WeatherState, WeatheringCopper};
use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::entity::ai::path::PathComputationType;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Whether the lantern hangs from the block above instead of standing on the block below.
const HANGING: BoolProperty = BlockStateProperties::HANGING;
/// Waterlogged property.
const WATERLOGGED: BoolProperty = BlockStateProperties::WATERLOGGED;

/// Vanilla `LanternBlock` behavior.
#[block_behavior]
pub struct LanternBlock {
    block: BlockRef,
}

impl LanternBlock {
    /// Creates a new lantern block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `LanternBlock.getConnectedDirection`: the face the lantern attaches to.
    #[must_use]
    fn connected_direction(state: BlockStateId) -> Direction {
        if state.get_value(&HANGING) {
            Direction::Down
        } else {
            Direction::Up
        }
    }

    fn can_survive_at(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let attach_direction = Self::connected_direction(state).opposite();
        let support_pos = pos.relative(attach_direction);
        world.is_face_sturdy_for(
            world.get_block_state(support_pos),
            support_pos,
            attach_direction.opposite(),
            SupportType::Center,
        )
    }

    fn placement_state(block: BlockRef, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        for direction in context.get_nearest_looking_directions() {
            if direction.get_axis() != Axis::Y {
                continue;
            }

            let state = block
                .default_state()
                .set_value(&HANGING, direction == Direction::Up);
            if Self::can_survive_at(state, context.world, context.place_pos()) {
                return Some(state.set_value(&WATERLOGGED, context.is_water_source()));
            }
        }

        None
    }

    fn shape_update(
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);

        if Self::connected_direction(state).opposite() == direction
            && !Self::can_survive_at(state, world, pos)
        {
            return vanilla_blocks::AIR.default_state();
        }

        state
    }
}

impl BlockBehavior for LanternBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Self::placement_state(self.block, context)
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::can_survive_at(state, world, pos)
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        Self::shape_update(state, world, pos, direction)
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

/// Vanilla `WeatheringLanternBlock` behavior.
#[block_behavior]
pub struct WeatheringLanternBlock {
    block: BlockRef,
    #[json_arg(r#enum = "WeatherState", json = "weather_state")]
    weathering: WeatheringCopper,
}

impl WeatheringLanternBlock {
    /// Creates a new weathering lantern block behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weather_state: WeatherState) -> Self {
        Self {
            block,
            weathering: WeatheringCopper::new(weather_state),
        }
    }
}

impl BlockBehavior for WeatheringLanternBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        LanternBlock::placement_state(self.block, context)
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        LanternBlock::can_survive_at(state, world, pos)
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        LanternBlock::shape_update(state, world, pos, direction)
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.weathering.change_over_time(state, world, pos);
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
