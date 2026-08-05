//! Copper golem statue block entity.

use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use simdnbt::{FromNbtTag as _, ToNbtTag as _};
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Vanilla `CopperGolemStatueBlockEntity`.
///
/// The statue only carries the golem's custom name; waxing a statue back into a live
/// copper golem needs the `CopperGolem` entity, which Steel does not have yet.
pub struct CopperGolemStatueBlockEntity {
    base: BlockEntityBase,
    custom_name: SyncMutex<Option<TextComponent>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CopperGolemStatueBlockEntity`.
unsafe impl DowncastType for CopperGolemStatueBlockEntity {
    const TYPE_KEY: DowncastTypeKey =
        DowncastTypeKey::new("steel:block_entity/copper_golem_statue");
}

impl CopperGolemStatueBlockEntity {
    /// Creates an unnamed copper golem statue block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(
                &vanilla_block_entity_types::COPPER_GOLEM_STATUE,
                world,
                pos,
                state,
            ),
            custom_name: SyncMutex::new(None),
        }
    }

    /// Returns the name carried over from the golem this statue was made from.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.custom_name.lock().clone()
    }

    /// Replaces the statue's custom name.
    pub fn set_custom_name(&self, custom_name: Option<TextComponent>) {
        *self.custom_name.lock() = custom_name;
        self.set_changed();
    }
}

impl BlockEntity for CopperGolemStatueBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        *self.custom_name.lock() = nbt.get("CustomName").and_then(TextComponent::from_nbt_tag);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(custom_name) = self.custom_name.lock().clone() {
            nbt.insert("CustomName", custom_name.to_nbt_tag());
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}
