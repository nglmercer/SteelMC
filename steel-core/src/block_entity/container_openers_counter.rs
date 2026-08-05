//! Container openers counter — Steel port of vanilla `ContainerOpenersCounter`.

use std::sync::Arc;

use steel_registry::blocks::BlockRef;
use steel_utils::{BlockPos, BlockStateId, locks::SyncMutex};

use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Tracks how many players have a container open.
///
/// Vanilla rechecks every 5 ticks via `scheduleTick` and plays open/close
/// sounds plus `CONTAINER_OPEN/CLOSE` game events. Steel's simplified version
/// counts increment/decrement calls from `MenuKind::on_open`/`removed` and
/// fires the same observable side-effects that matter for gameplay: block
/// events (lid), sound, game event, and for barrels the `OPEN` blockstate.
pub struct ContainerOpenersCounter {
    count: SyncMutex<i32>,
}

impl ContainerOpenersCounter {
    /// Creates a new counter with zero openers.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: SyncMutex::new(0),
        }
    }

    /// Returns the current count of openers.
    #[must_use]
    pub fn get_count(&self) -> i32 {
        *self.count.lock()
    }

    /// Vanilla `incrementOpeners`.
    pub fn increment(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        block: BlockRef,
        play_open: impl FnOnce(&Arc<World>, BlockPos, BlockStateId),
    ) {
        let mut count = self.count.lock();
        let prev = *count;
        *count += 1;
        if prev == 0 {
            play_open(world, pos, state);
            world.game_event(
                &steel_registry::vanilla_game_events::CONTAINER_OPEN,
                pos,
                &GameEventContext::default(),
            );
            Self::schedule_recheck(world, pos, block);
        }
        let current = *count;
        drop(count);
        Self::opener_count_changed(world, pos, block, prev, current);
    }

    /// Vanilla `decrementOpeners`.
    pub fn decrement(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        state: BlockStateId,
        block: BlockRef,
        play_close: impl FnOnce(&Arc<World>, BlockPos, BlockStateId),
    ) {
        let mut count = self.count.lock();
        let prev = *count;
        if *count > 0 {
            *count -= 1;
        }
        let current = *count;
        let should_close = current == 0;
        drop(count);
        if should_close {
            play_close(world, pos, state);
            world.game_event(
                &steel_registry::vanilla_game_events::CONTAINER_CLOSE,
                pos,
                &GameEventContext::default(),
            );
        }
        Self::opener_count_changed(world, pos, block, prev, current);
    }

    /// Vanilla `recheckOpeners` — simplified: schedule another tick if still open.
    ///
    /// Steel does not scan `Level.getEntities(ContainerUser)`; the menu close
    /// path decrements the counter reliably. The recheck exists so block ticks
    /// remain vanilla-compatible and to fire `openerCountChanged` if a player
    /// disconnects without a clean close (counter will be fixed on next tick
    /// via `Player` cleanup if needed).
    pub fn recheck(&self, world: &Arc<World>, pos: BlockPos, block: BlockRef) {
        let prev = *self.count.lock();
        // In Steel the count is authoritative; just ensure a recheck tick while open.
        if prev > 0 {
            Self::schedule_recheck(world, pos, block);
        }
    }

    fn opener_count_changed(
        world: &Arc<World>,
        pos: BlockPos,
        block: BlockRef,
        _prev: i32,
        current: i32,
    ) {
        world.block_event(pos, block, 1, current);
    }

    fn schedule_recheck(world: &Arc<World>, pos: BlockPos, block: BlockRef) {
        world.schedule_block_tick_default(pos, block, 5);
    }
}

impl Default for ContainerOpenersCounter {
    fn default() -> Self {
        Self::new()
    }
}
