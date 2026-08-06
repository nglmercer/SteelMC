//! Hopper block entity.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{
    BlockPos, BlockStateId, Direction, DowncastType, DowncastTypeKey, locks::SyncMutex,
};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::{Container, add_item, container_at};
use crate::inventory::lock::{ContainerLockGuard, ContainerRef, SharedContainer};
use crate::world::World;
use steel_registry::data_components::DataComponentPatch;
use steel_registry::data_components::vanilla_components::CONTAINER;

/// Vanilla hoppers hold five slots.
pub const HOPPER_SLOTS: usize = 5;
/// Vanilla `HopperBlockEntity.MOVE_ITEM_SPEED`: ticks between transfers.
const MOVE_ITEM_SPEED: i32 = 8;
/// Vanilla `HopperBlockEntity.NO_COOLDOWN_TIME`.
const NO_COOLDOWN_TIME: i32 = -1;

/// Vanilla `HopperBlockEntity`.
///
/// Vanilla also vacuums up `ItemEntity`s floating above the hopper; that path needs the
/// item-entity pickup rules and is not implemented here yet.
pub struct HopperBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<HopperContainer>>,
    container_ref: ContainerRef,
    cooldown: SyncMutex<i32>,
}

struct HopperContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `HopperBlockEntity`.
unsafe impl DowncastType for HopperBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/hopper");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a hopper block entity.
unsafe impl DowncastType for HopperContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/hopper");
}

impl HopperBlockEntity {
    /// Creates an empty hopper block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::HOPPER,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(HopperContainer {
            items: vec![ItemStack::empty(); HOPPER_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            cooldown: SyncMutex::new(NO_COOLDOWN_TIME),
        }
    }

    fn is_on_cooldown(&self) -> bool {
        *self.cooldown.lock() > 0
    }

    fn set_cooldown(&self, ticks: i32) {
        *self.cooldown.lock() = ticks;
    }

    fn is_empty(&self) -> bool {
        self.container.lock().items.iter().all(ItemStack::is_empty)
    }

    /// Vanilla `HopperBlockEntity.inventoryFull`.
    fn is_full(&self) -> bool {
        self.container
            .lock()
            .items
            .iter()
            .all(|item| !item.is_empty() && item.count() == item.max_stack_size())
    }

    /// Vanilla `HopperBlockEntity.ejectItems`: push one item into the container ahead.
    fn eject_items(&self, world: &Arc<World>, pos: BlockPos, facing: Direction) -> bool {
        let Some(target) = container_at(world, pos.relative(facing)) else {
            return false;
        };

        for slot in 0..HOPPER_SLOTS {
            let item = self.container.lock().items[slot].clone();
            if item.is_empty() {
                continue;
            }

            let mut single = item.clone();
            single.set_count(1);
            if add_item(&target, single).is_empty() {
                let mut container = self.container.lock();
                let mut remaining = item;
                remaining.set_count(remaining.count() - 1);
                container.items[slot] = remaining;
                drop(container);
                self.set_changed();
                return true;
            }
        }

        false
    }

    /// Vanilla `HopperBlockEntity.suckInItems`: pull one item from the container above.
    fn suck_in_items(&self, world: &Arc<World>, pos: BlockPos) -> bool {
        let Some(source) = container_at(world, pos.above()) else {
            return false;
        };

        let taken = {
            let mut guard = ContainerLockGuard::lock_all(&[&source]);
            let Some(container) = guard.get_mut(source.container_id()) else {
                return false;
            };

            let mut taken = ItemStack::empty();
            for slot in 0..container.get_container_size() {
                let item = container.get_item(slot).clone();
                if item.is_empty() {
                    continue;
                }

                let mut single = item.clone();
                single.set_count(1);
                let mut remaining = item;
                remaining.set_count(remaining.count() - 1);
                container.set_item(slot, remaining);
                taken = single;
                break;
            }
            taken
        };

        if taken.is_empty() {
            return false;
        }

        let leftover = add_item(&self.container_ref, taken);
        if leftover.is_empty() {
            self.set_changed();
            return true;
        }

        // The hopper could not take it after all; hand it back to the source.
        let _ = add_item(&source, leftover);
        false
    }
}

impl BlockEntity for HopperBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `HopperBlockEntity.pushItemsTick`.
    fn tick(&self, world: &Arc<World>) {
        {
            let mut cooldown = self.cooldown.lock();
            *cooldown -= 1;
        }
        if self.is_on_cooldown() {
            return;
        }
        self.set_cooldown(0);

        let pos = self.get_block_pos();
        let state = world.get_block_state(pos);
        if !state.get_value(&BlockStateProperties::ENABLED) {
            return;
        }

        let facing = state.get_value(&BlockStateProperties::FACING_HOPPER);

        let mut changed = false;
        if !self.is_empty() {
            changed = self.eject_items(world, pos, facing);
        }
        if !self.is_full() {
            changed |= self.suck_in_items(world, pos);
        }

        if changed {
            self.set_cooldown(MOVE_ITEM_SPEED);
        }
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); HOPPER_SLOTS])
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
                    if slot < HOPPER_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
        }
        drop(container);

        *self.cooldown.lock() = nbt_view.int("TransferCooldown").unwrap_or(NO_COOLDOWN_TIME);
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
        nbt.insert("TransferCooldown", *self.cooldown.lock());
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for HopperContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        HOPPER_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < HOPPER_SLOTS {
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
