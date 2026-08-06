//! Vanilla `EnchantmentHelper` enchantment rolling.
//!
//! These are the pure-data halves of vanilla's `EnchantmentHelper`: they need only the
//! enchantment registry, an `ItemStack`, and a random source. They live here rather
//! than in steel-core so that loot-table functions (`enchant_randomly`, `enchant_with_levels`)
//! and villager trade generation can reach them; steel-core re-exports them for the
//! enchanting-table path.

use steel_utils::random::Random;

use crate::data_components::vanilla_components::ENCHANTABLE;
use crate::enchantment::{Enchantment, EnchantmentRef};
use crate::item_stack::ItemStack;
use crate::vanilla_items;

/// Vanilla's enchanting table caps the bookshelf bonus at 15.
const MAX_BOOKCASES: i32 = 15;

/// Vanilla `EnchantmentInstance`: one enchantment at one level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnchantmentInstance {
    /// The enchantment being offered.
    pub enchantment: EnchantmentRef,
    /// The level being offered.
    pub level: u32,
}

/// Vanilla `EnchantmentHelper.getEnchantmentCost`: the level cost shown on an offer.
///
/// Returns 0 for items that cannot be enchanted at all.
#[must_use]
pub fn enchantment_cost(
    random: &mut impl Random,
    slot: i32,
    bookcases: i32,
    stack: &ItemStack,
) -> i32 {
    if stack.get(ENCHANTABLE).is_none() {
        return 0;
    }

    let bookcases = bookcases.min(MAX_BOOKCASES);
    let selected =
        random.next_i32_bounded(8) + 1 + (bookcases >> 1) + random.next_i32_bounded(bookcases + 1);

    match slot {
        0 => (selected / 3).max(1),
        1 => selected * 2 / 3 + 1,
        _ => selected.max(bookcases * 2),
    }
}

/// Vanilla `EnchantmentHelper.selectEnchantment`: rolls the enchantments an offer grants.
#[must_use]
pub fn select_enchantment(
    random: &mut impl Random,
    stack: &ItemStack,
    enchantment_cost: i32,
    candidates: &[EnchantmentRef],
) -> Vec<EnchantmentInstance> {
    let mut results = Vec::new();
    let Some(enchantable) = stack.get(ENCHANTABLE) else {
        return results;
    };

    // The item's own enchantability widens the effective cost, then jitters it by ±15%.
    let spread = enchantable.value() / 4 + 1;
    let mut cost =
        enchantment_cost + 1 + random.next_i32_bounded(spread) + random.next_i32_bounded(spread);
    let jitter = (random.next_f32() + random.next_f32() - 1.0) * 0.15;
    #[expect(
        clippy::cast_precision_loss,
        reason = "mirrors vanilla's float arithmetic on the cost"
    )]
    let jittered = steel_utils::java::round_to_i32(cost as f32 + cost as f32 * jitter);
    cost = jittered.max(1);

    let mut available = available_enchantment_results(cost, stack, candidates);
    if available.is_empty() {
        return results;
    }

    if let Some(picked) = take_weighted(random, &available) {
        results.push(picked);
    }

    // Each extra enchantment is progressively less likely and halves the remaining cost.
    while random.next_i32_bounded(50) <= cost {
        if let Some(last) = results.last() {
            retain_compatible(&mut available, *last);
        }
        if available.is_empty() {
            break;
        }
        if let Some(picked) = take_weighted(random, &available) {
            results.push(picked);
        }
        cost /= 2;
    }

    results
}

/// Vanilla `EnchantmentHelper.getAvailableEnchantmentResults`: the highest level of every
/// candidate whose cost window contains `value`.
#[must_use]
pub fn available_enchantment_results(
    value: i32,
    stack: &ItemStack,
    candidates: &[EnchantmentRef],
) -> Vec<EnchantmentInstance> {
    // A plain book may receive anything; every other item is limited to its primary set.
    let is_book = stack.is(&vanilla_items::BOOK);
    let mut results = Vec::new();

    for enchantment in candidates {
        if !is_book && !enchantment.is_primary_item(stack.item()) {
            continue;
        }

        for level in (1..=enchantment.max_level).rev() {
            if value >= enchantment.min_cost(level) && value <= enchantment.max_cost(level) {
                results.push(EnchantmentInstance { enchantment, level });
                break;
            }
        }
    }

    results
}

/// Vanilla `WeightedRandom.getRandomItem` over enchantment weights.
fn take_weighted(
    random: &mut impl Random,
    available: &[EnchantmentInstance],
) -> Option<EnchantmentInstance> {
    let total: i32 = available
        .iter()
        .map(|instance| i32::try_from(instance.enchantment.weight).unwrap_or(i32::MAX))
        .sum();
    if total <= 0 {
        return None;
    }

    let mut selection = random.next_i32_bounded(total);
    for instance in available {
        selection -= i32::try_from(instance.enchantment.weight).unwrap_or(i32::MAX);
        if selection < 0 {
            return Some(*instance);
        }
    }
    None
}

/// Vanilla `EnchantmentHelper.filterCompatibleEnchantments`.
fn retain_compatible(available: &mut Vec<EnchantmentInstance>, chosen: EnchantmentInstance) {
    available
        .retain(|instance| Enchantment::are_compatible(instance.enchantment, chosen.enchantment));
}
