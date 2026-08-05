//! Chiseled bookshelf block entity.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Vanilla `ChiseledBookShelfBlock.MAX_BOOKS_IN_STORAGE`.
pub const CHISELED_BOOKSHELF_SLOTS: usize = 6;

/// Vanilla's `lastInteractedSlot` value before any interaction.
const NO_LAST_SLOT: i32 = -1;

/// Vanilla `ChiseledBookShelfBlockEntity`.
pub struct ChiseledBookShelfBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<ChiseledBookShelfContainer>>,
    container_ref: ContainerRef,
    /// Vanilla's `lastInteractedSlot`, which drives the comparator output.
    last_interacted_slot: SyncMutex<i32>,
}

struct ChiseledBookShelfContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ChiseledBookShelfBlockEntity`.
unsafe impl DowncastType for ChiseledBookShelfBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/chiseled_bookshelf");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a chiseled bookshelf block entity.
unsafe impl DowncastType for ChiseledBookShelfContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/chiseled_bookshelf");
}

impl ChiseledBookShelfBlockEntity {
    /// Creates an empty chiseled bookshelf block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::CHISELED_BOOKSHELF,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(ChiseledBookShelfContainer {
            items: vec![ItemStack::empty(); CHISELED_BOOKSHELF_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            last_interacted_slot: SyncMutex::new(NO_LAST_SLOT),
        }
    }

    /// Vanilla `ChiseledBookShelfBlockEntity.getLastInteractedSlot`.
    ///
    /// `-1` until a player has put a book in or taken one out.
    #[must_use]
    pub fn last_interacted_slot(&self) -> i32 {
        *self.last_interacted_slot.lock()
    }

    /// Puts a book into `slot`, replacing whatever was there.
    pub fn set_book(&self, slot: usize, stack: ItemStack) {
        if slot >= CHISELED_BOOKSHELF_SLOTS {
            return;
        }
        self.container.lock().items[slot] = stack;
        self.record_interaction(slot);
        self.set_changed();
    }

    /// Records which slot a player last touched, for the comparator output.
    fn record_interaction(&self, slot: usize) {
        *self.last_interacted_slot.lock() = i32::try_from(slot).unwrap_or(NO_LAST_SLOT);
    }

    /// Takes the book out of `slot`, leaving it empty.
    #[must_use]
    pub fn take_book(&self, slot: usize) -> ItemStack {
        if slot >= CHISELED_BOOKSHELF_SLOTS {
            return ItemStack::empty();
        }
        let taken = mem::take(&mut self.container.lock().items[slot]);
        self.record_interaction(slot);
        self.set_changed();
        taken
    }
}

impl BlockEntity for ChiseledBookShelfBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(
                &mut container.items,
                vec![ItemStack::empty(); CHISELED_BOOKSHELF_SLOTS],
            )
        };
        let Some(world) = self.get_level() else {
            return;
        };
        for item in items {
            world.drop_item_stack(pos, item);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());

        if let Some(items_list) = nbt_view.list("Items")
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < CHISELED_BOOKSHELF_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }
        drop(container);

        *self.last_interacted_slot.lock() =
            nbt_view.int("last_interacted_slot").unwrap_or(NO_LAST_SLOT);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let container = self.container.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in container.items.iter().enumerate() {
            if !item.is_empty()
                && let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag()
            {
                item_nbt.insert("Slot", slot as i8);
                items.push(item_nbt);
            }
        }
        nbt.insert("Items", NbtList::Compound(items));
        nbt.insert("last_interacted_slot", self.last_interacted_slot());
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for ChiseledBookShelfContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        CHISELED_BOOKSHELF_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < CHISELED_BOOKSHELF_SLOTS {
            // Vanilla stores exactly one book per slot.
            if !stack.is_empty() && stack.count() > 1 {
                stack.set_count(1);
            }
            self.items[slot] = stack;
        }
    }

    fn get_max_stack_size(&self) -> i32 {
        1
    }

    fn set_changed(&mut self) {}
}
