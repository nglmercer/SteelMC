//! Dispenser and dropper block entity.

use std::{
    mem,
    sync::{Arc, Weak},
};

use rand::RngExt as _;
use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;
use steel_registry::data_components::DataComponentPatch;
use steel_registry::data_components::vanilla_components::CONTAINER;

/// Vanilla dispensers and droppers both hold nine slots.
pub const DISPENSER_SLOTS: usize = 9;

/// Vanilla `DispenserBlockEntity`, also used for droppers.
pub struct DispenserBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<DispenserContainer>>,
    container_ref: ContainerRef,
}

struct DispenserContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `DispenserBlockEntity`.
unsafe impl DowncastType for DispenserBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/dispenser");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a dispenser block entity.
unsafe impl DowncastType for DispenserContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/dispenser");
}

impl DispenserBlockEntity {
    /// Creates an empty dispenser or dropper block entity.
    #[must_use]
    pub fn new(
        block_entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        let base = Arc::new(BlockEntityBase::new(block_entity_type, level, pos, state));
        let container = Arc::new(SyncMutex::new(DispenserContainer {
            items: vec![ItemStack::empty(); DISPENSER_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
        }
    }

    /// Vanilla `DispenserBlockEntity.getRandomSlot`: picks one non-empty slot uniformly.
    ///
    /// Returns `None` when the dispenser is empty.
    #[must_use]
    pub fn random_occupied_slot(&self) -> Option<usize> {
        let container = self.container.lock();
        let mut chosen = None;
        let mut seen = 0;
        for (slot, item) in container.items.iter().enumerate() {
            if item.is_empty() {
                continue;
            }
            seen += 1;
            // Reservoir sampling of size one, matching vanilla's `nextInt(count) == 0`.
            if rand::rng().random_range(0..seen) == 0 {
                chosen = Some(slot);
            }
        }
        chosen
    }

    /// Returns a copy of the stack in `slot`.
    #[must_use]
    pub fn item(&self, slot: usize) -> ItemStack {
        self.container
            .lock()
            .items
            .get(slot)
            .cloned()
            .unwrap_or_else(ItemStack::empty)
    }

    /// Replaces the stack in `slot`.
    pub fn set_item(&self, slot: usize, stack: ItemStack) {
        if slot < DISPENSER_SLOTS {
            self.container.lock().items[slot] = stack;
            self.set_changed();
        }
    }
}

impl BlockEntity for DispenserBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(
                &mut container.items,
                vec![ItemStack::empty(); DISPENSER_SLOTS],
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
                    if slot < DISPENSER_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }
    }

    fn collect_implicit_components(&self, patch: &mut DataComponentPatch) {
        // Vanilla `BaseContainerBlockEntity.collectImplicitComponents`.
        if let Some(contents) =
            crate::block_entity::container_contents_component(&self.container.lock().items)
        {
            patch.set(CONTAINER, contents);
        }
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
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for DispenserContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        DISPENSER_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < DISPENSER_SLOTS {
            let max_stack_size = self.get_max_stack_size_for_item(&stack);
            if !stack.is_empty() && stack.count() > max_stack_size {
                stack.set_count(max_stack_size);
            }
            self.items[slot] = stack;
        }
    }

    fn get_max_stack_size(&self) -> i32 {
        64
    }

    fn set_changed(&mut self) {}
}
