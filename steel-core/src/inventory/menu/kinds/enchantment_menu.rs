//! Enchanting table menu.

use std::sync::Arc;

use steel_protocol::packets::game::SoundSource;
use steel_registry::{
    REGISTRY, RegistryExt as _, TaggedRegistryExt as _,
    blocks::block_state_ext::BlockStateExt as _, enchantment::EnchantmentRef,
    item_stack::ItemStack, sound_events, vanilla_blocks, vanilla_enchantment_tags::EnchantmentTag,
    vanilla_items, vanilla_menu_types,
};
use steel_utils::random::{Random as _, legacy_random::LegacyRandom};
use steel_utils::{
    BlockPos,
    locks::{IntoShared as _, Shared},
};

use crate::{
    behavior::blocks::valid_enchanting_bookshelf_count,
    enchantment_helper::{EnchantmentInstance, enchantment_cost, select_enchantment},
    inventory::{container::SimpleContainer, prelude::*},
    player::player_inventory::PlayerInventory,
    world::World,
};

/// Vanilla shows three offers.
const OFFERS: usize = 3;
/// Vanilla's "no clue" marker for the offered enchantment and level.
const NO_CLUE: i16 = -1;
/// Container-local slot indices.
const ITEM_SLOT: usize = 0;
const LAPIS_SLOT: usize = 1;

/// Builds the enchanting table menu.
#[must_use]
pub fn enchantment(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    pos: BlockPos,
    world: &Arc<World>,
    enchantment_seed: i32,
) -> Menu {
    let enchant_slots = SimpleContainer::new(2).into_shared();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::ENCHANTMENT, container_id);

    let input = builder.section_all_with(
        enchant_slots.clone(),
        SectionKind::restricted(|index, stack: &ItemStack| match index {
            ITEM_SLOT => true,
            LAPIS_SLOT => stack.is(&vanilla_items::LAPIS_LAZULI),
            _ => false,
        }),
    );

    let player = builder.player_inventory(&inventory);

    // Vanilla's data slot order: the three costs, the seed, the enchantment clues, the
    // level clues. The client reads them positionally.
    let costs = [
        builder.data_slot(0),
        builder.data_slot(0),
        builder.data_slot(0),
    ];
    let seed = builder.data_slot(seed_low_bits(enchantment_seed));
    let enchantment_clues = [
        builder.data_slot(NO_CLUE),
        builder.data_slot(NO_CLUE),
        builder.data_slot(NO_CLUE),
    ];
    let level_clues = [
        builder.data_slot(NO_CLUE),
        builder.data_slot(NO_CLUE),
        builder.data_slot(NO_CLUE),
    ];

    builder.route(player.hotbar(), input, FillDirection::Forward);
    builder.route(player.main(), input, FillDirection::Forward);
    builder.route(input, player.all(), FillDirection::Forward);
    builder.drain(input);

    builder.build(EnchantmentKind {
        enchant_slots,
        block_pos: pos,
        world: Arc::clone(world),
        costs,
        seed,
        seed_value: enchantment_seed,
        enchantment_clues,
        level_clues,
        offer_costs: [0; OFFERS],
    })
}

/// The client only receives the low 16 bits of a data slot, matching vanilla.
const fn seed_low_bits(seed: i32) -> i16 {
    let [low, high, _, _] = seed.to_le_bytes();
    i16::from_le_bytes([low, high])
}

/// Per-menu enchanting table state: the inputs, the three offers, and the offer seed.
pub struct EnchantmentKind {
    enchant_slots: Shared<SimpleContainer>,
    block_pos: BlockPos,
    world: Arc<World>,
    /// Client-facing level costs of the three offers.
    costs: [DataSlot; OFFERS],
    /// Client-facing seed slot.
    seed: DataSlot,
    /// Full server-side seed; the data slot only carries its low bits.
    seed_value: i32,
    /// Client-facing registry id of each offer's headline enchantment.
    enchantment_clues: [DataSlot; OFFERS],
    /// Client-facing level of each offer's headline enchantment.
    level_clues: [DataSlot; OFFERS],
    /// Server-side copy of the costs, which are needed at full width.
    offer_costs: [i32; OFFERS],
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for EnchantmentKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/enchantment");
}

impl EnchantmentKind {
    /// The enchantments an enchanting table may offer, in registry order.
    fn table_enchantments() -> Vec<EnchantmentRef> {
        REGISTRY
            .enchantments
            .get_tag(&EnchantmentTag::IN_ENCHANTING_TABLE)
            .unwrap_or_default()
    }

    /// Vanilla `EnchantmentMenu.getEnchantmentList`.
    fn offer_enchantments(
        &self,
        stack: &ItemStack,
        offer: usize,
        cost: i32,
    ) -> Vec<EnchantmentInstance> {
        let mut random = LegacyRandom::from_seed(0);
        random.set_seed(i64::from(self.seed_value) + offer as i64);

        let candidates = Self::table_enchantments();
        if candidates.is_empty() {
            return Vec::new();
        }

        let mut selected = select_enchantment(&mut random, stack, cost, &candidates);
        // Enchanting a book always loses one of several rolled enchantments.
        if stack.is(&vanilla_items::BOOK) && selected.len() > 1 {
            let dropped =
                random.next_i32_bounded(i32::try_from(selected.len()).unwrap_or(i32::MAX)) as usize;
            selected.remove(dropped);
        }
        selected
    }

    fn clear_offers(&mut self, behavior: &mut MenuBehavior) {
        for offer in 0..OFFERS {
            self.offer_costs[offer] = 0;
            self.costs[offer].set(behavior, 0);
            self.enchantment_clues[offer].set(behavior, NO_CLUE);
            self.level_clues[offer].set(behavior, NO_CLUE);
        }
    }

    /// Vanilla `EnchantmentMenu.slotsChanged`.
    fn recompute(&mut self, behavior: &mut MenuBehavior, guard: &mut ContainerLockGuard) {
        let stack = {
            let container = guard
                .get_mut(ContainerId::from_arc(&self.enchant_slots))
                .expect("enchant container not locked");
            container.get_item(ITEM_SLOT).clone()
        };

        if stack.is_empty() || !stack.is_enchantable() {
            self.clear_offers(behavior);
            return;
        }

        let bookcases = valid_enchanting_bookshelf_count(&self.world, self.block_pos);

        let mut random = LegacyRandom::from_seed(0);
        random.set_seed(i64::from(self.seed_value));
        for offer in 0..OFFERS {
            let offer_index = i32::try_from(offer).unwrap_or(0);
            let mut cost = enchantment_cost(&mut random, offer_index, bookcases, &stack);
            // An offer that cannot even reach its own tier is not shown.
            if cost < offer_index + 1 {
                cost = 0;
            }
            self.offer_costs[offer] = cost;
            self.costs[offer].set(behavior, i16::try_from(cost).unwrap_or(i16::MAX));
            self.enchantment_clues[offer].set(behavior, NO_CLUE);
            self.level_clues[offer].set(behavior, NO_CLUE);
        }

        for offer in 0..OFFERS {
            if self.offer_costs[offer] <= 0 {
                continue;
            }
            let rolled = self.offer_enchantments(&stack, offer, self.offer_costs[offer]);
            if rolled.is_empty() {
                continue;
            }

            // The clue shown to the client is one of the rolled enchantments, drawn from
            // the same shared random stream vanilla uses here.
            let index =
                random.next_i32_bounded(i32::try_from(rolled.len()).unwrap_or(i32::MAX)) as usize;
            let clue = rolled[index];
            let id = REGISTRY
                .enchantments
                .id_from_key(&clue.enchantment.key)
                .and_then(|id| i16::try_from(id).ok())
                .unwrap_or(NO_CLUE);
            self.enchantment_clues[offer].set(behavior, id);
            self.level_clues[offer].set(behavior, i16::try_from(clue.level).unwrap_or(NO_CLUE));
        }
    }
}

impl MenuKind for EnchantmentKind {
    /// Returns true while the original enchanting table remains in range.
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::ENCHANTING_TABLE
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }

    fn on_open(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.recompute(behavior, guard);
    }

    fn slots_changed(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.recompute(behavior, guard);
    }

    /// Vanilla `EnchantmentMenu.clickMenuButton`: buys the chosen offer.
    fn on_button_click(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        button_id: i32,
        player: &Player,
    ) -> bool {
        let Ok(offer) = usize::try_from(button_id) else {
            return false;
        };
        if offer >= OFFERS {
            log::debug!(
                "Player {} pressed invalid enchanting button id: {button_id}",
                player.gameprofile.name
            );
            return false;
        }

        let level_cost = i32::try_from(offer).unwrap_or(0) + 1;
        let cost = self.offer_costs[offer];
        let free = player.has_infinite_materials();

        let (stack, lapis_count) = {
            let container = guard
                .get_mut(ContainerId::from_arc(&self.enchant_slots))
                .expect("enchant container not locked");
            (
                container.get_item(ITEM_SLOT).clone(),
                container.get_item(LAPIS_SLOT).count(),
            )
        };

        if !free && lapis_count < level_cost {
            return false;
        }
        if cost <= 0 || stack.is_empty() {
            return false;
        }
        if !free {
            let level = player.experience.lock().level();
            if level < level_cost || level < cost {
                return false;
            }
        }

        let rolled = self.offer_enchantments(&stack, offer, cost);
        if rolled.is_empty() {
            return false;
        }

        player.on_enchantment_performed(level_cost);
        self.seed_value = player.enchantment_seed();
        self.seed.set(behavior, seed_low_bits(self.seed_value));

        {
            let container = guard
                .get_mut(ContainerId::from_arc(&self.enchant_slots))
                .expect("enchant container not locked");

            let mut enchanted = stack;
            // A plain book becomes an enchanted book before the levels go on.
            if enchanted.is(&vanilla_items::BOOK) {
                enchanted.set_item(&vanilla_items::ENCHANTED_BOOK.key);
            }
            for instance in rolled {
                enchanted.upgrade_enchantment(instance.enchantment.key.clone(), instance.level);
            }
            container.set_item(ITEM_SLOT, enchanted);

            let lapis = container.get_item_mut(LAPIS_SLOT);
            if !free {
                lapis.shrink(level_cost);
            }
            if lapis.is_empty() {
                container.set_item(LAPIS_SLOT, ItemStack::empty());
            }
            container.set_changed();
        }

        self.recompute(behavior, guard);
        let pitch = rand::random::<f32>() * 0.1 + 0.9;
        self.world.play_sound(
            &sound_events::BLOCK_ENCHANTMENT_TABLE_USE,
            SoundSource::Blocks,
            self.block_pos,
            1.0,
            pitch,
            None,
        );
        true
    }
}
