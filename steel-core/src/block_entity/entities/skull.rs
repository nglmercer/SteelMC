//! Skull block-entity owner, sound and name storage.

use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use simdnbt::{FromNbtTag as _, ToNbtTag as _};
use steel_registry::resolvable_profile::ResolvableProfile;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{
    BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier, locks::SyncMutex,
};
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

struct SkullState {
    owner: Option<ResolvableProfile>,
    note_block_sound: Option<Identifier>,
    custom_name: Option<TextComponent>,
}

/// Vanilla `SkullBlockEntity`.
///
/// Vanilla's dragon/piglin head animation counters are client-side only and have no
/// server-side representation here.
pub struct SkullBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<SkullState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SkullBlockEntity`.
unsafe impl DowncastType for SkullBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/skull");
}

impl SkullBlockEntity {
    /// Creates an ownerless skull block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::SKULL, world, pos, state),
            state: SyncMutex::new(SkullState {
                owner: None,
                note_block_sound: None,
                custom_name: None,
            }),
        }
    }

    /// Returns the profile of the player this skull belongs to, if any.
    #[must_use]
    pub fn owner_profile(&self) -> Option<ResolvableProfile> {
        self.state.lock().owner.clone()
    }

    /// Returns the note block sound this skull overrides, if any.
    #[must_use]
    pub fn note_block_sound(&self) -> Option<Identifier> {
        self.state.lock().note_block_sound.clone()
    }

    /// Returns the skull's custom name, if it has one.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.state.lock().custom_name.clone()
    }

    /// Applies the components carried by the item this skull was placed from.
    pub fn apply_item_components(
        &self,
        owner: Option<ResolvableProfile>,
        note_block_sound: Option<Identifier>,
        custom_name: Option<TextComponent>,
    ) {
        let mut state = self.state.lock();
        state.owner = owner;
        state.note_block_sound = note_block_sound;
        state.custom_name = custom_name;
        drop(state);
        self.set_changed();
    }
}

impl BlockEntity for SkullBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();

        state.owner = nbt.get("profile").and_then(ResolvableProfile::from_nbt_tag);
        state.note_block_sound = nbt
            .get("note_block_sound")
            .and_then(Identifier::from_nbt_tag);
        state.custom_name = nbt.get("custom_name").and_then(TextComponent::from_nbt_tag);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();

        if let Some(owner) = &state.owner {
            nbt.insert("profile", owner.clone().to_nbt_tag());
        }
        if let Some(note_block_sound) = &state.note_block_sound {
            nbt.insert("note_block_sound", note_block_sound.clone().to_nbt_tag());
        }
        if let Some(custom_name) = &state.custom_name {
            nbt.insert("custom_name", custom_name.clone().to_nbt_tag());
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}
