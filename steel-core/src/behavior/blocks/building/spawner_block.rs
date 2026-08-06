//! `SpawnerBlock` behavior.

use std::sync::{Arc, Weak};

use rand::RngExt as _;

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::{BlockBehavior, BlockEntityCreation, BlockPlaceContext};
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::world::World;

/// Vanilla `SpawnerBlock`.
#[block_behavior(class = "SpawnerBlock")]
pub struct SpawnerBlock {
    block: BlockRef,
}

impl SpawnerBlock {
    /// Creates the spawner block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for SpawnerBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::MOB_SPAWNER,
            level,
            pos,
            state,
        ))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::MOB_SPAWNER,
        )
    }

    /// Vanilla `SpawnerBlock.spawnAfterBreak` drops 15–44 experience.
    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _tool: &ItemStack,
        drop_experience: bool,
    ) {
        if !drop_experience {
            return;
        }
        let mut rng = rand::rng();
        let amount = 15 + rng.random_range(0..15) + rng.random_range(0..15);
        world.pop_experience(pos, amount);
    }
}
