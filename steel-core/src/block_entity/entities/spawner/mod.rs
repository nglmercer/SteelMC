//! Mob spawner block entity.

mod base_spawner;
mod spawn_data;

use std::sync::{Arc, Weak};

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use self::base_spawner::BaseSpawner;
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Vanilla `SpawnerBlockEntity`.
pub struct SpawnerBlockEntity {
    base: BlockEntityBase,
    spawner: BaseSpawner,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SpawnerBlockEntity`.
unsafe impl DowncastType for SpawnerBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/spawner");
}

impl SpawnerBlockEntity {
    /// Creates a mob spawner block entity with vanilla default state.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::MOB_SPAWNER, world, pos, state),
            spawner: BaseSpawner::new(),
        }
    }

    /// Runs vanilla `Spawner.setEntityId`, used by spawn eggs used on a spawner.
    pub fn set_entity_id(&self, entity_type: EntityTypeRef) {
        self.spawner.set_entity_id(entity_type, &mut rand::rng());
        self.set_changed();
    }
}

impl BlockEntity for SpawnerBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn trigger_event(&self, param_a: i32, _param_b: i32) -> bool {
        BaseSpawner::on_event_triggered(param_a)
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        self.spawner.load(&nbt);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.spawner.save(nbt);
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        // Vanilla `SpawnerBlockEntity.getUpdateTag` withholds the weighted list.
        nbt.remove("SpawnPotentials");
        Some(nbt)
    }

    fn tick(&self, world: &Arc<World>) {
        self.spawner.server_tick(world, self.get_block_pos());
    }
}
