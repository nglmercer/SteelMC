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
            cooking_total: [0; CAMPFIRE_SLOTS],
        }));
        let shared: SharedContainer = container.clone();
        let container_ref = ContainerRef::owned_by_block_entity(shared, Arc::clone(&base));
        Self { base, container, container_ref }
    }
}

impl CampfireBlockEntity {
    /// Attempts to place one item from `stack` onto an empty slot.
    ///
    /// Mirrors `CampfireBlockEntity.placeFood` — validates campfire recipe,
    /// consumes one item, initializes cooking progress/total, and notifies.
    pub fn place_food(&self, world: &Arc<World>, stack: &mut ItemStack) -> bool {
        if stack.is_empty() {
            return false;
        }
        let Some(recipe) = REGISTRY.recipes.find_campfire_recipe(stack) else {
            return false;
        };
        let mut c = self.container.lock();
        for i in 0..CAMPFIRE_SLOTS {
            if c.items[i].is_empty() {
                c.cooking_total[i] = recipe.cooking_time;
                c.cooking_progress[i] = 0;
                let taken = stack.split(1);
                // Preserve single-item stack semantics; `split` handles count.
                c.items[i] = taken;
                drop(c);
                let pos = self.base.pos();
                let state = world.get_block_state(pos);
                world.game_event(
                    &steel_registry::vanilla_game_events::BLOCK_CHANGE,
                    pos,
                    &crate::world::game_event::GameEventContext::new(None, Some(state)),
                );
                self.set_changed();
                // Notify clients of BE change (vanilla sends BlockUpdated).
                world.block_entity_changed(pos);
                return true;
            }
        }
        false
    }
}

impl BlockEntity for CampfireBlockEntity {
    fn base(&self) -> &BlockEntityBase { &self.base }

    fn tick(&self, world: &Arc<World>) {
        let pos = self.base.pos();
        let state = world.get_block_state(pos);
        let lit = state.get_value(&BlockStateProperties::LIT);
        if lit {
            // Cook tick — mirrors `CampfireBlockEntity.cookTick`
            let mut c = self.container.lock();
            let mut changed = false;
            for i in 0..CAMPFIRE_SLOTS {
                let stack = c.items[i].clone();
                if stack.is_empty() {
                    if c.cooking_progress[i] != 0 {
                        changed = true;
                    }
                    continue;
                }
                changed = true;
                c.cooking_progress[i] += 1;
                if c.cooking_progress[i] >= c.cooking_total[i] {
                    // Assemble result using current recipe; drop as entity like vanilla.
                    let result = REGISTRY
                        .recipes
                        .find_campfire_recipe(&stack)
                        .map(|r| r.assemble(&stack))
                        .unwrap_or(stack.clone());
                    // Drop result at block position (vanilla uses Containers.dropItemStack)
                    drop(c);
                    world.drop_item_stack(pos, result);
                    c = self.container.lock();
                    c.items[i] = ItemStack::empty();
                    c.cooking_progress[i] = 0;
                    // Reset total to default to avoid stale value on next placement
                    // (vanilla keeps per-slot time until next placement).
                    changed = true;
                }
            }
            let has_progress = changed;
            drop(c);
            if has_progress {
                // Vanilla: setChanged + sendBlockUpdated + gameEvent
                world.game_event(
                    &steel_registry::vanilla_game_events::BLOCK_CHANGE,
                    pos,
                    &crate::world::game_event::GameEventContext::new(None, Some(state)),
                );
                world.block_entity_changed(pos);
                self.set_changed();
            }
        } else {
            // Cooldown tick — mirrors `CampfireBlockEntity.cooldownTick`
            let mut c = self.container.lock();
            let mut changed = false;
            for i in 0..CAMPFIRE_SLOTS {
                if c.cooking_progress[i] > 0 {
                    changed = true;
                    let total = c.cooking_total[i].max(1);
                    c.cooking_progress[i] = (c.cooking_progress[i] - 2).clamp(0, total);
                }
            }
            drop(c);
            if changed {
                self.set_changed();
                world.block_entity_changed(pos);
            }
        }
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
        // Vanilla: int arrays "CookingTimes" / "CookingTotalTimes"
        if let Some(arr) = view.int_array("CookingTimes") {
            for i in 0..CAMPFIRE_SLOTS.min(arr.len()) {
                c.cooking_progress[i] = arr[i];
            }
            for i in arr.len()..CAMPFIRE_SLOTS {
                c.cooking_progress[i] = 0;
            }
        } else {
            c.cooking_progress = [0; CAMPFIRE_SLOTS];
        }
        if let Some(arr) = view.int_array("CookingTotalTimes") {
            for i in 0..CAMPFIRE_SLOTS.min(arr.len()) {
                c.cooking_total[i] = arr[i];
            }
            for i in arr.len()..CAMPFIRE_SLOTS {
                c.cooking_total[i] = 0;
            }
        } else {
            c.cooking_total = [0; CAMPFIRE_SLOTS];
        }
        // Back-compat: legacy per-slot shorts
        for i in 0..CAMPFIRE_SLOTS {
            if let Some(v) = view.short(&format!("CookingTime{i}")) {
                c.cooking_progress[i] = v as i32;
            }
            if let Some(v) = view.short(&format!("CookingTotalTime{i}")) {
                c.cooking_total[i] = v as i32;
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
        nbt.insert("CookingTimes", NbtTag::IntArray(c.cooking_progress.to_vec()));
        nbt.insert("CookingTotalTimes", NbtTag::IntArray(c.cooking_total.to_vec()));
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
