//! Brewing stand block entity — fuel + bottle-state + brew-time tracking.
//! Potion transformation (`PotionBrewing`) is stubbed pending SteelExtractor brewing registry.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use simdnbt::ToNbtTag;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::{LevelAccessor, World};

/// Number of slots in a brewing stand (3 bottles + ingredient + fuel).
pub const BREWING_STAND_SLOTS: usize = 5;

const FUEL_SLOT: usize = 4;
#[expect(dead_code, reason = "ingredient slot used in future brewing logic")]
const INGREDIENT_SLOT: usize = 3;

/// Vanilla `BrewingStandBlockEntity` — container for potion brewing.
///
///
/// Fuel (`BLAZE_POWDER` → 20 uses) and `HAS_BOTTLE[3]` block-state sync are
/// implemented. Potion transformation via `PotionBrewing` is stubbed pending
/// SteelExtractor brewing registry; `isBrewable`/`doBrew` are no-ops for now.
pub struct BrewingStandBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<BrewingContainer>>,
    container_ref: ContainerRef,
}

struct BrewingContainer {
    items: Vec<ItemStack>,
    brew_time: i32,
    fuel: i32,
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
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::BREWING_STAND,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(BrewingContainer {
            items: vec![ItemStack::empty(); BREWING_STAND_SLOTS],
            brew_time: 0,
            fuel: 0,
        }));
        let shared: SharedContainer = container.clone();
        let container_ref = ContainerRef::owned_by_block_entity(shared, Arc::clone(&base));
        Self { base, container, container_ref }
    }

    fn is_brewing_fuel(item: &ItemStack) -> bool {
        // Vanilla `ItemTags.BREWING_FUEL` is `blazed_powder` in 26.2
        item.is(&steel_registry::vanilla_items::BLAZE_POWDER)
    }

    fn get_potion_bits(items: &[ItemStack]) -> [bool; 3] {
        let mut bits = [false; 3];
        for i in 0..3 {
            bits[i] = !items[i].is_empty();
        }
        bits
    }
}

impl BlockEntity for BrewingStandBlockEntity {
    fn base(&self) -> &BlockEntityBase { &self.base }

    fn tick(&self, world: &Arc<World>) {
        let pos = self.base.pos();
        let mut c = self.container.lock();
        // Fuel: BLAZE_POWDER → 20 uses, like vanilla `BrewingStandBlockEntity.serverTick`
        if c.fuel <= 0 && Self::is_brewing_fuel(&c.items[FUEL_SLOT]) {
            c.fuel = 20;
            let mut fuel = c.items[FUEL_SLOT].clone();
            fuel.shrink(1);
            c.items[FUEL_SLOT] = fuel;
            drop(c);
            self.set_changed();
            world.block_entity_changed(pos);
            c = self.container.lock();
        }

        // Potion brewing is stubbed: we track brewTime but never complete a brew
        // until `PotionBrewing` registry is generated via SteelExtractor.
        // Keep HAS_BOTTLE sync so comparators/redstone see correct bottle count.
        let bits = Self::get_potion_bits(&c.items);
        drop(c);

        // Sync HAS_BOTTLE[0..2] blockstate, like vanilla tail of serverTick
        let mut state = world.get_block_state(pos);
        let mut changed_state = false;
        for i in 0..3 {
            let prop = match i {
                0 => &steel_registry::blocks::properties::BlockStateProperties::HAS_BOTTLE_0,
                1 => &steel_registry::blocks::properties::BlockStateProperties::HAS_BOTTLE_1,
                _ => &steel_registry::blocks::properties::BlockStateProperties::HAS_BOTTLE_2,
            };
            let has = bits[i];
            if state.try_get_value(prop) != Some(has) {
                state = state.set_value(prop, has);
                changed_state = true;
            }
        }
        if changed_state {
            world.set_block_state(pos, state, steel_utils::types::UpdateFlags::UPDATE_ALL);
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
                        if s < BREWING_STAND_SLOTS {
                            if let Some(item) = ItemStack::from_borrowed_compound(&comp) {
                                c.items[s] = item;
                            }
                        }
                    }
                }
            }
        }
        c.brew_time = view.short("BrewTime").map(|v| v as i32).unwrap_or(0);
        c.fuel = view.byte("Fuel").map(|v| v as i32).unwrap_or(0);
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
        nbt.insert("BrewTime", c.brew_time as i16);
        nbt.insert("Fuel", c.fuel as i8);
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
