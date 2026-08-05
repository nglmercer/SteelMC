//! Chest block entity implementation.
//!
//! Backs single chests, trapped chests and copper chests. Double chests are two of these
//! entities combined by [`ChestBlock`](crate::behavior::blocks::ChestBlock) at open time.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;

use crate::block_entity::container_openers_counter::ContainerOpenersCounter;
use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Number of slots in a single chest (3 rows of 9).
pub const CHEST_SLOTS: usize = 27;

/// Vanilla `ChestBlockEntity`.
///
/// Lid animation is client-side; `ContainerOpenersCounter` tracks openers,
/// fires `CONTAINER_OPEN/CLOSE` game events, `blockEvent(1, count)` for the
/// lid, and chest sounds.
pub struct ChestBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<ChestContainer>>,
    container_ref: ContainerRef,
    openers_counter: ContainerOpenersCounter,
    /// Vanilla `ChestLidController` openness; kept for `getOpenNess` if needed.
    chest_lid_open: SyncMutex<bool>,
}

struct ChestContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ChestBlockEntity`.
unsafe impl DowncastType for ChestBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/chest");
}

// SAFETY: This key is owned by Steel and uniquely identifies the independently lockable
// inventory data used by a chest block entity.
unsafe impl DowncastType for ChestContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/chest");
}

impl ChestBlockEntity {
    /// Creates an empty chest block entity of the given block entity type.
    ///
    /// Trapped and copper chests use the same storage with a different registered type.
    #[must_use]
    pub fn new(
        block_entity_type: BlockEntityTypeRef,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        let base = Arc::new(BlockEntityBase::new(block_entity_type, level, pos, state));
        let container = Arc::new(SyncMutex::new(ChestContainer {
            items: vec![ItemStack::empty(); CHEST_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(shared_container, Arc::clone(&base)),
            base,
            container,
            openers_counter: ContainerOpenersCounter::new(),
            chest_lid_open: SyncMutex::new(false),
        }
    }

    /// Vanilla `ChestBlockEntity.startOpen`.
    pub fn start_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        self.openers_counter.increment(&world, pos, state, block, |world, pos, state| {
            Self::play_sound(world, pos, state, true);
        });
    }

    /// Vanilla `ChestBlockEntity.stopOpen`.
    pub fn stop_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        self.openers_counter.decrement(&world, pos, state, block, |world, pos, state| {
            Self::play_sound(world, pos, state, false);
        });
    }

    /// Vanilla `ChestBlockEntity.recheckOpen`.
    pub fn recheck_open(&self) {
        let Some(world) = self.get_level() else {
            return;
        };
        let pos = self.get_block_pos();
        let state = self.get_block_state();
        let block = state.get_block();
        self.openers_counter.recheck(&world, pos, block);
    }

    /// Returns current opener count.
    #[must_use]
    pub fn open_count(&self) -> i32 {
        self.openers_counter.get_count()
    }

    fn play_sound(world: &Arc<World>, pos: BlockPos, state: BlockStateId, open: bool) {
        use steel_registry::blocks::properties::ChestType;
        use steel_registry::blocks::properties::BlockStateProperties;
        use steel_registry::blocks::block_state_ext::BlockStateExt as _;
        use steel_registry::sound_events;
        use steel_registry::vanilla_block_tags::BlockTag;
        // Only LEFT/SINGLE chests emit sound; right half is silent in double.
        let chest_type = state
            .try_get_value(&BlockStateProperties::CHEST_TYPE)
            .unwrap_or(ChestType::Single);
        if chest_type == ChestType::Right {
            return;
        }
        let sound = if open {
            &sound_events::BLOCK_CHEST_OPEN
        } else {
            &sound_events::BLOCK_CHEST_CLOSE
        };
        // For trapped/copper chests vanilla uses different sounds, but they share
        // the same event keys; use generic chest sounds for now.
        let is_trapped = state.get_block().has_tag(&BlockTag::COPPER_CHESTS) == false
            && state.get_block().key.path.as_ref() == "trapped_chest";
        let _ = is_trapped;
        // Copper chests use same sounds in vanilla except same as chest.
        world.play_sound(
            sound,
            SoundSource::Blocks,
            pos,
            0.5,
            0.9 + rand::random::<f32>() * 0.1,
            None,
        );
    }
}

impl BlockEntity for ChestBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn trigger_event(&self, param_a: i32, param_b: i32) -> bool {
        if param_a == 1 {
            *self.chest_lid_open.lock() = param_b > 0;
            return true;
        }
        false
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); CHEST_SLOTS])
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
                    if slot < CHEST_SLOTS
                        && let Some(item) = ItemStack::from_borrowed_compound(&compound)
                    {
                        container.items[slot] = item;
                    }
                }
            }
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

impl Container for ChestContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        CHEST_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < CHEST_SLOTS {
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
