//! Result slot handler for the loom.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use steel_protocol::packets::game::SoundSource;
use steel_registry::{item_stack::ItemStack, sound_events};
use steel_utils::{BlockPos, locks::Shared};

use crate::{
    inventory::{
        container::{ResultContainer, SimpleContainer},
        lock::{ContainerId, ContainerLockGuard, ContainerRef},
        slots::ResultHandler,
    },
    player::Player,
    world::World,
};

/// Result slot handler for a loom.
///
/// Taking the woven banner consumes one banner and one dye, matching vanilla.
#[derive(Clone)]
pub struct LoomResultHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    block_pos: BlockPos,
    world: Arc<World>,
    /// Vanilla plays the weave sound at most once per game tick.
    last_sound_tick: Arc<AtomicI64>,
}

impl LoomResultHandler {
    /// Creates a new handler.
    #[must_use]
    pub fn new(
        input_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
        block_pos: BlockPos,
        world: Arc<World>,
    ) -> Self {
        Self {
            input_container,
            result_container,
            block_pos,
            world,
            last_sound_tick: Arc::new(AtomicI64::new(-1)),
        }
    }
}

impl ResultHandler for LoomResultHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, _guard: &mut ContainerLockGuard) {}

    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        let input_id = ContainerId::from_arc(&self.input_container);
        let input = guard.get_mut(input_id).expect("input container not locked");
        input.remove_item(0, 1);
        input.remove_item(1, 1);
        input.set_changed();

        let game_time = self.world.game_time();
        if self.last_sound_tick.swap(game_time, Ordering::Relaxed) != game_time {
            self.world.play_sound(
                &sound_events::UI_LOOM_TAKE_RESULT,
                SoundSource::Blocks,
                self.block_pos,
                1.0,
                1.0,
                None,
            );
        }

        guard
            .get_mut(ContainerId::from_arc(&self.result_container))
            .expect("container not locked")
            .set_changed();
        None
    }

    fn is_result_valid(&self, _guard: &ContainerLockGuard, _player: &Player) -> bool {
        true
    }
}
