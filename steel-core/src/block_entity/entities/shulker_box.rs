//! Shulker box block entity implementation.
//!
//! Unlike other containers, a broken shulker box keeps its contents inside the dropped
//! item rather than scattering them, so this entity deliberately does not drop its items
//! on removal — [`ShulkerBoxBlock`](crate::behavior::blocks::ShulkerBoxBlock) reads them
//! back out when generating drops.

use std::sync::{Arc, Weak};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::components::ItemContainerContents;
use steel_registry::item_stack::ItemStack;
use steel_registry::item_stack_template::ItemStackTemplate;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;
use steel_registry::data_components::DataComponentPatch;
use steel_registry::data_components::vanilla_components::CONTAINER;

/// Number of slots in a shulker box (3 rows of 9).
pub const SHULKER_BOX_SLOTS: usize = 27;

/// Vanilla `ShulkerBoxBlockEntity`.
///
/// Vanilla's lid animation state is only used for rendering and for the "can the lid
/// open" collision test, which Steel does not model yet.
pub struct ShulkerBoxBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<ShulkerBoxContainer>>,
    container_ref: ContainerRef,
    open_count: SyncMutex<i32>,
}

struct ShulkerBoxContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ShulkerBoxBlockEntity`.
unsafe impl DowncastType for ShulkerBoxBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/shulker_box");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a shulker box block entity.
unsafe impl DowncastType for ShulkerBoxContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/shulker_box");
}

impl ShulkerBoxBlockEntity {
    /// Creates an empty shulker box block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::SHULKER_BOX,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(ShulkerBoxContainer {
            items: vec![ItemStack::empty(); SHULKER_BOX_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            open_count: SyncMutex::new(0),
        }
    }

    /// Vanilla `ShulkerBoxBlockEntity.startOpen`.
    pub fn start_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        let mut count = self.open_count.lock();
        *count += 1;
        let current = *count;
        drop(count);
        world.block_event(pos, block, 1, current);
        if current == 1 {
            world.game_event(
                &steel_registry::vanilla_game_events::CONTAINER_OPEN,
                pos,
                &crate::world::game_event::GameEventContext::default(),
            );
            world.play_sound(
                &steel_registry::sound_events::BLOCK_SHULKER_BOX_OPEN,
                SoundSource::Blocks,
                pos,
                0.5,
                0.9 + rand::random::<f32>() * 0.1,
                None,
            );
        }
    }

    /// Vanilla `ShulkerBoxBlockEntity.stopOpen`.
    pub fn stop_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        let mut count = self.open_count.lock();
        if *count > 0 {
            *count -= 1;
        }
        let current = *count;
        drop(count);
        world.block_event(pos, block, 1, current);
        if current == 0 {
            world.game_event(
                &steel_registry::vanilla_game_events::CONTAINER_CLOSE,
                pos,
                &crate::world::game_event::GameEventContext::default(),
            );
            world.play_sound(
                &steel_registry::sound_events::BLOCK_SHULKER_BOX_CLOSE,
                SoundSource::Blocks,
                pos,
                0.5,
                0.9 + rand::random::<f32>() * 0.1,
                None,
            );
        }
    }

    /// Returns whether every slot is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.container.lock().items.iter().all(ItemStack::is_empty)
    }

    /// Packs the contents into the `container` item component vanilla stores on the
    /// dropped shulker box item.
    ///
    /// Returns `None` when the box is empty, matching vanilla's omission of the component.
    #[must_use]
    pub fn contents_component(&self) -> Option<ItemContainerContents> {
        if self.is_empty() {
            return None;
        }

        let items = self
            .container
            .lock()
            .items
            .iter()
            .map(|item| {
                (!item.is_empty())
                    .then(|| ItemStackTemplate::from_stack(item).ok())
                    .flatten()
            })
            .collect();

        ItemContainerContents::new(items).ok()
    }

    /// Replaces the contents from an item's `container` component.
    pub fn set_contents_from_component(&self, contents: &ItemContainerContents) {
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());
        for (slot, template) in contents.items().iter().enumerate().take(SHULKER_BOX_SLOTS) {
            if let Some(template) = template {
                container.items[slot] = template.create();
            }
        }
        drop(container);
        self.set_changed();
    }
}

impl BlockEntity for ShulkerBoxBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn trigger_event(&self, param_a: i32, param_b: i32) -> bool {
        if param_a == 1 {
            *self.open_count.lock() = param_b;
            return true;
        }
        false
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
                    if slot < SHULKER_BOX_SLOTS
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

impl Container for ShulkerBoxContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        SHULKER_BOX_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < SHULKER_BOX_SLOTS {
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

    fn shulker_box() -> ShulkerBoxBlockEntity {
        init_test_registry();
        ShulkerBoxBlockEntity::new(
            Weak::new(),
            BlockPos::new(8, 70, -2),
            vanilla_blocks::SHULKER_BOX.default_state(),
        )
    }

    #[test]
    fn empty_box_has_no_contents_component() {
        assert!(shulker_box().contents_component().is_none());
    }

    #[test]
    fn contents_round_trip_through_the_item_component() {
        let source = shulker_box();
        source
            .container
            .lock()
            .set_item(4, ItemStack::with_count(&vanilla_items::STONE, 12));

        let contents = source
            .contents_component()
            .expect("non-empty box should produce contents");

        let restored = shulker_box();
        restored.set_contents_from_component(&contents);

        let container = restored.container.lock();
        assert_eq!(container.get_item(4).count(), 12);
        assert_eq!(container.get_item(4).item(), &*vanilla_items::STONE);
        assert!(container.get_item(0).is_empty());
    }
}
