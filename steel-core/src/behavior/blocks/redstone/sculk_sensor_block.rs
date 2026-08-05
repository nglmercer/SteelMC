//! Sculk sensor and calibrated sculk sensor behaviors.

use std::sync::{Arc, Weak};

use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, SculkSensorPhase};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types, vanilla_blocks,
    vanilla_game_events,
};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::context::BlockPlaceContext;
use crate::block_entity::BlockEntityTicker;
use crate::block_entity::entities::SculkSensorBlockEntity;
use crate::entity::ai::path::PathComputationType;
use crate::world::game_event::GameEventContext;
use crate::world::game_event::vibration::resonance_event;
use crate::world::{LevelReader, ScheduledTickAccess, SignalQueryContext, World};

/// Vanilla `SculkSensorBlock.ACTIVE_TICKS`.
const ACTIVE_TICKS: i32 = 30;
/// Vanilla `CalibratedSculkSensorBlock.getActiveTicks`.
const CALIBRATED_ACTIVE_TICKS: i32 = 10;
/// Vanilla `SculkSensorBlock.COOLDOWN_TICKS`.
const COOLDOWN_TICKS: i32 = 10;
/// Experience a broken sculk sensor drops.
const BREAK_EXPERIENCE: i32 = 5;

/// Vanilla `SculkSensorBlock.RESONANCE_PITCH_BEND`, built from `NoteBlock.getPitchFromNote`
/// over vanilla's resonance tone map.
const RESONANCE_TONES: [i32; 16] = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];

/// Vanilla `NoteBlock.getPitchFromNote`.
fn pitch_from_note(note: i32) -> f32 {
    2.0_f32.powf((f32::from(i16::try_from(note).unwrap_or(0)) - 12.0) / 12.0)
}

/// Vanilla `SculkSensorBlock` behavior.
#[block_behavior]
pub struct SculkSensorBlock {
    block: BlockRef,
}

impl SculkSensorBlock {
    /// Creates a new sculk sensor behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `SculkSensorBlock.getPhase`.
    #[must_use]
    pub fn phase(state: BlockStateId) -> SculkSensorPhase {
        state.get_value(&BlockStateProperties::SCULK_SENSOR_PHASE)
    }

    /// Vanilla `SculkSensorBlock.canActivate`.
    #[must_use]
    pub fn can_activate(state: BlockStateId) -> bool {
        Self::phase(state) == SculkSensorPhase::Inactive
    }

    /// Vanilla `SculkSensorBlock.updateNeighbours`: the sensor itself and the block below.
    fn update_neighbours(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        let block = state.get_block();
        world.update_neighbors_at(pos, block);
        world.update_neighbors_at(pos.below(), block);
    }

    /// Vanilla `SculkSensorBlock.deactivate`.
    pub fn deactivate(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        world.set_block(
            pos,
            state
                .set_value(
                    &BlockStateProperties::SCULK_SENSOR_PHASE,
                    SculkSensorPhase::Cooldown,
                )
                .set_value(&BlockStateProperties::POWER, 0_u8),
            UpdateFlags::UPDATE_ALL,
        );
        world.schedule_block_tick_default(pos, state.get_block(), COOLDOWN_TICKS);
        Self::update_neighbours(world, pos, state);
    }

    /// Vanilla `SculkSensorBlock.activate`.
    pub fn activate(
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        power: i32,
        frequency: i32,
    ) {
        let block = state.get_block();
        let active_ticks = if block == &vanilla_blocks::CALIBRATED_SCULK_SENSOR {
            CALIBRATED_ACTIVE_TICKS
        } else {
            ACTIVE_TICKS
        };

        world.set_block(
            pos,
            state
                .set_value(
                    &BlockStateProperties::SCULK_SENSOR_PHASE,
                    SculkSensorPhase::Active,
                )
                .set_value(
                    &BlockStateProperties::POWER,
                    u8::try_from(power.clamp(0, 15)).unwrap_or(0),
                ),
            UpdateFlags::UPDATE_ALL,
        );
        world.schedule_block_tick_default(pos, block, active_ticks);
        Self::update_neighbours(world, pos, state);
        Self::try_resonate_vibration(world, pos, frequency);
        world.game_event(
            &vanilla_game_events::SCULK_SENSOR_TENDRILS_CLICKING,
            pos,
            &GameEventContext::new(None, Some(state)),
        );

        if !state.get_value(&BlockStateProperties::WATERLOGGED) {
            let pitch = rand::rng().random::<f32>() * 0.2 + 0.8;
            world.play_sound(
                &sound_events::BLOCK_SCULK_SENSOR_CLICKING,
                SoundSource::Blocks,
                pos,
                1.0,
                pitch,
                None,
            );
        }
    }

    /// Vanilla `SculkSensorBlock.tryResonateVibration`: adjacent amethyst re-emits the
    /// vibration at its own frequency.
    fn try_resonate_vibration(world: &Arc<World>, pos: BlockPos, frequency: i32) {
        let Some(resonance) = resonance_event(frequency) else {
            return;
        };
        let Ok(tone_index) = usize::try_from(frequency) else {
            return;
        };
        let Some(tone) = RESONANCE_TONES.get(tone_index) else {
            return;
        };

        for direction in Direction::ALL {
            let neighbor_pos = pos.relative(direction);
            let neighbor_state = world.get_block_state(neighbor_pos);
            if !REGISTRY
                .blocks
                .is_in_tag(neighbor_state.get_block(), &BlockTag::VIBRATION_RESONATORS)
            {
                continue;
            }

            world.game_event(
                resonance,
                neighbor_pos,
                &GameEventContext::new(None, Some(neighbor_state)),
            );
            world.play_sound(
                &sound_events::BLOCK_AMETHYST_BLOCK_RESONATE,
                SoundSource::Blocks,
                neighbor_pos,
                1.0,
                pitch_from_note(*tone),
                None,
            );
        }
    }

    fn placement_state(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        self.block.default_state().set_value(
            &BlockStateProperties::WATERLOGGED,
            context.is_water_source(),
        )
    }

    /// Shared vanilla `SculkSensorBlock.tick`.
    fn run_tick(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        match Self::phase(state) {
            SculkSensorPhase::Active => Self::deactivate(world, pos, state),
            SculkSensorPhase::Cooldown => {
                world.set_block(
                    pos,
                    state.set_value(
                        &BlockStateProperties::SCULK_SENSOR_PHASE,
                        SculkSensorPhase::Inactive,
                    ),
                    UpdateFlags::UPDATE_ALL,
                );
                if !state.get_value(&BlockStateProperties::WATERLOGGED) {
                    let pitch = rand::rng().random::<f32>() * 0.2 + 0.8;
                    world.play_sound(
                        &sound_events::BLOCK_SCULK_SENSOR_CLICKING_STOP,
                        SoundSource::Blocks,
                        pos,
                        1.0,
                        pitch,
                        None,
                    );
                }
            }
            SculkSensorPhase::Inactive => {}
        }
    }

    /// Shared vanilla `SculkSensorBlock.affectNeighborsAfterRemoval`.
    fn removal_updates(world: &Arc<World>, pos: BlockPos, state: BlockStateId) {
        if Self::phase(state) == SculkSensorPhase::Active {
            Self::update_neighbours(world, pos, state);
        }
    }

    /// Shared vanilla `SculkSensorBlock.ownSignal`.
    fn own_power(state: BlockStateId) -> i32 {
        i32::from(state.get_value(&BlockStateProperties::POWER))
    }

    fn new_sensor_entity(
        entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(SculkSensorBlockEntity::new(
            entity_type,
            level,
            pos,
            state,
        )))
    }
}

impl BlockBehavior for SculkSensorBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.placement_state(context))
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
        Self::run_tick(world, pos, state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        Self::removal_updates(world, pos, state);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        Self::new_sensor_entity(&vanilla_block_entity_types::SCULK_SENSOR, level, pos, state)
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::SCULK_SENSOR,
        )
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
        Self::own_power(state)
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        // Only the block above a sensor is strongly powered.
        if direction == Direction::Up {
            Self::own_power(state)
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
        if Self::phase(state) != SculkSensorPhase::Active {
            return 0;
        }
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                use steel_utils::Downcast as _;
                entity
                    .downcast_ref::<SculkSensorBlockEntity>()
                    .map(SculkSensorBlockEntity::last_vibration_frequency)
            })
            .unwrap_or(0)
    }

    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _tool: &ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            world.pop_experience(pos, BREAK_EXPERIENCE);
        }
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}

/// Vanilla `CalibratedSculkSensorBlock` behavior: a sensor that only hears one frequency.
#[block_behavior]
pub struct CalibratedSculkSensorBlock {
    sensor: SculkSensorBlock,
}

impl CalibratedSculkSensorBlock {
    /// Creates a new calibrated sculk sensor behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            sensor: SculkSensorBlock::new(block),
        }
    }
}

impl BlockBehavior for CalibratedSculkSensorBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.sensor.placement_state(context).set_value(
            &BlockStateProperties::HORIZONTAL_FACING,
            context.horizontal_direction(),
        ))
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
        SculkSensorBlock::run_tick(world, pos, state);
    }

    fn affect_neighbors_after_removal(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _moved_by_piston: bool,
    ) {
        SculkSensorBlock::removal_updates(world, pos, state);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        SculkSensorBlock::new_sensor_entity(
            &vanilla_block_entity_types::CALIBRATED_SCULK_SENSOR,
            level,
            pos,
            state,
        )
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::CALIBRATED_SCULK_SENSOR,
        )
    }

    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    /// The calibration face reads redstone in, so it never powers out.
    fn get_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        if direction == state.get_value(&BlockStateProperties::HORIZONTAL_FACING) {
            0
        } else {
            SculkSensorBlock::own_power(state)
        }
    }

    fn get_own_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        SculkSensorBlock::own_power(state)
    }

    fn get_direct_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        direction: Direction,
        _context: SignalQueryContext,
    ) -> i32 {
        if direction == Direction::Up {
            SculkSensorBlock::own_power(state)
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
        direction: Direction,
    ) -> i32 {
        self.sensor
            .get_analog_output_signal(state, world, pos, direction)
    }

    fn spawn_after_break(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        drop_experience: bool,
    ) {
        self.sensor
            .spawn_after_break(state, world, pos, tool, drop_experience);
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
