//! Crafter block entity.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::item_stack::ItemStack;
use steel_registry::recipe::CraftingInput;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// A crafter holds a three-by-three grid.
pub const CRAFTER_SLOTS: usize = 9;
/// Width and height of the crafter's grid.
const CRAFTER_GRID: usize = 3;

struct CrafterState {
    /// Ticks left in the crafting animation.
    crafting_ticks_remaining: i32,
    /// Whether a redstone pulse has armed the crafter.
    triggered: bool,
}

/// Vanilla `CrafterBlockEntity`.
///
/// Vanilla lets a player click a slot in the crafter menu to disable it so the recipe
/// leaves that slot empty; that needs the crafter menu's slot-toggle packet, so every
/// slot is currently enabled.
pub struct CrafterBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<CrafterContainer>>,
    container_ref: ContainerRef,
    state: SyncMutex<CrafterState>,
}

struct CrafterContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CrafterBlockEntity`.
unsafe impl DowncastType for CrafterBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/crafter");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a crafter block entity.
unsafe impl DowncastType for CrafterContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/crafter");
}

impl CrafterBlockEntity {
    /// Creates an empty crafter block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::CRAFTER,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(CrafterContainer {
            items: vec![ItemStack::empty(); CRAFTER_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            state: SyncMutex::new(CrafterState {
                crafting_ticks_remaining: 0,
                triggered: false,
            }),
        }
    }

    /// Vanilla `CrafterBlockEntity.asCraftInput`.
    #[must_use]
    pub fn as_craft_input(&self) -> CraftingInput {
        CraftingInput::new(
            CRAFTER_GRID,
            CRAFTER_GRID,
            self.container.lock().items.clone(),
        )
    }

    /// Marks the crafter as armed by redstone.
    pub fn set_triggered(&self, triggered: bool) {
        self.state.lock().triggered = triggered;
    }

    /// Starts the crafting animation.
    pub fn set_crafting_ticks_remaining(&self, ticks: i32) {
        self.state.lock().crafting_ticks_remaining = ticks;
    }

    /// Consumes one item from every occupied slot, as vanilla does after a craft.
    pub fn consume_ingredients(&self) {
        let mut container = self.container.lock();
        for item in &mut container.items {
            if !item.is_empty() {
                item.set_count(item.count() - 1);
            }
        }
        drop(container);
        self.set_changed();
    }

    /// Vanilla `CrafterBlockEntity.getRedstoneSignal`: one per filled slot.
    #[must_use]
    pub fn redstone_signal(&self) -> i32 {
        let container = self.container.lock();
        container
            .items
            .iter()
            .filter(|item| !item.is_empty() && item.count() >= item.max_stack_size())
            .count()
            .try_into()
            .unwrap_or(0)
    }
}

impl BlockEntity for CrafterBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `CrafterBlockEntity.serverTick`: runs down the crafting animation.
    fn tick(&self, _world: &Arc<World>) {
        let mut state = self.state.lock();
        if state.crafting_ticks_remaining > 0 {
            state.crafting_ticks_remaining -= 1;
        }
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(
                &mut container.items,
                vec![ItemStack::empty(); CRAFTER_SLOTS],
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
                    if slot < CRAFTER_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }
        drop(container);

        let mut state = self.state.lock();
        state.crafting_ticks_remaining = nbt_view.int("crafting_ticks_remaining").unwrap_or(0);
        state.triggered = nbt_view.byte("triggered").unwrap_or(0) != 0;
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
        drop(container);
        nbt.insert("Items", NbtList::Compound(items));

        let state = self.state.lock();
        nbt.insert("crafting_ticks_remaining", state.crafting_ticks_remaining);
        nbt.insert("triggered", i8::from(state.triggered));
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for CrafterContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        CRAFTER_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < CRAFTER_SLOTS {
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
