//! Lectern block entity.

use std::mem;
use std::sync::{Arc, Weak};

use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::data_components::vanilla_components::WRITTEN_BOOK_CONTENT;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// A lectern holds exactly one book.
pub const LECTERN_SLOTS: usize = 1;

struct LecternState {
    /// The page the lectern is currently open at.
    page: i32,
    /// Number of pages in the current book, used for the comparator output.
    page_count: i32,
}

/// Vanilla `LecternBlockEntity`.
pub struct LecternBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<LecternContainer>>,
    container_ref: ContainerRef,
    state: SyncMutex<LecternState>,
}

struct LecternContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `LecternBlockEntity`.
unsafe impl DowncastType for LecternBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/lectern");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a lectern block entity.
unsafe impl DowncastType for LecternContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/lectern");
}

impl LecternBlockEntity {
    /// Creates a bookless lectern block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::LECTERN,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(LecternContainer {
            items: vec![ItemStack::empty(); LECTERN_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            state: SyncMutex::new(LecternState {
                page: 0,
                page_count: 1,
            }),
        }
    }

    /// Returns the book currently on the lectern.
    #[must_use]
    pub fn book(&self) -> ItemStack {
        self.container.lock().items[0].clone()
    }

    /// Vanilla `LecternBlockEntity.setBook`: places a book and resets to page one.
    pub fn set_book(&self, book: ItemStack) {
        let page_count = Self::page_count(&book);
        self.container.lock().items[0] = book;
        let mut state = self.state.lock();
        state.page = 0;
        state.page_count = page_count;
        drop(state);
        self.set_changed();
    }

    /// Removes the book from the lectern and returns it.
    #[must_use]
    pub fn take_book(&self) -> ItemStack {
        let book = mem::take(&mut self.container.lock().items[0]);
        let mut state = self.state.lock();
        state.page = 0;
        state.page_count = 1;
        drop(state);
        self.set_changed();
        book
    }

    /// Returns the page the lectern is open at.
    #[must_use]
    pub fn page(&self) -> i32 {
        self.state.lock().page
    }

    /// Vanilla `LecternBlockEntity.setPage`, clamped to the book's page count.
    pub fn set_page(&self, page: i32) {
        let mut state = self.state.lock();
        state.page = page.clamp(0, state.page_count - 1);
        drop(state);
        self.set_changed();
    }

    /// Vanilla `LecternBlockEntity.getRedstoneSignal`.
    ///
    /// Scales the current page across the comparator's 1..=15 range.
    #[must_use]
    pub fn redstone_signal(&self) -> i32 {
        let state = self.state.lock();
        if state.page_count <= 1 {
            return 15;
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "page counts are far below f32's exact integer range"
        )]
        let progress = state.page as f32 / (state.page_count - 1) as f32;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "progress is within 0..=1 so the product is within 0..=14"
        )]
        let scaled = (progress * 14.0).round() as i32;
        scaled + 1
    }

    /// Number of pages in `book`, or one when it carries no written content.
    fn page_count(book: &ItemStack) -> i32 {
        book.get(WRITTEN_BOOK_CONTENT)
            .map_or(1, |content| {
                i32::try_from(content.pages().len()).unwrap_or(1)
            })
            .max(1)
    }
}

impl BlockEntity for LecternBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();

        let book = nbt_view
            .compound("Book")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);
        let page_count = Self::page_count(&book);
        self.container.lock().items[0] = book;

        let mut state = self.state.lock();
        state.page_count = page_count;
        state.page = nbt_view.int("Page").unwrap_or(0).clamp(0, page_count - 1);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let book = self.book();
        if !book.is_empty()
            && let NbtTag::Compound(book_nbt) = book.to_nbt_tag()
        {
            nbt.insert("Book", book_nbt);
            nbt.insert("Page", self.page());
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        // The client renders the open book and its page.
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for LecternContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        LECTERN_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < LECTERN_SLOTS {
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
