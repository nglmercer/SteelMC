//! Enchanting table block entity.

use std::sync::Weak;

use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;

use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Vanilla `EnchantingTableBlockEntity`.
///
/// Vanilla stores only the table's custom name here — every other field drives the client's
/// book animation — so this entity carries no state of its own beyond the base.
pub struct EnchantingTableBlockEntity {
    base: BlockEntityBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies
// `EnchantingTableBlockEntity`.
unsafe impl DowncastType for EnchantingTableBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/enchanting_table");
}

impl EnchantingTableBlockEntity {
    /// Creates an enchanting table block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::ENCHANTING_TABLE,
                world,
                pos,
                state,
            ),
        }
    }
}

impl BlockEntity for EnchantingTableBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, _nbt: &BorrowedNbtCompound<'_>) {}

    fn save_additional(&self, _nbt: &mut NbtCompound) {}
}
