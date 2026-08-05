//! Banner block-entity pattern and name storage.

use std::sync::Weak;

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use simdnbt::{FromNbtTag as _, ToNbtTag as _};
use steel_registry::data_components::components::BannerPatternLayers;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};
use text_components::TextComponent;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

struct BannerState {
    patterns: BannerPatternLayers,
    custom_name: Option<TextComponent>,
}

/// Vanilla `BannerBlockEntity`.
///
/// The base dye color lives in the block itself (one block per color), so unlike vanilla
/// there is no separate color field to store.
pub struct BannerBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<BannerState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BannerBlockEntity`.
unsafe impl DowncastType for BannerBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/banner");
}

impl BannerBlockEntity {
    /// Creates an empty banner block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::BANNER, world, pos, state),
            state: SyncMutex::new(BannerState {
                patterns: BannerPatternLayers::empty(),
                custom_name: None,
            }),
        }
    }

    /// Returns the banner's pattern layers.
    #[must_use]
    pub fn patterns(&self) -> BannerPatternLayers {
        self.state.lock().patterns.clone()
    }

    /// Replaces the banner's pattern layers.
    pub fn set_patterns(&self, patterns: BannerPatternLayers) {
        self.state.lock().patterns = patterns;
        self.set_changed();
    }

    /// Returns the banner's custom name, if it has one.
    #[must_use]
    pub fn custom_name(&self) -> Option<TextComponent> {
        self.state.lock().custom_name.clone()
    }

    /// Replaces the banner's custom name.
    pub fn set_custom_name(&self, custom_name: Option<TextComponent>) {
        self.state.lock().custom_name = custom_name;
        self.set_changed();
    }
}

impl BlockEntity for BannerBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();

        state.patterns = nbt
            .get("patterns")
            .and_then(BannerPatternLayers::from_nbt_tag)
            .unwrap_or_else(BannerPatternLayers::empty);
        state.custom_name = nbt.get("CustomName").and_then(TextComponent::from_nbt_tag);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();

        // Vanilla omits an empty pattern list entirely.
        if state.patterns != BannerPatternLayers::empty() {
            nbt.insert("patterns", state.patterns.clone().to_nbt_tag());
        }

        if let Some(custom_name) = &state.custom_name {
            nbt.insert("CustomName", custom_name.clone().to_nbt_tag());
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use steel_registry::data_components::components::BannerPatternLayer;
    use steel_registry::test_support::init_test_registry;
    use steel_registry::{DyeColor, RegistryHolder, vanilla_banner_patterns, vanilla_blocks};

    use super::*;

    fn banner() -> BannerBlockEntity {
        init_test_registry();
        BannerBlockEntity::new(
            Weak::new(),
            BlockPos::new(-3, 71, 12),
            vanilla_blocks::WHITE_BANNER.default_state(),
        )
    }

    fn reload(nbt: &NbtCompound) -> BannerBlockEntity {
        let mut bytes = Vec::new();
        nbt.write(&mut bytes);
        let borrowed = read_borrowed_compound(&mut Cursor::new(bytes.as_slice()))
            .expect("test NBT should reborrow");
        let loaded = banner();
        loaded.load_additional(&borrowed);
        loaded
    }

    #[test]
    fn patterns_round_trip_through_vanilla_nbt_key() {
        let source = banner();
        let patterns = BannerPatternLayers::new(vec![BannerPatternLayer::new(
            RegistryHolder::reference(&vanilla_banner_patterns::CREEPER),
            DyeColor::Lime,
        )]);
        source.set_patterns(patterns.clone());

        let mut nbt = NbtCompound::new();
        source.save_additional(&mut nbt);

        assert_eq!(reload(&nbt).patterns(), patterns);
    }

    #[test]
    fn empty_patterns_are_omitted_and_load_back_as_empty() {
        let mut nbt = NbtCompound::new();
        banner().save_additional(&mut nbt);
        assert!(nbt.get("patterns").is_none());

        assert_eq!(reload(&nbt).patterns(), BannerPatternLayers::empty());
    }
}
