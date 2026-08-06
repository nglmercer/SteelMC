//! Abstract furnace block entity — furnace, smoker, blast furnace.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::REGISTRY;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::fuel;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::LevelAccessor;
use crate::world::World;

/// Number of slots in a furnace (input + fuel + result).
pub const FURNACE_SLOTS: usize = 3;
const SLOT_INPUT: usize = 0;
const SLOT_FUEL: usize = 1;
const SLOT_RESULT: usize = 2;

/// Which furnace variant determines recipe type and default cook time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnaceKind {
    /// Regular furnace (200 ticks, smelting recipes).
    Furnace,
    /// Smoker (100 ticks, smoking recipes — food only).
    Smoker,
    /// Blast furnace (100 ticks, blasting recipes — ores/ingots).
    BlastFurnace,
}

impl FurnaceKind {
    fn default_cooking_time(self) -> i32 {
        match self {
            Self::Furnace => 200,
            Self::Smoker | Self::BlastFurnace => 100,
        }
    }
    fn find_recipe_result(self, input: &ItemStack) -> Option<ItemStack> {
        match self {
            Self::Furnace => REGISTRY.recipes.find_smelting_result(input, false),
            Self::Smoker => REGISTRY.recipes.find_smoking_result(input, false),
            Self::BlastFurnace => REGISTRY.recipes.find_blasting_result(input, false),
        }
    }
    fn find_recipe_cooking_time(self, input: &ItemStack) -> Option<i32> {
        match self {
            Self::Furnace => REGISTRY
                .recipes
                .find_smelting_recipe(input)
                .map(|r| r.cooking_time),
            Self::Smoker => REGISTRY
                .recipes
                .find_smoking_recipe(input)
                .map(|r| r.cooking_time),
            Self::BlastFurnace => REGISTRY
                .recipes
                .find_blasting_recipe(input)
                .map(|r| r.cooking_time),
        }
    }
}

/// Shared furnace inventory and burn state (lit time, cooking progress).
pub struct FurnaceContainer {
    /// Stored stacks: input, fuel, result.
    pub items: Vec<ItemStack>,
    /// Remaining burn ticks for current fuel.
    pub lit_time_remaining: i32,
    /// Total burn ticks for current fuel piece.
    pub lit_duration: i32,
    /// Current recipe cooking progress.
    pub cooking_progress: i32,
    /// Total ticks required for the current recipe.
    pub cooking_total_time: i32,
    /// Which furnace variant owns this container.
    pub kind: FurnaceKind,
}

impl FurnaceContainer {
    fn new(kind: FurnaceKind) -> Self {
        Self {
            items: vec![ItemStack::empty(); FURNACE_SLOTS],
            lit_time_remaining: 0,
            lit_duration: 0,
            cooking_progress: 0,
            cooking_total_time: kind.default_cooking_time(),
            kind,
        }
    }
}

/// Vanilla abstract furnace — backs furnace, smoker and blast furnace.
pub struct AbstractFurnaceBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<FurnaceContainer>>,
    container_ref: ContainerRef,
    kind: FurnaceKind,
}

// SAFETY: keys are Steel-owned
unsafe impl DowncastType for AbstractFurnaceBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/furnace");
}
unsafe impl DowncastType for FurnaceContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/furnace");
}

/// Concrete furnace block entity type.
pub type FurnaceBlockEntity = AbstractFurnaceBlockEntity;
/// Concrete smoker block entity type.
pub type SmokerBlockEntity = AbstractFurnaceBlockEntity;
/// Concrete blast furnace block entity type.
pub type BlastFurnaceBlockEntity = AbstractFurnaceBlockEntity;

impl AbstractFurnaceBlockEntity {
    fn new_with_kind(
        block_entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
        kind: FurnaceKind,
    ) -> Self {
        let base = Arc::new(BlockEntityBase::new(block_entity_type, level, pos, state));
        let container = Arc::new(SyncMutex::new(FurnaceContainer::new(kind)));
        let shared: SharedContainer = container.clone();
        let container_ref = ContainerRef::owned_by_block_entity(shared, Arc::clone(&base));
        Self {
            base,
            container,
            container_ref,
            kind,
        }
    }

    /// Creates a furnace entity (normal fuel, smelting recipes).
    pub fn new_furnace(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::new_with_kind(
            &vanilla_block_entity_types::FURNACE,
            level,
            pos,
            state,
            FurnaceKind::Furnace,
        )
    }
    /// Creates a smoker entity (fast cooking, smoking recipes).
    pub fn new_smoker(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::new_with_kind(
            &vanilla_block_entity_types::SMOKER,
            level,
            pos,
            state,
            FurnaceKind::Smoker,
        )
    }
    /// Creates a blast furnace entity (fast cooking, blasting recipes).
    pub fn new_blast_furnace(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::new_with_kind(
            &vanilla_block_entity_types::BLAST_FURNACE,
            level,
            pos,
            state,
            FurnaceKind::BlastFurnace,
        )
    }
    /// Which furnace variant this entity represents.
    pub fn kind(&self) -> FurnaceKind {
        self.kind
    }
    /// Shared furnace container handle.
    pub fn container_arc(&self) -> Arc<SyncMutex<FurnaceContainer>> {
        Arc::clone(&self.container)
    }
}

impl BlockEntity for AbstractFurnaceBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn tick(&self, world: &Arc<World>) {
        let pos = self.base.pos();
        let mut c = self.container.lock();
        let was_lit = c.lit_time_remaining > 0;
        let mut is_lit = was_lit;
        if c.lit_time_remaining > 0 {
            c.lit_time_remaining -= 1;
            is_lit = c.lit_time_remaining > 0;
        }
        let has_input = !c.items[SLOT_INPUT].is_empty();
        let has_fuel = !c.items[SLOT_FUEL].is_empty();
        let recipe_result = if has_input {
            self.kind.find_recipe_result(&c.items[SLOT_INPUT])
        } else {
            None
        };
        let can_burn = recipe_result
            .as_ref()
            .map(|r| can_burn(&c.items, r))
            .unwrap_or(false);
        let mut changed = false;
        if is_lit || (has_fuel && has_input && recipe_result.is_some()) {
            if has_input && recipe_result.is_some() {
                let result = recipe_result.unwrap();
                if can_burn {
                    if !is_lit {
                        let fuel_item = c.items[SLOT_FUEL].item();
                        let burn = fuel::burn_duration(fuel_item);
                        if burn > 0 {
                            c.lit_time_remaining = burn;
                            c.lit_duration = burn;
                            let mut fuel = c.items[SLOT_FUEL].clone();
                            let fi = fuel.item();
                            fuel.shrink(1);
                            if fuel.is_empty() {
                                c.items[SLOT_FUEL] = fi.get_crafting_remainder();
                            } else {
                                c.items[SLOT_FUEL] = fuel;
                            }
                            is_lit = true;
                            changed = true;
                        }
                    }
                    if is_lit {
                        c.cooking_progress += 1;
                        if c.cooking_progress >= c.cooking_total_time {
                            c.cooking_progress = 0;
                            let inp = c.items[SLOT_INPUT].clone();
                            if let Some(t) = self.kind.find_recipe_cooking_time(&inp) {
                                c.cooking_total_time = t;
                            }
                            let inp2 = c.items[SLOT_INPUT].clone();
                            burn_items(&mut c.items, &inp2, &result);
                            changed = true;
                        }
                    } else {
                        c.cooking_progress = 0;
                    }
                } else {
                    c.cooking_progress = 0;
                }
            } else {
                c.cooking_progress = 0;
            }
        } else if c.cooking_progress > 0 {
            c.cooking_progress = (c.cooking_progress - 2).max(0).min(c.cooking_total_time);
        }
        if !has_input {
        } else if c.cooking_progress == 0 {
            let inp = c.items[SLOT_INPUT].clone();
            if let Some(t) = self.kind.find_recipe_cooking_time(&inp) {
                c.cooking_total_time = t;
            } else {
                c.cooking_total_time = self.kind.default_cooking_time();
            }
        }
        if was_lit != is_lit {
            changed = true;
            let old_state = world.get_block_state(pos);
            let new_state = old_state.set_value(&BlockStateProperties::LIT, is_lit);
            drop(c);
            world.set_block_state(pos, new_state, steel_utils::types::UpdateFlags::UPDATE_ALL);
            if changed {
                if let Some(be) = world.get_block_entity(pos) {
                    be.set_changed();
                } else {
                    world.block_entity_changed(pos);
                }
            }
            return;
        }
        drop(c);
        if changed {
            self.set_changed();
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
                        if s < FURNACE_SLOTS {
                            if let Some(item) = ItemStack::from_borrowed_compound(&comp) {
                                c.items[s] = item;
                            }
                        }
                    }
                }
            }
        }
        c.lit_time_remaining = view
            .short("lit_time_remaining")
            .map(|v| v as i32)
            .unwrap_or(0);
        c.lit_duration = view.short("lit_total_time").map(|v| v as i32).unwrap_or(0);
        c.cooking_progress = view
            .short("cooking_time_spent")
            .map(|v| v as i32)
            .unwrap_or(0);
        c.cooking_total_time = view
            .short("cooking_total_time")
            .map(|v| v as i32)
            .unwrap_or(self.kind.default_cooking_time());
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
        nbt.insert("lit_time_remaining", c.lit_time_remaining as i16);
        nbt.insert("lit_total_time", c.lit_duration as i16);
        nbt.insert("cooking_time_spent", c.cooking_progress as i16);
        nbt.insert("cooking_total_time", c.cooking_total_time as i16);
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut c = self.container.lock();
            mem::replace(&mut c.items, vec![ItemStack::empty(); FURNACE_SLOTS])
        };
        let Some(world) = self.get_level() else {
            return;
        };
        for item in items {
            if !item.is_empty() {
                world.drop_item_stack(pos, item);
            }
        }
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }
}

fn can_burn(items: &[ItemStack], result: &ItemStack) -> bool {
    let out = &items[SLOT_RESULT];
    if out.is_empty() {
        return true;
    }
    if !ItemStack::is_same_item_same_components(out, result) {
        return false;
    }
    let total = out.count() + result.count();
    let max = out.max_stack_size().min(result.max_stack_size());
    total <= max
}

fn burn_items(items: &mut [ItemStack], input: &ItemStack, result: &ItemStack) {
    if items[SLOT_RESULT].is_empty() {
        items[SLOT_RESULT] = result.clone();
    } else {
        items[SLOT_RESULT].grow(result.count());
    }
    if input.is(&steel_registry::vanilla_items::WET_SPONGE)
        && !items[SLOT_FUEL].is_empty()
        && items[SLOT_FUEL].is(&steel_registry::vanilla_items::BUCKET)
    {
        items[SLOT_FUEL] = ItemStack::new(&steel_registry::vanilla_items::WATER_BUCKET);
    }
    items[SLOT_INPUT].shrink(1);
}

impl Container for FurnaceContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }
    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }
    fn get_container_size(&self) -> usize {
        FURNACE_SLOTS
    }
    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < FURNACE_SLOTS {
            if !stack.is_empty() && stack.count() > stack.max_stack_size() {
                stack.set_count(stack.max_stack_size());
            }
            let old = self.items[slot].clone();
            let same = !stack.is_empty() && ItemStack::is_same_item_same_components(&stack, &old);
            self.items[slot] = stack;
            if slot == SLOT_INPUT && !same {
                self.cooking_progress = 0;
                let inp = self.items[SLOT_INPUT].clone();
                if !inp.is_empty() {
                    if let Some(t) = self.kind.find_recipe_cooking_time(&inp) {
                        self.cooking_total_time = t;
                    }
                }
            }
        }
    }
    fn get_max_stack_size(&self) -> i32 {
        64
    }
    fn set_changed(&mut self) {}
}
