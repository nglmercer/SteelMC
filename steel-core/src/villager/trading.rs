//! Trading primitives for villagers and wandering traders.
//!
//! Mirrors `net.minecraft.world.item.trading.MerchantOffer` / `MerchantOffers`
//! at the gameplay level (cost, demand, special price, XP, restock).

use steel_registry::item_stack::ItemStack;
#[expect(unused_imports, reason = "phase-6 stub retains identifier for future trade-set registry")]
use steel_utils::Identifier;

/// A single villager trade offer.
///
/// Simplified relative to vanilla codec but preserves all gameplay-visible
/// fields: buy A, optional buy B, sell, uses, maxUses, demand, specialPrice,
/// priceMultiplier, xp, rewardExp.
#[derive(Debug, Clone)]
pub struct MerchantOffer {
    pub buy_a: ItemStack,
    pub buy_b: Option<ItemStack>,
    pub sell: ItemStack,
    pub uses: i32,
    pub max_uses: i32,
    pub demand: i32,
    pub special_price: i32,
    pub price_multiplier: f32,
    pub xp: i32,
    pub reward_xp: bool,
}

impl MerchantOffer {
    #[must_use]
    pub fn new(buy_a: ItemStack, buy_b: Option<ItemStack>, sell: ItemStack, max_uses: i32, xp: i32, price_multiplier: f32) -> Self {
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

    #[must_use]
    pub fn is_out_of_stock(&self) -> bool {
        self.uses >= self.max_uses
    }

    #[must_use]
    pub fn needs_restock(&self) -> bool {
        self.uses > 0
    }

    pub fn reset_uses(&mut self) {
        self.uses = 0;
    }

    pub fn increase_uses(&mut self) {
        self.uses += 1;
    }

    pub fn update_demand(&mut self) {
        // vanilla: demand += uses - (maxUses - uses)
        self.demand += self.uses - (self.max_uses - self.uses);
    }

    pub fn reset_special_price(&mut self) {
        self.special_price = 0;
    }

    pub fn add_special_price(&mut self, delta: i32) {
        self.special_price += delta;
    }

    #[must_use]
    pub fn adjusted_buy_a_count(&self) -> i32 {
        let base = self.buy_a.count() as i32;
        let demand_component = (base as f32 * self.demand as f32 * self.price_multiplier).floor() as i32;
        let adjusted = base + demand_component.max(0) + self.special_price;
        adjusted.clamp(1, self.buy_a.count() as i32 * 2 + 64).max(1)
    }

    #[must_use]
    pub fn can_trade(&self, offered_a: &ItemStack, offered_b: &ItemStack) -> bool {
        if self.is_out_of_stock() {
            return false;
        }
        if offered_a.item() != self.buy_a.item() || offered_a.count() < self.adjusted_buy_a_count() {
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
    pub offers: Vec<MerchantOffer>,
}

impl MerchantOffers {
    #[must_use]
    pub fn new() -> Self {
        Self { offers: Vec::new() }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }

    pub fn push(&mut self, offer: MerchantOffer) {
        self.offers.push(offer);
    }

    pub fn reset_all_uses(&mut self) {
        for o in &mut self.offers {
            o.reset_uses();
        }
    }

    pub fn update_all_demand(&mut self) {
        for o in &mut self.offers {
            o.update_demand();
        }
    }

    pub fn reset_all_special_prices(&mut self) {
        for o in &mut self.offers {
            o.reset_special_price();
        }
    }

    #[must_use]
    pub fn needs_restock(&self) -> bool {
        self.offers.iter().any(|o| o.needs_restock())
    }

    /// Generates a minimal deterministic set of offers for a profession/level.
    ///
    /// Real vanilla uses loot tables / `VillagerTrades` registries; this
    /// provides a gameplay-plausible stub that can be replaced when extractor
    /// data is available, while keeping profession/level progression testable.
    #[must_use]
    pub fn generate_for_profession(profession_id: i32, level: i32) -> Self {
        // Deterministic stub: 2 offers per level with emerald as currency.
        // Uses vanilla item keys so `ItemStack` construction via registry works.
        let mut offers = MerchantOffers::new();
        // Map a few sample sells per profession so trades are not uniform.
        let buy_item = &steel_registry::vanilla_items::EMERALD;
        let sell_item: &steel_registry::items::Item = match profession_id {
            1 => &steel_registry::vanilla_items::IRON_INGOT,      // armorer
            2 => &steel_registry::vanilla_items::COOKED_BEEF,     // butcher
            3 => &steel_registry::vanilla_items::MAP,             // cartographer
            4 => &steel_registry::vanilla_items::REDSTONE,        // cleric
            5 => &steel_registry::vanilla_items::BREAD,           // farmer
            6 => &steel_registry::vanilla_items::COOKED_COD,      // fisherman
            7 => &steel_registry::vanilla_items::ARROW,           // fletcher
            8 => &steel_registry::vanilla_items::LEATHER,         // leatherworker
            9 => &steel_registry::vanilla_items::BOOK,            // librarian
            10 => &steel_registry::vanilla_items::CLAY_BALL,      // mason
            12 => &steel_registry::vanilla_items::WHITE_WOOL,     // shepherd
            13 => &steel_registry::vanilla_items::IRON_AXE,       // toolsmith
            14 => &steel_registry::vanilla_items::IRON_SWORD,     // weaponsmith
            _ => &steel_registry::vanilla_items::EMERALD,
        };
        // Level influences maxUses and xp (mirrors vanilla 16 uses, xp 10-30)
        for i in 0..(2 * level.clamp(1, 5)) {
            let buy = ItemStack::with_count(buy_item, 1 + i);
            let sell = ItemStack::with_count(sell_item, 1);
            offers.push(MerchantOffer::new(buy, None, sell, 12 + level * 2, 5 + level * 2, 0.05));
        }
        offers
    }
}
