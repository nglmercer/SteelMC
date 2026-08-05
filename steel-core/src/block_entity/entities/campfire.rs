//! Campfire block entity — 4-slot cooking when lit.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use simdnbt::ToNbtTag;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_registry::REGISTRY;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Number of slots on a campfire (up to 4 items can cook simultaneously).
pub const CAMPFIRE_SLOTS: usize = 4;

/// Vanilla `CampfireBlockEntity` — cooks up to 4 items when lit.
pub struct CampfireBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<CampfireContainer>>,
    container_ref: ContainerRef,
}

struct CampfireContainer {
    items: Vec<ItemStack>,
    cooking_progress: [i32; CAMPFIRE_SLOTS],
    cooking_total: [i32; CAMPFIRE_SLOTS],
}

unsafe impl DowncastType for CampfireBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/campfire");
}
unsafe impl DowncastType for CampfireContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/campfire");
}

impl CampfireBlockEntity {
    /// Creates a new campfire block entity with empty slots.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(&vanilla_block_entity_types::CAMPFIRE, level, pos, state));
        let container = Arc::new(SyncMutex::new(CampfireContainer {
            items: vec![ItemStack::empty(); CAMPFIRE_SLOTS],
            cooking_progress: [0; CAMPFIRE_SLOTS],
            cooking_total: [600; CAMPFIRE_SLOTS],
        }));
        let shared: SharedContainer = container.clone();
        let container_ref = ContainerRef::owned_by_block_entity(shared, Arc::clone(&base));
        Self { base, container, container_ref }
    }
}

impl BlockEntity for CampfireBlockEntity {
    fn base(&self) -> &BlockEntityBase { &self.base }

    fn tick(&self, world: &Arc<World>) {
        let pos = self.base.pos();
        let state = world.get_block_state(pos);
        if !state.get_value(&BlockStateProperties::LIT) {
            return;
        }
        let mut c = self.container.lock();
        for i in 0..CAMPFIRE_SLOTS {
            let stack = c.items[i].clone();
            if stack.is_empty() {
                c.cooking_progress[i] = 0;
                continue;
            }
            if let Some(recipe) = REGISTRY.recipes.find_campfire_recipe(&stack) {
                c.cooking_total[i] = recipe.cooking_time;
                c.cooking_progress[i] += 1;
                if c.cooking_progress[i] >= c.cooking_total[i] {
                    c.cooking_progress[i] = 0;
                    let result = recipe.assemble(&stack);
                    c.items[i] = result;
                    c.cooking_total[i] = 600;
                }
            } else {
                c.cooking_progress[i] = 0;
            }
        }
        drop(c);
        self.set_changed();
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let view: NbtCompoundView<'_, '_> = nbt.into();
        let mut c = self.container.lock();
        c.items.fill(ItemStack::empty());
        if let Some(list) = view.list("Items") {
            if let Some(comps) = list.compounds() {
                for comp in comps {
                    if let Some(slot) = comp.byte("Slot") {
                        let s = slot as usize;
                        if s < CAMPFIRE_SLOTS {
                            if let Some(item) = ItemStack::from_borrowed_compound(&comp) { c.items[s] = item; }
                        }
                    }
                }
            }
        }
        for i in 0..CAMPFIRE_SLOTS {
            c.cooking_progress[i] = view.short(&format!("CookingTime{i}")).map(|v| v as i32).unwrap_or(0);
            c.cooking_total[i] = view.short(&format!("CookingTotalTime{i}")).map(|v| v as i32).unwrap_or(600);
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
        for i in 0..CAMPFIRE_SLOTS {
            nbt.insert(format!("CookingTime{i}").as_str(), c.cooking_progress[i] as i16);
            nbt.insert(format!("CookingTotalTime{i}").as_str(), c.cooking_total[i] as i16);
        }
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut c = self.container.lock();
            mem::replace(&mut c.items, vec![ItemStack::empty(); CAMPFIRE_SLOTS])
        };
        let Some(world) = self.get_level() else { return; };
        for item in items { if !item.is_empty() { world.drop_item_stack(pos, item); } }
    }

    fn container_ref(&self) -> Option<ContainerRef> { Some(self.container_ref.clone()) }
    fn get_update_tag(&self) -> Option<NbtCompound> { None }
}

impl Container for CampfireContainer {
    fn items(&self) -> &[ItemStack] { &self.items }
    fn items_mut(&mut self) -> &mut [ItemStack] { &mut self.items }
    fn get_container_size(&self) -> usize { CAMPFIRE_SLOTS }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < CAMPFIRE_SLOTS {
            if !stack.is_empty() && stack.count() > stack.max_stack_size() { stack.set_count(stack.max_stack_size()); }
            self.items[slot] = stack;
            self.cooking_progress[slot] = 0;
        }
    }
    fn get_max_stack_size(&self) -> i32 { 64 }
    fn set_changed(&mut self) {}
}
