//! Barrel block entity implementation.
//!
//! Barrels are container block entities with 27 slots (3x9 grid),
//! functioning similarly to chests but without double-chest behavior.

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

use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;

use crate::block_entity::container_openers_counter::ContainerOpenersCounter;
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;
use crate::world::LevelAccessor as _;

/// Number of slots in a barrel (3 rows of 9).
pub const BARREL_SLOTS: usize = 27;

/// Barrel block entity.
///
/// A simple container with 27 slots, using the same menu as chests.
pub struct BarrelBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<BarrelContainer>>,
    container_ref: ContainerRef,
    openers_counter: ContainerOpenersCounter,
}

struct BarrelContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BarrelBlockEntity`.
unsafe impl DowncastType for BarrelBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/barrel");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently
// lockable inventory data used by a barrel block entity.
unsafe impl DowncastType for BarrelContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/barrel");
}

impl BarrelBlockEntity {
    /// Creates a new barrel block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::BARREL,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(BarrelContainer {
            items: vec![ItemStack::empty(); BARREL_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            openers_counter: ContainerOpenersCounter::new(),
        }
    }

    /// Vanilla `BarrelBlockEntity.startOpen`.
    pub fn start_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        self.openers_counter.increment(&world, pos, state, block, |world, pos, state| {
            Self::play_sound(world, pos, state, true);
            Self::update_block_state(world, pos, state, true);
        });
    }

    /// Vanilla `BarrelBlockEntity.stopOpen`.
    pub fn stop_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        self.openers_counter.decrement(&world, pos, state, block, |world, pos, state| {
            Self::play_sound(world, pos, state, false);
            Self::update_block_state(world, pos, state, false);
        });
    }

    /// Vanilla `BarrelBlockEntity.recheckOpen`.
    pub fn recheck_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        self.openers_counter.recheck(&world, pos, block);
    }

    fn update_block_state(world: &Arc<World>, pos: BlockPos, state: BlockStateId, open: bool) {
        use steel_registry::blocks::block_state_ext::BlockStateExt as _;
        use steel_registry::blocks::properties::BlockStateProperties;
        let new_state = state.set_value(&BlockStateProperties::OPEN, open);
        if new_state != state {
            world.set_block_state(pos, new_state, steel_utils::types::UpdateFlags::UPDATE_ALL);
        }
    }

    fn play_sound(world: &Arc<World>, pos: BlockPos, _state: BlockStateId, open: bool) {
        use steel_registry::sound_events;
        let sound = if open {
            &sound_events::BLOCK_BARREL_OPEN
        } else {
            &sound_events::BLOCK_BARREL_CLOSE
        };
        world.play_sound(
            sound,
            SoundSource::Blocks,
            pos,
            0.5,
            0.9 + rand::random::<f32>() * 0.1,
            None,
        );
    }

    /// Returns current opener count.
    #[must_use]
    pub fn open_count(&self) -> i32 {
        self.openers_counter.get_count()
    }
}

impl BlockEntity for BarrelBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn trigger_event(&self, param_a: i32, param_b: i32) -> bool {
        // Barrel has no lid animation param, but we keep for completeness.
        let _ = (param_a, param_b);
        false
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); BARREL_SLOTS])
        };
        let Some(world) = self.get_level() else {
            return;
        };
        for item in items {
            world.drop_item_stack(pos, item);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        // Convert to NbtCompound view for accessing methods
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());

        // Load items from NBT using borrowed NBT for proper ItemStack parsing
        if let Some(items_list) = nbt_view.list("Items")
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                // Each item has a "Slot" byte and item data
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < BARREL_SLOTS {
                        // Parse item directly from the borrowed compound
                        if let Some(item) = ItemStack::from_borrowed_compound(&compound) {
                            container.items[slot] = item;
                        }
                    }
                }
            }
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        // Save items to NBT (only non-empty slots)
        let container = self.container.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in container.items.iter().enumerate() {
            if !item.is_empty() {
                // Use ItemStack's ToNbtTag implementation for proper component serialization
                if let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag() {
                    item_nbt.insert("Slot", slot as i8);
                    items.push(item_nbt);
                }
            }
        }
        nbt.insert("Items", NbtList::Compound(items));
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        // Barrels don't need to send inventory to clients on chunk load
        // (unlike signs which display text)
        None
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for BarrelContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        BARREL_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < BARREL_SLOTS {
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

#[cfg(test)]
mod tests {
    use steel_registry::{test_support::init_test_registry, vanilla_blocks, vanilla_items};

    use super::*;

    fn test_barrel() -> BarrelBlockEntity {
        init_test_registry();
        BarrelBlockEntity::new(
            Weak::new(),
            BlockPos::new(1, 2, 3),
            vanilla_blocks::BARREL.default_state(),
        )
    }

    #[test]
    fn set_item_limits_stack_to_vanilla_container_maximum() {
        let barrel = test_barrel();
        barrel
            .container
            .lock()
            .set_item(0, ItemStack::with_count(&vanilla_items::STONE, 100));

        assert_eq!(barrel.container.lock().get_item(0).count(), 64);
    }

    #[test]
    fn pre_remove_preserves_slots_for_existing_menu_references() {
        let barrel = test_barrel();
        barrel
            .container
            .lock()
            .set_item(0, ItemStack::new(&vanilla_items::STONE));

        barrel.pre_remove_side_effects(
            BlockPos::new(1, 2, 3),
            vanilla_blocks::BARREL.default_state(),
        );

        let container = barrel.container.lock();
        assert_eq!(container.items.len(), BARREL_SLOTS);
        assert!(container.items.iter().all(ItemStack::is_empty));
    }
}
