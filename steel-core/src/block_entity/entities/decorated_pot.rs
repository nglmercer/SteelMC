//! Decorated pot block entity.

use std::sync::{Arc, Weak};

use simdnbt::FromNbtTag as _;
use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::data_components::components::PotDecorations;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;
use steel_registry::data_components::DataComponentPatch;
use steel_registry::data_components::vanilla_components::{CONTAINER, POT_DECORATIONS};

/// Vanilla's decorated pot holds exactly one stack.
pub const DECORATED_POT_SLOTS: usize = 1;

/// Vanilla `DecoratedPotBlockEntity`.
///
/// Vanilla's wobble animation is a client-side block event with no server-side state.
pub struct DecoratedPotBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<DecoratedPotContainer>>,
    container_ref: ContainerRef,
    decorations: SyncMutex<PotDecorations>,
}

struct DecoratedPotContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `DecoratedPotBlockEntity`.
unsafe impl DowncastType for DecoratedPotBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/decorated_pot");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a decorated pot block entity.
unsafe impl DowncastType for DecoratedPotContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/decorated_pot");
}

impl DecoratedPotBlockEntity {
    /// Creates an empty, undecorated pot block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::DECORATED_POT,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(DecoratedPotContainer {
            items: vec![ItemStack::empty(); DECORATED_POT_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            decorations: SyncMutex::new(PotDecorations::EMPTY),
        }
    }

    /// Returns the pot's four sherd faces.
    #[must_use]
    pub fn decorations(&self) -> PotDecorations {
        self.decorations.lock().clone()
    }

    /// Replaces the pot's sherd faces.
    pub fn set_decorations(&self, decorations: PotDecorations) {
        *self.decorations.lock() = decorations;
        self.set_changed();
    }

    /// Vanilla `ContainerSingleItem.getTheItem`.
    #[must_use]
    pub fn the_item(&self) -> ItemStack {
        self.container.lock().items[0].clone()
    }

    /// Vanilla `ContainerSingleItem.setTheItem`.
    pub fn set_the_item(&self, stack: ItemStack) {
        self.container.lock().items[0] = stack;
        self.set_changed();
    }
}

impl BlockEntity for DecoratedPotBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();

        *self.decorations.lock() = nbt_view
            .get("sherds")
            .and_then(PotDecorations::from_nbt_tag)
            .unwrap_or(PotDecorations::EMPTY);

        let item = nbt_view
            .compound("item")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);
        self.container.lock().items[0] = item;
    }

    fn collect_implicit_components(&self, patch: &mut DataComponentPatch) {
        // Vanilla `DecoratedPotBlockEntity.collectImplicitComponents`.
        patch.set(POT_DECORATIONS, self.decorations());
        if let Some(contents) =
            crate::block_entity::container_contents_component(&[self.the_item()])
        {
            patch.set(CONTAINER, contents);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let decorations = self.decorations();
        if decorations != PotDecorations::EMPTY {
            nbt.insert("sherds", decorations.to_nbt_tag());
        }

        let item = self.the_item();
        if !item.is_empty()
            && let NbtTag::Compound(item_nbt) = item.to_nbt_tag()
        {
            nbt.insert("item", item_nbt);
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        // The client renders the sherd faces, so they ship with the chunk.
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for DecoratedPotContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        DECORATED_POT_SLOTS
    }

    fn set_item(&mut self, slot: usize, stack: ItemStack) {
        if slot < DECORATED_POT_SLOTS {
            self.items[slot] = stack;
        }
    }

    fn get_max_stack_size(&self) -> i32 {
        64
    }

    fn set_changed(&mut self) {}
}
