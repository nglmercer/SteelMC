//! Brewing stand block entity — minimal container, no brewing logic yet.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use simdnbt::ToNbtTag;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Number of slots in a brewing stand (3 bottles + ingredient + fuel).
pub const BREWING_STAND_SLOTS: usize = 5;

/// Vanilla `BrewingStandBlockEntity` — container for potion brewing (logic TODO).
pub struct BrewingStandBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<BrewingContainer>>,
    container_ref: ContainerRef,
}

struct BrewingContainer {
    items: Vec<ItemStack>,
}

// SAFETY: Steel-owned
unsafe impl DowncastType for BrewingStandBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/brewing_stand");
}
unsafe impl DowncastType for BrewingContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/brewing_stand");
}

impl BrewingStandBlockEntity {
    /// Creates a new brewing stand block entity with empty slots.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(&vanilla_block_entity_types::BREWING_STAND, level, pos, state));
        let container = Arc::new(SyncMutex::new(BrewingContainer { items: vec![ItemStack::empty(); BREWING_STAND_SLOTS] }));
        let shared: SharedContainer = container.clone();
        let container_ref = ContainerRef::owned_by_block_entity(shared, Arc::clone(&base));
        Self { base, container, container_ref }
    }
}

impl BlockEntity for BrewingStandBlockEntity {
    fn base(&self) -> &BlockEntityBase { &self.base }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let mut c = self.container.lock();
        c.items.fill(ItemStack::empty());
        if let Some(list) = view.list("Items") {
            if let Some(comps) = list.compounds() {
                for comp in comps {
                    if let Some(slot) = comp.byte("Slot") {
                        let s = slot as usize;
                        if s < BREWING_STAND_SLOTS {
                            if let Some(item) = ItemStack::from_borrowed_compound(&comp) { c.items[s] = item; }
                        }
                    }
                }
            }
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let c = self.container.lock();
        let mut list: Vec<NbtCompound> = Vec::new();
        for (slot, item) in c.items.iter().enumerate() {
            if !item.is_empty() {
                if let NbtTag::Compound(mut comp) = item.clone().to_nbt_tag() {
                    comp.insert("Slot", slot as i8);
                    list.push(comp);
                }
            }
        }
        nbt.insert("Items", NbtList::Compound(list));
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut c = self.container.lock();
            mem::replace(&mut c.items, vec![ItemStack::empty(); BREWING_STAND_SLOTS])
        };
        let Some(world) = self.get_level() else { return; };
        for item in items { if !item.is_empty() { world.drop_item_stack(pos, item); } }
    }

    fn container_ref(&self) -> Option<ContainerRef> { Some(self.container_ref.clone()) }
    fn get_update_tag(&self) -> Option<NbtCompound> { None }
}

impl Container for BrewingContainer {
    fn items(&self) -> &[ItemStack] { &self.items }
    fn items_mut(&mut self) -> &mut [ItemStack] { &mut self.items }
    fn get_container_size(&self) -> usize { BREWING_STAND_SLOTS }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < BREWING_STAND_SLOTS {
            if !stack.is_empty() && stack.count() > stack.max_stack_size() { stack.set_count(stack.max_stack_size()); }
            self.items[slot] = stack;
        }
    }
    fn get_max_stack_size(&self) -> i32 { 64 }
    fn set_changed(&mut self) {}
}
