//! Grindstone menu.

use std::sync::Arc;

use steel_registry::{
    blocks::block_state_ext::BlockStateExt as _,
    data_components::vanilla_components::{
        ENCHANTMENTS, MAX_DAMAGE, REPAIR_COST, STORED_ENCHANTMENTS,
    },
    item_stack::ItemStack,
    vanilla_blocks, vanilla_items, vanilla_menu_types,
};
use steel_utils::{
    BlockPos, Identifier,
    locks::{IntoShared as _, Shared},
};

use crate::{
    inventory::{
        container::{ResultContainer, SimpleContainer},
        prelude::*,
        slots::{GrindstoneResultHandler, is_curse},
    },
    player::player_inventory::PlayerInventory,
    world::World,
};

/// Vanilla gives the merged item this fraction of its durability as a bonus, in percent.
const MERGE_DURABILITY_BONUS_PERCENT: i32 = 5;

/// Builds the grindstone menu.
#[must_use]
pub fn grindstone(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    pos: BlockPos,
    world: &Arc<World>,
) -> Menu {
    let input_container = SimpleContainer::new(2).into_shared();
    let result_container = ResultContainer::new().into_shared();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::GRINDSTONE, container_id);

    // Vanilla only accepts items a grindstone can actually work on.
    let input = builder.section_all_with(
        input_container.clone(),
        SectionKind::restricted(|_index, stack: &ItemStack| {
            stack.is_damageable_item() || has_any_enchantments(stack)
        }),
    );
    let result = builder.result_slot(GrindstoneResultHandler::new(
        input_container.clone(),
        result_container.clone(),
        pos,
        world.clone(),
    ));

    let player = builder.player_inventory(&inventory);

    builder.route_with_remainder_policy(
        result,
        player.all(),
        FillDirection::Backward,
        FakeResultRemainderPolicy::Discard,
    );
    builder.route(input, player.all(), FillDirection::Forward);
    builder.route(player.hotbar(), input, FillDirection::Forward);
    builder.route(player.main(), input, FillDirection::Forward);
    builder.drain(input);

    builder.build(GrindstoneKind {
        input_container,
        result_container,
        block_pos: pos,
        world: Arc::clone(world),
    })
}

/// Vanilla `EnchantmentHelper.hasAnyEnchantments`.
fn has_any_enchantments(stack: &ItemStack) -> bool {
    stack
        .get_enchantments_for_crafting()
        .is_some_and(|enchantments| !enchantments.is_empty())
}

/// Per-menu grindstone state: the two inputs and the computed result.
pub struct GrindstoneKind {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    block_pos: BlockPos,
    world: Arc<World>,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for GrindstoneKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/grindstone");
}

impl GrindstoneKind {
    /// Vanilla `GrindstoneMenu.createResult`.
    ///
    /// # Panics
    /// Panics if the input container is not exactly two slots.
    fn create_result(&self, guard: &mut ContainerLockGuard) {
        let Some([input_container, result_container]) = guard.get_disjoint_mut([
            ContainerId::from_arc(&self.input_container),
            ContainerId::from_arc(&self.result_container),
        ]) else {
            panic!("failed to lock input and/or result containers to create grindstone result")
        };

        let [first, second] = input_container.items() else {
            panic!("input_container in grindstone menu does not fit expected shape")
        };

        result_container.set_item(0, Self::compute_result(first, second));
    }

    /// Vanilla `GrindstoneMenu.computeResult`.
    fn compute_result(first: &ItemStack, second: &ItemStack) -> ItemStack {
        if first.is_empty() && second.is_empty() {
            return ItemStack::empty();
        }
        // Stacked inputs are rejected outright.
        if first.count() > 1 || second.count() > 1 {
            return ItemStack::empty();
        }

        if !first.is_empty() && !second.is_empty() {
            return Self::merge_items(first, second);
        }

        let item = if first.is_empty() { second } else { first };
        if has_any_enchantments(item) {
            Self::remove_non_curses_from(item.clone())
        } else {
            ItemStack::empty()
        }
    }

    /// Vanilla `GrindstoneMenu.mergeItems`.
    fn merge_items(first: &ItemStack, second: &ItemStack) -> ItemStack {
        if !first.is(second.item()) {
            return ItemStack::empty();
        }

        let durability = first.get_max_damage().max(second.get_max_damage());
        let first_remaining = first.get_max_damage() - first.get_damage_value();
        let second_remaining = second.get_max_damage() - second.get_damage_value();
        let remaining =
            first_remaining + second_remaining + durability * MERGE_DURABILITY_BONUS_PERCENT / 100;

        let mut count = 1;
        if !first.is_damageable_item() {
            // Two identical undamageable items stack into a pair instead of repairing.
            if first.max_stack_size() < 2 || !ItemStack::matches(first, second) {
                return ItemStack::empty();
            }
            count = 2;
        }

        let mut result = first.copy_with_count(count);
        if result.is_damageable_item() {
            result.set(MAX_DAMAGE, durability);
            result.set_damage_value((durability - remaining).max(0));
        }

        Self::merge_enchants_from(&mut result, second);
        Self::remove_non_curses_from(result)
    }

    /// Vanilla `GrindstoneMenu.mergeEnchantsFrom`.
    fn merge_enchants_from(target: &mut ItemStack, source: &ItemStack) {
        let Some(source_enchantments) = source.get_enchantments_for_crafting().cloned() else {
            return;
        };

        for (key, level) in source_enchantments.iter() {
            // A curse already present is not stacked higher; anything else upgrades.
            if is_curse(key) && target.get_enchantment_level(key) != 0 {
                continue;
            }
            target.upgrade_enchantment(key.clone(), *level);
        }
    }

    /// Vanilla `GrindstoneMenu.removeNonCursesFrom`.
    fn remove_non_curses_from(mut item: ItemStack) -> ItemStack {
        let curses: Vec<(Identifier, u32)> = item
            .get_enchantments_for_crafting()
            .map(|enchantments| {
                enchantments
                    .iter()
                    .filter(|(key, _)| is_curse(key))
                    .map(|(key, level)| (key.clone(), *level))
                    .collect()
            })
            .unwrap_or_default();

        // `set_enchantments` only overwrites the listed entries, so the old set has to go
        // first for the non-curses to actually come off.
        item.remove(ENCHANTMENTS);
        item.remove(STORED_ENCHANTMENTS);
        item.set_enchantments(&curses, false);
        // Removing the last stored enchantment turns an enchanted book back into a book.
        if item.is(&vanilla_items::ENCHANTED_BOOK) && curses.is_empty() {
            item.set_item(&vanilla_items::BOOK.key);
        }

        let mut repair_cost: i32 = 0;
        for _ in 0..curses.len() {
            repair_cost = repair_cost.saturating_mul(2).saturating_add(1);
        }
        item.set(REPAIR_COST, repair_cost);
        item
    }
}

impl MenuKind for GrindstoneKind {
    /// Returns true while the original grindstone remains in range.
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::GRINDSTONE
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }

    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.create_result(guard);
    }

    /// Clears the virtual result on close. Inputs are drained by [`Menu::removed`].
    fn removed(&mut self, _behavior: &mut MenuBehavior, _player: &Player) {
        self.result_container.lock().set_item(0, ItemStack::empty());
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::item_stack::ItemStack;
    use steel_registry::test_support::init_test_registry;
    use steel_registry::{vanilla_enchantments, vanilla_items};

    use super::GrindstoneKind;

    #[test]
    fn stripping_the_last_enchantment_turns_an_enchanted_book_back_into_a_book() {
        init_test_registry();
        let mut book = ItemStack::new(&vanilla_items::ENCHANTED_BOOK);
        book.set_enchantments(&[(vanilla_enchantments::SHARPNESS.key.clone(), 3)], false);

        let result = GrindstoneKind::compute_result(&book, &ItemStack::empty());

        assert!(result.is(&vanilla_items::BOOK));
        assert!(result.get_enchantments_for_crafting().is_none());
    }

    #[test]
    fn curses_survive_the_grindstone_and_keep_an_enchanted_book_enchanted() {
        init_test_registry();
        let mut book = ItemStack::new(&vanilla_items::ENCHANTED_BOOK);
        book.set_enchantments(
            &[
                (vanilla_enchantments::SHARPNESS.key.clone(), 3),
                (vanilla_enchantments::VANISHING_CURSE.key.clone(), 1),
            ],
            false,
        );

        let result = GrindstoneKind::compute_result(&book, &ItemStack::empty());

        assert!(result.is(&vanilla_items::ENCHANTED_BOOK));
        let remaining = result
            .get_enchantments_for_crafting()
            .expect("curses should remain on the book");
        assert_eq!(
            remaining.get_level(&vanilla_enchantments::VANISHING_CURSE.key),
            1
        );
        assert_eq!(remaining.get_level(&vanilla_enchantments::SHARPNESS.key), 0);
    }

    #[test]
    fn merging_two_damaged_tools_adds_their_remaining_durability_plus_a_bonus() {
        init_test_registry();
        let mut first = ItemStack::new(&vanilla_items::IRON_PICKAXE);
        let mut second = ItemStack::new(&vanilla_items::IRON_PICKAXE);
        let max_damage = first.get_max_damage();
        first.set_damage_value(max_damage / 2);
        second.set_damage_value(max_damage / 2);

        let result = GrindstoneKind::compute_result(&first, &second);

        // Two halves plus the flat 5% bonus leave the result fully repaired.
        assert_eq!(result.get_damage_value(), 0);
        assert_eq!(result.count(), 1);
    }

    #[test]
    fn an_unenchanted_undamaged_single_item_yields_nothing() {
        init_test_registry();
        let stick = ItemStack::new(&vanilla_items::STICK);

        assert!(GrindstoneKind::compute_result(&stick, &ItemStack::empty()).is_empty());
    }
}
