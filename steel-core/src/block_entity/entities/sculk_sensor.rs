//! Sculk sensor block entity, including the calibrated variant.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::game_events::GameEventRef;
use steel_registry::{vanilla_block_entity_types, vanilla_game_events};
use steel_utils::locks::SyncMutex;
use steel_utils::{
    BlockPos, BlockStateId, Downcast as _, DowncastType, DowncastTypeKey, SectionPos,
};

use crate::behavior::blocks::SculkSensorBlock;
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::game_event::vibration::{
    VibrationData, VibrationUser, game_event_frequency, handle_vibration_event,
    redstone_strength_for_distance, tick_vibration,
};
use crate::world::game_event::{GameEventContext, GameEventListener, SharedGameEventListener};
use crate::world::{LevelReader as _, SignalGetter as _, World};

/// Vanilla `SculkSensorBlockEntity.VibrationUser.LISTENER_RANGE`.
const SENSOR_LISTENER_RANGE: i32 = 8;
/// Vanilla `CalibratedSculkSensorBlockEntity.VibrationUser.getListenerRadius`.
const CALIBRATED_LISTENER_RANGE: i32 = 16;

/// Vanilla `SculkSensorBlockEntity`, also backing the calibrated variant.
pub struct SculkSensorBlockEntity {
    base: BlockEntityBase,
    data: SyncMutex<VibrationData>,
    last_vibration_frequency: AtomicI32,
    /// The registered vibration listener, kept so it can be unregistered by identity.
    listener: SyncMutex<Option<SharedGameEventListener>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SculkSensorBlockEntity`.
unsafe impl DowncastType for SculkSensorBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/sculk_sensor");
}

impl SculkSensorBlockEntity {
    /// Creates a sculk sensor block entity of `entity_type`.
    #[must_use]
    pub fn new(
        entity_type: BlockEntityTypeRef,
        world: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        Self {
            base: BlockEntityBase::new(entity_type, world, pos, state),
            data: SyncMutex::new(VibrationData::default()),
            last_vibration_frequency: AtomicI32::new(0),
            listener: SyncMutex::new(None),
        }
    }

    /// Returns the frequency of the vibration this sensor last received.
    #[must_use]
    pub fn last_vibration_frequency(&self) -> i32 {
        self.last_vibration_frequency.load(Ordering::Relaxed)
    }

    /// Whether this is the calibrated variant, which listens further and can filter.
    fn is_calibrated(&self) -> bool {
        self.base.block_entity_type == &vanilla_block_entity_types::CALIBRATED_SCULK_SENSOR
    }

    fn listener_range(&self) -> i32 {
        if self.is_calibrated() {
            CALIBRATED_LISTENER_RANGE
        } else {
            SENSOR_LISTENER_RANGE
        }
    }

    /// Vanilla `CalibratedSculkSensorBlockEntity.VibrationUser.getBackSignal`: the redstone
    /// strength fed into the sensor's rear, which selects the frequency it listens for.
    fn back_signal(&self, world: &Arc<World>, state: BlockStateId) -> i32 {
        let facing = state
            .get_value(&BlockStateProperties::HORIZONTAL_FACING)
            .opposite();
        world.get_signal(self.base.pos.relative(facing), facing)
    }

    /// Registers this sensor's vibration listener once its world is available.
    ///
    /// Vanilla registers block entity listeners when the chunk attaches them; Steel has no
    /// such hook, so the sensor registers on its first tick instead.
    fn ensure_listening(&self, world: &Arc<World>) {
        let mut slot = self.listener.lock();
        if slot.is_some() {
            return;
        }
        let listener: SharedGameEventListener = Arc::new(SculkSensorListener {
            pos: self.base.pos,
            radius: self.listener_range(),
        });
        world.register_game_event_listener(
            SectionPos::from_block_pos(self.base.pos),
            Arc::clone(&listener),
        );
        *slot = Some(listener);
    }
}

/// The world-side handle for a sculk sensor's vibration listening.
///
/// The sensor's own state lives in its block entity, which this resolves per event, so the
/// listener never holds a strong reference back to it.
struct SculkSensorListener {
    pos: BlockPos,
    radius: i32,
}

impl GameEventListener for SculkSensorListener {
    fn listener_pos(&self) -> Option<DVec3> {
        Some(DVec3::new(
            f64::from(self.pos.x()) + 0.5,
            f64::from(self.pos.y()) + 0.5,
            f64::from(self.pos.z()) + 0.5,
        ))
    }

    fn listener_radius(&self) -> i32 {
        self.radius
    }

    fn handle_game_event(
        &self,
        world: &Arc<World>,
        event: GameEventRef,
        context: &GameEventContext<'_>,
        source_pos: DVec3,
    ) -> bool {
        let Some(block_entity) = world.get_block_entity(self.pos) else {
            return false;
        };
        let Some(sensor) = block_entity.downcast_ref::<SculkSensorBlockEntity>() else {
            return false;
        };

        let mut data = sensor.data.lock();
        handle_vibration_event(world, &mut data, sensor, event, context, source_pos)
    }
}

impl VibrationUser for SculkSensorBlockEntity {
    fn listener_radius(&self) -> i32 {
        self.listener_range()
    }

    fn listener_pos(&self) -> DVec3 {
        DVec3::new(
            f64::from(self.base.pos.x()) + 0.5,
            f64::from(self.base.pos.y()) + 0.5,
            f64::from(self.base.pos.z()) + 0.5,
        )
    }

    fn can_receive_vibration(
        &self,
        world: &Arc<World>,
        source_pos: BlockPos,
        event: GameEventRef,
        _context: &GameEventContext<'_>,
    ) -> bool {
        let state = world.get_block_state(self.base.pos);

        // A calibrated sensor with a powered rear only hears that one frequency.
        if self.is_calibrated() {
            let filter = self.back_signal(world, state);
            if filter != 0 && game_event_frequency(event) != filter {
                return false;
            }
        }

        // A sensor ignores its own placement and destruction.
        if source_pos == self.base.pos
            && (event == &vanilla_game_events::BLOCK_DESTROY
                || event == &vanilla_game_events::BLOCK_PLACE)
        {
            return false;
        }

        game_event_frequency(event) != 0 && SculkSensorBlock::can_activate(state)
    }

    fn on_receive_vibration(
        &self,
        world: &Arc<World>,
        _source_pos: BlockPos,
        event: GameEventRef,
        distance: f32,
    ) {
        let state = world.get_block_state(self.base.pos);
        if !SculkSensorBlock::can_activate(state) {
            return;
        }

        let frequency = game_event_frequency(event);
        self.last_vibration_frequency
            .store(frequency, Ordering::Relaxed);
        let power = redstone_strength_for_distance(distance, self.listener_range());
        SculkSensorBlock::activate(world, self.base.pos, state, power, frequency);
    }
}

impl BlockEntity for SculkSensorBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla ticks the vibration system from the sensor's block entity ticker.
    fn tick(&self, world: &Arc<World>) {
        self.ensure_listening(world);
        let mut data = self.data.lock();
        tick_vibration(world, &mut data, self);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        if let Some(frequency) = nbt.int("last_vibration_frequency") {
            self.last_vibration_frequency
                .store(frequency, Ordering::Relaxed);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert(
            "last_vibration_frequency",
            self.last_vibration_frequency.load(Ordering::Relaxed),
        );
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let Some(listener) = self.listener.lock().take() else {
            return;
        };
        if let Some(world) = self.base.level() {
            world.unregister_game_event_listener(SectionPos::from_block_pos(pos), &listener);
        }
    }
}
