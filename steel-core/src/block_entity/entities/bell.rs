//! Bell block entity.

use std::sync::{Arc, Weak};

use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{
    BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, locks::SyncMutex,
};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Vanilla `BellBlockEntity.DURATION`: how long the bell keeps swinging.
const RING_DURATION: i32 = 50;

struct BellState {
    /// Ticks remaining in the current swing, or zero when at rest.
    ticks: i32,
    /// The side the bell was struck from, which drives the swing direction.
    click_direction: Direction,
}

/// Vanilla `BellBlockEntity`.
///
/// Vanilla's ring also reveals nearby raiders and notifies raids; that needs the raid
/// system, so this entity only tracks the swing.
pub struct BellBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<BellState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BellBlockEntity`.
unsafe impl DowncastType for BellBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/bell");
}

impl BellBlockEntity {
    /// Creates a bell block entity at rest.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::BELL, world, pos, state),
            state: SyncMutex::new(BellState {
                ticks: 0,
                click_direction: Direction::North,
            }),
        }
    }

    /// Vanilla `BellBlockEntity.onHit`: starts a swing from `direction`.
    pub fn on_hit(&self, direction: Direction) {
        let mut state = self.state.lock();
        state.click_direction = direction;
        state.ticks = RING_DURATION;
    }

    /// Returns whether the bell is currently swinging.
    #[must_use]
    pub fn is_ringing(&self) -> bool {
        self.state.lock().ticks > 0
    }

    /// Returns the side the current swing was started from.
    #[must_use]
    pub fn click_direction(&self) -> Direction {
        self.state.lock().click_direction
    }
}

impl BlockEntity for BellBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `BellBlockEntity.serverTick`: runs the swing down to rest.
    fn tick(&self, _world: &Arc<World>) {
        let mut state = self.state.lock();
        if state.ticks > 0 {
            state.ticks -= 1;
        }
    }

    fn load_additional(&self, _nbt: &BorrowedNbtCompound<'_>) {
        // Vanilla persists nothing: a bell is always at rest after a reload.
    }

    fn save_additional(&self, _nbt: &mut NbtCompound) {}
}
