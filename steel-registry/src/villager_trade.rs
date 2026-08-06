//! Vanilla `VillagerTrade` / `TradeSet` registries.
//!
//! Villager offers are datapack-driven in vanilla: a profession level names a `TradeSet`,
//! which names a tag of `VillagerTrade`s and how many to roll. Both are compiled into typed
//! Rust by `build/villager_trades.rs`.

use rustc_hash::FxHashMap;
use steel_utils::Identifier;

use crate::item_stack::ItemStack;
use crate::loot_table::{ConditionalLootFunction, LootCondition, LootContext, NumberProvider};
use crate::{REGISTRY, RegistryExt as _};

/// The item a trade hands to the player, before its modifiers run.
#[derive(Debug)]
pub struct TradeResult {
    pub item: Identifier,
    pub count: i32,
}

impl TradeResult {
    /// Vanilla `ItemStackTemplate.create`.
    #[must_use]
    pub fn create(&self) -> ItemStack {
        REGISTRY
            .items
            .by_key(&self.item)
            .map_or_else(ItemStack::empty, |item| {
                ItemStack::with_count(item, self.count)
            })
    }
}

/// Vanilla `TradeCost`: the item a player must hand over.
#[derive(Debug)]
pub struct TradeCost {
    pub item: Identifier,
    pub count: NumberProvider,
    /// Exact component match required on the player's input, as the datapack's JSON
    /// component map. Vanilla models this as `DataComponentExactPredicate`; Steel applies it
    /// to the cost stack so the offer both displays and compares correctly.
    pub components: Option<&'static str>,
}

impl TradeCost {
    /// Vanilla `TradeCost.toItemCost`: rolls the count and adds any surcharge earned by the
    /// result's modifiers, clamped to the item's stack limit.
    #[must_use]
    pub fn to_item_stack<R: rand::Rng>(
        &self,
        ctx: &mut LootContext<'_, R>,
        additional_cost: i32,
    ) -> ItemStack {
        let Some(item) = REGISTRY.items.by_key(&self.item) else {
            return ItemStack::empty();
        };
        let count = self.count.get_int(ctx.rng).saturating_add(additional_cost);
        let mut stack = ItemStack::new(item);
        if let Some(components) = self.components {
            stack.set_components_from_json(components);
        }
        stack.set_count(count.clamp(0, stack.max_stack_size()));
        stack
    }
}

/// One datapack-defined villager trade.
#[derive(Debug)]
pub struct VillagerTrade {
    pub key: Identifier,
    pub wants: TradeCost,
    pub additional_wants: Option<TradeCost>,
    pub gives: TradeResult,
    /// Vanilla defaults: `max_uses` 4, `xp` 1, `reputation_discount` 0.
    pub max_uses: NumberProvider,
    pub xp: NumberProvider,
    pub reputation_discount: NumberProvider,
    /// Gates the offer on the merchant itself (e.g. cartographer biome variants).
    pub merchant_predicate: Option<LootCondition>,
    /// Loot functions applied to the *result* stack before pricing.
    pub given_item_modifiers: &'static [ConditionalLootFunction],
    /// Enchantment tag that doubles the surcharge when the result carries one of them.
    pub double_trade_price_enchantments: Option<Identifier>,
}

pub type VillagerTradeRef = &'static VillagerTrade;

/// A profession level's pool of trades and how many to roll from it.
#[derive(Debug)]
pub struct TradeSet {
    pub key: Identifier,
    /// Tag naming the candidate `VillagerTrade`s.
    pub trades: Identifier,
    pub amount: NumberProvider,
    pub allow_duplicates: bool,
    pub random_sequence: Option<Identifier>,
}

pub type TradeSetRef = &'static TradeSet;

pub struct VillagerTradeRegistry {
    villager_trades_by_id: Vec<VillagerTradeRef>,
    villager_trades_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl VillagerTradeRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            villager_trades_by_id: Vec::new(),
            villager_trades_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    VillagerTradeRegistry,
    VillagerTradeRef,
    villager_trades_by_id,
    villager_trades_by_key,
    allows_registering
);

crate::impl_registry!(
    VillagerTradeRegistry,
    VillagerTrade,
    villager_trades_by_id,
    villager_trades_by_key,
    villager_trades
);

pub struct TradeSetRegistry {
    trade_sets_by_id: Vec<TradeSetRef>,
    trade_sets_by_key: FxHashMap<Identifier, usize>,
    allows_registering: bool,
}

impl TradeSetRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            trade_sets_by_id: Vec::new(),
            trade_sets_by_key: FxHashMap::default(),
            allows_registering: true,
        }
    }
}

crate::impl_standard_methods!(
    TradeSetRegistry,
    TradeSetRef,
    trade_sets_by_id,
    trade_sets_by_key,
    allows_registering
);

crate::impl_registry!(
    TradeSetRegistry,
    TradeSet,
    trade_sets_by_id,
    trade_sets_by_key,
    trade_sets
);

/// The priced result of a trade: what the player pays and what they receive.
#[derive(Debug, Clone)]
pub struct TradeOffer {
    pub wants: ItemStack,
    pub additional_wants: Option<ItemStack>,
    pub gives: ItemStack,
    pub max_uses: i32,
    pub xp: i32,
    pub reputation_discount: f32,
}

impl VillagerTrade {
    /// Vanilla `VillagerTrade.getOffer`.
    ///
    /// Returns `None` when the merchant predicate rejects the trade, an item modifier
    /// discards the result, or either cost rolls below one item.
    #[must_use]
    pub fn get_offer<R: rand::Rng>(&self, ctx: &mut LootContext<'_, R>) -> Option<TradeOffer> {
        use crate::data_components::vanilla_components::{
            ADDITIONAL_TRADE_COST, STORED_ENCHANTMENTS,
        };

        if let Some(predicate) = &self.merchant_predicate
            && !predicate.test(ctx)
        {
            return None;
        }

        let mut gives = self.gives.create();
        for modifier in self.given_item_modifiers {
            if modifier.conditions.iter().all(|c| c.test(ctx)) {
                modifier.function.apply(&mut gives, ctx);
            }
            if gives.is_empty() {
                return None;
            }
        }

        // Enchanted-book trades price themselves via this component, which vanilla consumes
        // (removes) so it never reaches the player's inventory.
        let mut additional_cost = gives.get(ADDITIONAL_TRADE_COST).copied().unwrap_or(0);
        gives.remove(ADDITIONAL_TRADE_COST);

        if let Some(tag) = &self.double_trade_price_enchantments
            && let Some(enchantments) = gives.get(STORED_ENCHANTMENTS)
            && enchantments.iter().any(|(key, _)| {
                crate::REGISTRY
                    .enchantments
                    .by_key(key)
                    .is_some_and(|enchantment| {
                        crate::TaggedRegistryExt::is_in_tag(
                            &crate::REGISTRY.enchantments,
                            enchantment,
                            tag,
                        )
                    })
            })
        {
            additional_cost *= 2;
        }

        let wants = self.wants.to_item_stack(ctx, additional_cost);
        if wants.count() < 1 {
            return None;
        }

        let additional_wants = match &self.additional_wants {
            Some(cost) => {
                let stack = cost.to_item_stack(ctx, 0);
                if stack.count() < 1 {
                    return None;
                }
                Some(stack)
            }
            None => None,
        };

        Some(TradeOffer {
            wants,
            additional_wants,
            gives,
            max_uses: self.max_uses.get_int(ctx.rng).max(1),
            xp: self.xp.get_int(ctx.rng).max(0),
            reputation_discount: self.reputation_discount.get_simple(ctx.rng).max(0.0),
        })
    }
}
