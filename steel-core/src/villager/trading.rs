//! Trading primitives for villagers and wandering traders.
//!
//! Mirrors `net.minecraft.world.item.trading.MerchantOffer` / `MerchantOffers`
//! at the gameplay level (cost, demand, special price, XP, restock).

use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::LootContext;
use steel_registry::{REGISTRY, RegistryExt as _, TaggedRegistryExt as _};
use steel_utils::Identifier;
use steel_utils::random::{Random as _, RandomSource};

/// A single villager trade offer.
///
/// Simplified relative to vanilla codec but preserves all gameplay-visible
/// fields: buy A, optional buy B, sell, uses, maxUses, demand, specialPrice,
/// priceMultiplier, xp, rewardExp.
#[derive(Debug, Clone)]
pub struct MerchantOffer {
    /// First required input item stack.
    pub buy_a: ItemStack,
    /// Optional second required input item stack.
    pub buy_b: Option<ItemStack>,
    /// Output item stack offered to the player.
    pub sell: ItemStack,
    /// Number of times this offer has been used.
    pub uses: i32,
    /// Maximum uses before the offer locks.
    pub max_uses: i32,
    /// Accumulated demand affecting price.
    pub demand: i32,
    /// Reputation/special price adjustment.
    pub special_price: i32,
    /// Multiplier applied to demand-based price increases.
    pub price_multiplier: f32,
    /// XP granted to the villager when the trade is completed.
    pub xp: i32,
    /// Whether the trade grants XP.
    pub reward_xp: bool,
}

impl MerchantOffer {
    /// Creates a new offer from its buy/sell stacks and trade parameters.
    #[must_use]
    pub fn new(
        buy_a: ItemStack,
        buy_b: Option<ItemStack>,
        sell: ItemStack,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
    ) -> Self {
        Self {
            buy_a,
            buy_b,
            sell,
            uses: 0,
            max_uses,
            demand: 0,
            special_price: 0,
            price_multiplier,
            xp,
            reward_xp: true,
        }
    }

    /// Returns `true` if the offer has reached its maximum uses.
    #[must_use]
    pub fn is_out_of_stock(&self) -> bool {
        self.uses >= self.max_uses
    }

    /// Returns `true` if the offer needs a restock (has been used).
    #[must_use]
    pub fn needs_restock(&self) -> bool {
        self.uses > 0
    }

    /// Resets the use counter.
    pub fn reset_uses(&mut self) {
        self.uses = 0;
    }

    /// Increments the use counter by one.
    pub fn increase_uses(&mut self) {
        self.uses += 1;
    }

    /// Updates demand based on uses versus remaining uses.
    pub fn update_demand(&mut self) {
        // vanilla: demand += uses - (maxUses - uses)
        self.demand += self.uses - (self.max_uses - self.uses);
    }

    /// Resets the special price adjustment to zero.
    pub fn reset_special_price(&mut self) {
        self.special_price = 0;
    }

    /// Adjusts the special price by `delta`.
    pub fn add_special_price(&mut self, delta: i32) {
        self.special_price += delta;
    }

    /// Returns the effective count required for the first input, including demand and special price.
    #[must_use]
    pub fn adjusted_buy_a_count(&self) -> i32 {
        let base = self.buy_a.count() as i32;
        let demand_component =
            (base as f32 * self.demand as f32 * self.price_multiplier).floor() as i32;
        let adjusted = base + demand_component.max(0) + self.special_price;
        adjusted.clamp(1, self.buy_a.count() as i32 * 2 + 64).max(1)
    }

    /// Returns `true` if `offered_a`/`offered_b` satisfy this offer.
    #[must_use]
    pub fn can_trade(&self, offered_a: &ItemStack, offered_b: &ItemStack) -> bool {
        if self.is_out_of_stock() {
            return false;
        }
        if offered_a.item() != self.buy_a.item() || offered_a.count() < self.adjusted_buy_a_count()
        {
            return false;
        }
        match &self.buy_b {
            None => offered_b.is_empty(),
            Some(buy_b) => offered_b.item() == buy_b.item() && offered_b.count() >= buy_b.count(),
        }
    }
}

/// Collection of offers for a single villager/trader.
#[derive(Debug, Clone, Default)]
pub struct MerchantOffers {
    /// Underlying offer list.
    pub offers: Vec<MerchantOffer>,
}

impl MerchantOffers {
    /// Creates an empty offer list.
    #[must_use]
    pub fn new() -> Self {
        Self { offers: Vec::new() }
    }

    /// Returns `true` if no offers are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }

    /// Appends an offer.
    pub fn push(&mut self, offer: MerchantOffer) {
        self.offers.push(offer);
    }

    /// Resets use counters for all offers.
    pub fn reset_all_uses(&mut self) {
        for o in &mut self.offers {
            o.reset_uses();
        }
    }

    /// Updates demand for all offers.
    pub fn update_all_demand(&mut self) {
        for o in &mut self.offers {
            o.update_demand();
        }
    }

    /// Resets special price adjustments for all offers.
    pub fn reset_all_special_prices(&mut self) {
        for o in &mut self.offers {
            o.reset_special_price();
        }
    }

    /// Returns `true` if any offer needs a restock.
    #[must_use]
    pub fn needs_restock(&self) -> bool {
        self.offers.iter().any(|o| o.needs_restock())
    }

    /// Rolls the vanilla trade set for a profession level.
    ///
    /// Vanilla resolves `trade_set/<profession>/level_<n>`, rolls `amount` trades from the
    /// tag it names, and asks each for an offer. Trades whose merchant predicate rejects the
    /// villager (cartographer biome maps, for example) are skipped.
    #[must_use]
    pub fn generate_for_profession(profession_id: i32, level: i32) -> Self {
        let mut offers = MerchantOffers::new();
        let Some(profession) = REGISTRY.villager_professions.by_id(profession_id as usize) else {
            return offers;
        };

        let set_key = Identifier::vanilla(format!(
            "{}/level_{}",
            profession.key.path,
            level.clamp(1, 5)
        ));
        let Some(trade_set) = REGISTRY.trade_sets.by_key(&set_key) else {
            return offers;
        };
        let Some(candidates) = REGISTRY.villager_trades.get_tag(&trade_set.trades) else {
            return offers;
        };
        if candidates.is_empty() {
            return offers;
        }

        let mut rng = RandomSource::create_thread_safe();
        let mut ctx = LootContext::new(&mut rng);
        let wanted = trade_set.amount.get_int(ctx.rng).max(0) as usize;

        // Vanilla samples without replacement unless the set opts into duplicates.
        let mut remaining: Vec<_> = candidates.clone();
        for _ in 0..wanted {
            if remaining.is_empty() {
                break;
            }
            let index = ctx.rng.next_i32_bounded(remaining.len() as i32) as usize;
            let trade = if trade_set.allow_duplicates {
                remaining[index]
            } else {
                remaining.swap_remove(index)
            };

            if let Some(offer) = trade.get_offer(&mut ctx) {
                offers.push(MerchantOffer::new(
                    offer.wants,
                    offer.additional_wants,
                    offer.gives,
                    offer.max_uses,
                    offer.xp,
                    offer.reputation_discount,
                ));
            }
        }

        offers
    }
}
