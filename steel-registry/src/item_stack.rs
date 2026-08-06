//! Item stack implementation.

use std::{
    borrow::Cow,
    io::{Cursor, Result, Write},
};

use rand::RngExt;

use steel_utils::{
    DowncastType, Identifier,
    codec::VarInt,
    java,
    random::{Random, legacy_random::LegacyRandom, xoroshiro::Xoroshiro},
    serial::{ReadFrom, WriteTo},
};
use text_components::TextComponent;

use crate::{
    REGISTRY, RegistryEntry, RegistryExt, RegistryReference, TaggedRegistryExt as _,
    damage_type::DamageTypeRef,
    data_components::{
        Component, ComponentData, ComponentPatchEntry, CustomData, DataComponentMap,
        DataComponentPatch, DataComponentType,
        vanilla_components::{
            ADDITIONAL_TRADE_COST, ATTACK_RANGE, ATTRIBUTE_MODIFIERS, AttackRange, BLOCK_STATE,
            BLOCKS_ATTACKS, BUNDLE_CONTENTS, BlockItemStateProperties, CHARGED_PROJECTILES,
            CONTAINER, CUSTOM_DATA, CUSTOM_NAME, DAMAGE, DAMAGE_RESISTANT, DAMAGE_TYPE,
            ENCHANTABLE, ENCHANTMENTS, EQUIPPABLE, Equippable, ITEM_NAME, ItemAttributeModifiers,
            ItemEnchantments, MAX_DAMAGE, MAX_STACK_SIZE, MINIMUM_ATTACK_CHARGE,
            OMINOUS_BOTTLE_AMPLIFIER, OminousBottleAmplifier, PIERCING_WEAPON, POTION_CONTENTS,
            PiercingWeapon, PotionContents, REPAIRABLE, STORED_ENCHANTMENTS,
            SUSPICIOUS_STEW_EFFECTS, SuspiciousStewEffect, SuspiciousStewEffects, TOOL, Tool,
            UNBREAKABLE, WEAPON, WRITTEN_BOOK_CONTENT, Weapon,
        },
    },
    enchantment_effect::EnchantmentEffectComponent,
    equipment::EquipmentSlot,
    item_stack_template::ItemStackTemplate,
    items::{Item, ItemRef},
    vanilla_items,
};

/// A stack of items with a count and component modifications.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    /// The item type. AIR represents an empty stack.
    pub item: ItemRef,
    /// The number of items in this stack.
    pub count: i32,
    /// Modifications to the prototype components.
    patch: DataComponentPatch,
}

impl Default for ItemStack {
    fn default() -> Self {
        Self::empty()
    }
}

impl ItemStack {
    /// Creates an empty item stack (using AIR).
    #[must_use]
    pub fn empty() -> Self {
        Self {
            item: &vanilla_items::AIR,
            count: 0,
            patch: DataComponentPatch::new(),
        }
    }

    /// Creates a new item stack with count 1.
    #[must_use]
    pub fn new(item: ItemRef) -> Self {
        Self::with_count(item, 1)
    }

    /// Creates a new item stack with the specified count.
    #[must_use]
    pub fn with_count(item: ItemRef, count: i32) -> Self {
        Self {
            item,
            count,
            patch: DataComponentPatch::new(),
        }
    }

    /// Creates a new item stack with the specified count and component patch.
    #[must_use]
    pub fn with_count_and_patch(item: ItemRef, count: i32, mut patch: DataComponentPatch) -> Self {
        patch.sanitize_against(&item.components);
        Self { item, count, patch }
    }

    #[must_use]
    const fn prototype(&self) -> &'static DataComponentMap {
        &self.item.components
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.item == &*vanilla_items::AIR || self.count <= 0
    }

    #[must_use]
    pub fn item(&self) -> ItemRef {
        if self.is_empty() {
            &vanilla_items::AIR
        } else {
            self.item
        }
    }

    #[must_use]
    pub fn count(&self) -> i32 {
        if self.is_empty() { 0 } else { self.count }
    }

    #[must_use]
    pub const fn components_patch(&self) -> &DataComponentPatch {
        &self.patch
    }

    pub const fn set_count(&mut self, count: i32) {
        self.count = count;
    }

    /// Increases the count by the given amount.
    pub const fn grow(&mut self, amount: i32) {
        self.count += amount;
    }

    /// Decreases the count by the given amount.
    pub const fn shrink(&mut self, amount: i32) {
        self.count -= amount;
    }

    /// Splits off the specified amount from this stack and returns it as a new stack.
    ///
    /// If the amount is greater than or equal to the current count, this stack becomes
    /// empty and the entire contents are returned.
    pub fn split(&mut self, amount: i32) -> Self {
        let take = amount.min(self.count);
        let result = Self {
            item: self.item,
            count: take,
            patch: self.patch.clone(),
        };
        self.shrink(take);
        result
    }

    /// Copies the identity (item type and patch) from another stack.
    ///
    /// Used when splitting stacks to preserve components.
    #[must_use]
    pub fn copy_with_count(&self, count: i32) -> Self {
        if self.is_empty() {
            Self::empty()
        } else {
            Self {
                item: self.item,
                count,
                patch: self.patch.clone(),
            }
        }
    }

    /// Returns true if this item can stack (max stack size > 1 and not damaged).
    /// Damaged items cannot stack.
    #[must_use]
    pub fn is_stackable(&self) -> bool {
        self.max_stack_size() > 1 && (!self.is_damageable_item() || !self.is_damaged())
    }

    /// Returns true if this item can take damage.
    #[must_use]
    pub fn is_damageable_item(&self) -> bool {
        self.has(MAX_DAMAGE) && !self.has(UNBREAKABLE) && self.has(DAMAGE)
    }

    /// Returns true if this item has taken damage.
    #[must_use]
    pub fn is_damaged(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() > 0
    }

    /// Gets the current damage value of this item.
    #[must_use]
    pub fn get_damage_value(&self) -> i32 {
        self.get(DAMAGE)
            .copied()
            .unwrap_or(0)
            .clamp(0, self.get_max_damage())
    }

    /// Sets the damage value of this item.
    pub fn set_damage_value(&mut self, value: i32) {
        let clamped = value.clamp(0, self.get_max_damage());
        self.set(DAMAGE, clamped);
    }

    /// Vanilla `Item.getUseDuration`: how many ticks a use of this stack lasts.
    ///
    /// Consumables use their own duration; items that block attacks or are kinetic weapons
    /// are held indefinitely (vanilla's 72000-tick sentinel); everything else is 0.
    #[must_use]
    pub fn get_use_duration(&self) -> i32 {
        use crate::data_components::vanilla_components::{CONSUMABLE, KINETIC_WEAPON};

        /// Vanilla's "held until released" sentinel (one hour of ticks).
        const INDEFINITE_USE_TICKS: i32 = 72_000;

        if let Some(consumable) = self.get(CONSUMABLE) {
            return steel_utils::java::round_to_i32(consumable.consume_seconds() * 20.0);
        }
        if self.has(BLOCKS_ATTACKS) || self.has(KINETIC_WEAPON) {
            return INDEFINITE_USE_TICKS;
        }
        0
    }

    /// Gets the maximum damage this item can take before breaking.
    #[must_use]
    pub fn get_max_damage(&self) -> i32 {
        self.get(MAX_DAMAGE).copied().unwrap_or(0)
    }

    /// Returns true if the item is broken (damage >= max damage).
    #[must_use]
    pub fn is_broken(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() >= self.get_max_damage()
    }

    /// Returns vanilla `ItemStack.nextDamageWillBreak()`.
    #[must_use]
    pub fn next_damage_will_break(&self) -> bool {
        self.is_damageable_item() && self.get_damage_value() >= self.get_max_damage() - 1
    }

    /// Damages the item and breaks it if durability reaches zero.
    ///
    /// Returns `true` if the item broke and should be removed/replaced.
    pub fn hurt_and_break(&mut self, amount: i32, has_infinite_materials: bool) -> bool {
        let mut random = Xoroshiro::from_seed_unmixed(rand::rng().random());
        self.hurt_and_break_with_random(amount, has_infinite_materials, &mut random)
    }

    /// Damages the item using the supplied random source for data-driven
    /// `minecraft:item_damage` enchantment effects.
    ///
    /// Returns `true` if the item broke and should be removed/replaced.
    pub fn hurt_and_break_with_random(
        &mut self,
        amount: i32,
        has_infinite_materials: bool,
        random: &mut impl Random,
    ) -> bool {
        if !self.is_damageable_item() {
            return false;
        }

        if has_infinite_materials {
            return false;
        }

        let effective_amount = self.process_durability_change(amount, random);

        if effective_amount == 0 {
            return false;
        }

        let new_damage = self.get_damage_value() + effective_amount;

        // DEFERRED (Phase 4-8): Trigger the ITEM_DURABILITY_CHANGED advancement criterion.
        // Needs the advancement system, and a player handle this crate cannot reach — the
        // trigger belongs at the steel-core call sites, like the break event below.

        self.set_damage_value(new_damage);

        if self.is_broken() {
            // Returning `true` is how the break is reported: steel-core callers that know the
            // equipment slot call `LivingEntity::on_equipped_item_broken`, which broadcasts the
            // break event and refreshes attribute modifiers. This crate cannot reach entities.
            self.shrink(1);
            return true;
        }

        false
    }

    fn process_durability_change(&self, amount: i32, random: &mut impl Random) -> i32 {
        if amount <= 0 {
            return amount;
        }

        let Some(enchantments) = self.get_enchantments() else {
            return amount;
        };

        let mut value = amount as f32;
        for (key, level) in enchantments.iter() {
            if *level == 0 {
                continue;
            }
            let Some(enchantment) = REGISTRY.enchantments.by_key(key) else {
                continue;
            };

            for effect in enchantment.effects.item_damage {
                if effect.requirements.is_some_and(|requirements| {
                    requirements.matches_item_context(self.item()) != Some(true)
                }) {
                    continue;
                }
                value = effect
                    .effect
                    .process_with_random(*level as i32, random, value);
            }
        }

        value as i32
    }

    /// Returns true if this item has the specified component (by type).
    #[must_use]
    pub fn has<T: 'static>(&self, component: DataComponentType<T>) -> bool {
        self.has_component(&component.key)
    }

    /// Returns true if this item has the specified component (by key).
    #[must_use]
    pub fn has_component(&self, key: &Identifier) -> bool {
        match self.patch.get_entry(key) {
            Some(ComponentPatchEntry::Set(_)) => true,
            Some(ComponentPatchEntry::Removed) => false,
            None => self.prototype().get_raw(key).is_some(),
        }
    }

    #[must_use]
    pub fn is_same_item(a: &Self, b: &Self) -> bool {
        a.item().key == b.item().key
    }

    /// Checks if two stacks have the same item and components.
    #[must_use]
    pub fn is_same_item_same_components(a: &Self, b: &Self) -> bool {
        if !Self::is_same_item(a, b) {
            return false;
        }
        if a.is_empty() && b.is_empty() {
            return true;
        }
        a.components_equal(b)
    }

    #[must_use]
    pub fn matches(a: &Self, b: &Self) -> bool {
        a.count() == b.count() && Self::is_same_item_same_components(a, b)
    }

    #[must_use]
    pub fn is(&self, item: ItemRef) -> bool {
        self.item().key == item.key
    }

    #[must_use]
    pub fn max_stack_size(&self) -> i32 {
        self.get(MAX_STACK_SIZE).copied().unwrap_or(1)
    }

    /// Validates the complete stack constraints enforced by Vanilla's `ItemStack.validateStrict`.
    pub fn validate_strict(&self) -> Result<()> {
        let max_stack_size = self.max_stack_size();
        if self.has(MAX_DAMAGE) && max_stack_size > 1 {
            return Err(std::io::Error::other(
                "Item cannot be both damageable and stackable",
            ));
        }

        if let Some(container) = self.get(CONTAINER) {
            validate_contained_item_sizes(container.items().iter().flatten())?;
        }

        if let Some(bundle) = self.get(BUNDLE_CONTENTS) {
            validate_contained_item_sizes(bundle.items())?;
            bundle.validate_weight()?;
        }

        if let Some(projectiles) = self.get(CHARGED_PROJECTILES) {
            validate_contained_item_sizes(projectiles.items())?;
        }

        if self.count > max_stack_size {
            return Err(std::io::Error::other(format!(
                "Item stack with stack size of {} was larger than maximum: {max_stack_size}",
                self.count
            )));
        }
        Ok(())
    }

    /// Returns the equippable component if this item has one.
    #[must_use]
    pub fn get_equippable(&self) -> Option<&Equippable> {
        self.get(EQUIPPABLE)
    }

    /// Returns the item attribute modifiers component.
    #[must_use]
    pub fn get_attribute_modifiers(&self) -> Option<&ItemAttributeModifiers> {
        self.get(ATTRIBUTE_MODIFIERS)
    }

    /// Returns the equipment slot this item can be equipped to, if any.
    #[must_use]
    pub fn get_equippable_slot(&self) -> Option<EquipmentSlot> {
        self.get_equippable().map(|e| e.slot)
    }

    /// Returns true if this item can be equipped in the given slot.
    #[must_use]
    pub fn is_equippable_in_slot(&self, slot: EquipmentSlot) -> bool {
        self.get_equippable_slot() == Some(slot)
    }

    /// Gets the raw component data by key.
    #[must_use]
    pub fn get_effective_value_raw(&self, key: &Identifier) -> Option<&ComponentData> {
        match self.patch.get_entry(key) {
            Some(ComponentPatchEntry::Set(data)) => Some(data),
            Some(ComponentPatchEntry::Removed) => None,
            None => self.prototype().get_raw(key),
        }
    }

    /// Gets the effective value of a component, considering the patch and prototype.
    /// Returns `None` if the component is not present or has been removed.
    #[must_use]
    pub fn get<T: Component + DowncastType>(&self, component: DataComponentType<T>) -> Option<&T> {
        let data = self.get_effective_value_raw(&component.key)?;
        data.downcast_ref::<T>()
    }

    /// Gets the effective value of a component, or returns the default value if not present.
    #[must_use]
    pub fn get_or_default<T: Component + DowncastType + Clone>(
        &self,
        component: DataComponentType<T>,
        default: T,
    ) -> T {
        self.get(component).cloned().unwrap_or(default)
    }

    /// Sets a component value in this item's patch, overriding the prototype.
    pub fn set<T: Component + DowncastType>(&mut self, component: DataComponentType<T>, value: T) {
        let value = ComponentData::new(value);
        let is_default = self.prototype().get_raw(&component.key) == Some(&value);
        if is_default {
            self.patch.clear(component);
        } else {
            self.patch.set_component_data(component.key, value);
        }
    }

    /// Removes a component from this item (marks it as removed in the patch).
    /// This will hide the component even if it exists in the prototype.
    pub fn remove<T: 'static>(&mut self, component: DataComponentType<T>) {
        if self.prototype().get_raw(&component.key).is_some() {
            self.patch.remove(component);
        } else {
            self.patch.clear(component);
        }
    }

    /// Clears any patch entry for this component (neither set nor removed).
    /// The prototype value will be visible again.
    pub fn clear<T: 'static>(&mut self, component: DataComponentType<T>) {
        self.patch.clear(component);
    }

    /// Returns a reference to the component patch.
    #[must_use]
    pub const fn patch(&self) -> &DataComponentPatch {
        &self.patch
    }

    /// Gets the Tool component if present.
    #[must_use]
    pub fn get_tool(&self) -> Option<&Tool> {
        self.get(TOOL)
    }

    /// Gets the Weapon component if present.
    #[must_use]
    pub fn get_weapon(&self) -> Option<&Weapon> {
        self.get(WEAPON)
    }

    /// Gets the `AttackRange` component if present.
    #[must_use]
    pub fn get_attack_range(&self) -> Option<&AttackRange> {
        self.get(ATTACK_RANGE)
    }

    /// Returns vanilla `DataComponents.MINIMUM_ATTACK_CHARGE`, defaulting to 0.
    #[must_use]
    pub fn minimum_attack_charge(&self) -> f32 {
        self.get(MINIMUM_ATTACK_CHARGE).copied().unwrap_or(0.0)
    }

    /// Gets the vanilla damage type component if present.
    #[must_use]
    pub fn get_damage_type(&self) -> Option<DamageTypeRef> {
        self.get(DAMAGE_TYPE).map(|component| component.damage_type)
    }

    /// Returns vanilla `ItemStack.canBeHurtBy` for a damage type.
    #[must_use]
    pub fn can_be_hurt_by(&self, damage_type: DamageTypeRef) -> bool {
        self.get(DAMAGE_RESISTANT)
            .is_none_or(|resistance| !resistance.is_resistant_to(damage_type))
    }

    /// Returns vanilla `ItemStack.isValidRepairItem`.
    #[must_use]
    pub fn is_valid_repair_item(&self, repair_item: &Self) -> bool {
        self.get(REPAIRABLE)
            .is_some_and(|repairable| repairable.is_valid_repair_item(repair_item))
    }

    /// Returns whether this item has the vanilla piercing weapon component.
    #[must_use]
    pub fn is_piercing_weapon(&self) -> bool {
        self.has(PIERCING_WEAPON)
    }

    /// Gets the `PiercingWeapon` component if present.
    #[must_use]
    pub fn get_piercing_weapon(&self) -> Option<&PiercingWeapon> {
        self.get(PIERCING_WEAPON)
    }

    /// Returns the mining speed for the given block state ID.
    /// If no Tool component is present, returns 1.0 (hand speed).
    #[must_use]
    pub fn get_destroy_speed(&self, block_state_id: steel_utils::BlockStateId) -> f32 {
        self.get_tool()
            .map_or(1.0, |tool| tool.get_mining_speed(block_state_id))
    }

    /// Returns true if this tool is correct for getting drops from the block.
    #[must_use]
    pub fn is_correct_tool_for_drops(&self, block_state_id: steel_utils::BlockStateId) -> bool {
        self.get_tool()
            .is_some_and(|tool| tool.is_correct_for_drops(block_state_id))
    }

    /// Returns the damage per block for this tool (how much durability is consumed per block mined).
    /// Returns 0 if no Tool component is present.
    #[must_use]
    pub fn get_tool_damage_per_block(&self) -> i32 {
        self.get_tool().map_or(0, |tool| tool.damage_per_block)
    }

    /// Returns true if this tool can destroy blocks in creative mode.
    /// Returns true if no Tool component is present (default behavior).
    #[must_use]
    pub fn can_destroy_blocks_in_creative(&self) -> bool {
        self.get_tool()
            .is_none_or(|tool| tool.can_destroy_blocks_in_creative)
    }

    #[must_use]
    pub fn get_enchantment_level(&self, enchantment: &Identifier) -> i32 {
        self.get_enchantments()
            .map_or(0, |e| e.get_level(enchantment) as i32)
    }

    #[must_use]
    pub fn get_enchantments(&self) -> Option<&ItemEnchantments> {
        self.get(ENCHANTMENTS)
    }

    /// Vanilla `EnchantmentHelper.getEnchantmentsForCrafting`: enchanted books
    /// expose `STORED_ENCHANTMENTS` to crafting operations, while every other
    /// item exposes `ENCHANTMENTS`.
    #[must_use]
    pub fn get_enchantments_for_crafting(&self) -> Option<&ItemEnchantments> {
        self.get(self.enchantment_component())
    }

    /// Vanilla `EnchantmentHelper.getComponentType`.
    #[must_use]
    fn enchantment_component(&self) -> DataComponentType<ItemEnchantments> {
        if self.is(&vanilla_items::ENCHANTED_BOOK) {
            STORED_ENCHANTMENTS
        } else {
            ENCHANTMENTS
        }
    }

    /// Mirrors Vanilla's component-based `ItemStack.isEnchantable` check.
    #[must_use]
    pub fn is_enchantable(&self) -> bool {
        self.has(ENCHANTABLE)
            && self
                .get(ENCHANTMENTS)
                .is_some_and(ItemEnchantments::is_empty)
    }

    #[must_use]
    pub fn has_enchantment_effect(&self, component: EnchantmentEffectComponent) -> bool {
        let Some(enchantments) = self.get_enchantments() else {
            return false;
        };

        for (key, level) in enchantments.iter() {
            if *level == 0 {
                continue;
            }
            let Some(enchantment) = REGISTRY.enchantments.by_key(key) else {
                continue;
            };
            if enchantment.effects.has(component) {
                return true;
            }
        }

        false
    }

    #[must_use]
    pub fn apply_unconditional_enchantment_value_effects(
        &self,
        component: EnchantmentEffectComponent,
        input: f32,
    ) -> f32 {
        let Some(enchantments) = self.get_enchantments() else {
            return input;
        };

        let mut value = input;
        for (key, level) in enchantments.iter() {
            if *level == 0 {
                continue;
            }
            let Some(enchantment) = REGISTRY.enchantments.by_key(key) else {
                continue;
            };
            let level = *level as i32;

            for effect in enchantment.effects.value_effects(component) {
                if !effect.is_unconditional() {
                    continue;
                }
                if let Some(updated) = effect.effect.process_without_random(level, value) {
                    value = updated;
                }
            }

            let Some(effect) = enchantment.effects.single_value_effect(component) else {
                continue;
            };
            if let Some(updated) = effect.process_without_random(level, value) {
                value = updated;
            }
        }

        value
    }

    /// Sets the damage/durability as a fraction (0.0 = broken, 1.0 = full).
    /// If `add` is true, the fraction is applied on top of the remaining durability.
    ///
    /// Vanilla `SetItemDamageFunction.run`.
    pub fn set_damage_fraction(&mut self, fraction: f32, add: bool) {
        if !self.is_damageable_item() {
            return;
        }
        let max_damage = self.get_max_damage();
        let base = if add {
            1.0 - (self.get_damage_value() as f32 / max_damage as f32)
        } else {
            0.0
        };
        let remaining = 1.0 - (fraction + base).clamp(0.0, 1.0);
        self.set_damage_value((remaining * max_damage as f32).floor() as i32);
    }

    /// Resolves an `EnchantmentOptions` to the candidate enchantments it names.
    ///
    /// `None` in the options position means "every registered enchantment", matching
    /// vanilla's `options.orElseGet(... listElements())`.
    fn enchantment_candidates(
        options: &crate::loot_table::EnchantmentOptions,
    ) -> Vec<crate::enchantment::EnchantmentRef> {
        match options {
            crate::loot_table::EnchantmentOptions::Tag(tag) => {
                REGISTRY.enchantments.get_tag(tag).unwrap_or_default()
            }
            crate::loot_table::EnchantmentOptions::List(keys) => keys
                .iter()
                .filter_map(|key| REGISTRY.enchantments.by_key(key))
                .collect(),
        }
    }

    /// Vanilla `ItemStack.enchant` turns a plain book into an enchanted book first.
    fn promote_book_for_enchanting(&mut self) {
        if self.is(&vanilla_items::BOOK) {
            self.item = &vanilla_items::ENCHANTED_BOOK;
            self.patch.sanitize_against(&self.item.components);
        }
    }

    /// Enchants this item with one random enchantment from the given options.
    ///
    /// Vanilla `EnchantRandomlyFunction.run`: filters to enchantments that can apply
    /// (unless the target is a book), picks one uniformly, then rolls a level in
    /// `[min_level, max_level]`.
    pub fn enchant_randomly<R: rand::Rng>(
        &mut self,
        options: &crate::loot_table::EnchantmentOptions,
        only_compatible: bool,
        include_additional_cost_component: bool,
        rng: &mut R,
    ) {
        let target_is_book = self.is(&vanilla_items::BOOK);
        let check_compatibility = !target_is_book && only_compatible;
        let candidates: Vec<_> = Self::enchantment_candidates(options)
            .into_iter()
            .filter(|candidate| !check_compatibility || candidate.can_enchant(self.item))
            .collect();

        let Some(chosen) = candidates.get(rng.random_range(0..candidates.len().max(1))) else {
            return;
        };
        // Vanilla `Enchantment.getMinLevel` is always 1.
        let level = rng.random_range(1..=chosen.max_level);

        self.promote_book_for_enchanting();
        self.upgrade_enchantment(chosen.key.clone(), level);

        if include_additional_cost_component {
            let level = level as i32;
            let surcharge = 2 + rng.random_range(0..(5 + level * 10)) + 3 * level;
            self.set(ADDITIONAL_TRADE_COST, surcharge);
        }
    }

    /// Enchants this item as if using an enchanting table at the given level.
    ///
    /// Vanilla `EnchantWithLevelsFunction.run` → `EnchantmentHelper.enchantItem`.
    pub fn enchant_with_levels<R: rand::Rng>(
        &mut self,
        level: i32,
        options: &crate::loot_table::EnchantmentOptions,
        include_additional_cost_component: bool,
        rng: &mut R,
    ) {
        let candidates = Self::enchantment_candidates(options);
        // Vanilla drives this from the level's `RandomSource`, which is Java's LCG; seed a
        // `LegacyRandom` from the loot RNG so the bounded-int arithmetic stays java-exact.
        let mut random = LegacyRandom::from_seed(rng.random());
        let selected = crate::enchantment::selection::select_enchantment(
            &mut random,
            self,
            level,
            &candidates,
        );
        if selected.is_empty() {
            return;
        }

        self.promote_book_for_enchanting();
        for instance in selected {
            self.upgrade_enchantment(instance.enchantment.key.clone(), instance.level);
        }

        if include_additional_cost_component && level > 0 {
            self.set(ADDITIONAL_TRADE_COST, level);
        }
    }

    /// Copies components from a source (currently only the block entity) to this item.
    ///
    /// Vanilla `CopyComponentsFunction.run`: for each listed component present on the
    /// source, set it on the item. This is what lets a named or locked container keep its
    /// name, lock, and contents when broken.
    pub fn copy_components<R: rand::Rng>(
        &mut self,
        source: crate::loot_table::CopySource,
        include: &[Identifier],
        ctx: &crate::loot_table::LootContext<'_, R>,
    ) {
        // Every vanilla use is `source: block_entity`; entity sources would need component
        // snapshots on `EntityRef`, which nothing populates yet.
        if !matches!(source, crate::loot_table::CopySource::BlockEntity) {
            return;
        }
        let Some(components) = ctx.block_entity.and_then(|entity| entity.components) else {
            return;
        };

        for key in include {
            if let Some(ComponentPatchEntry::Set(data)) = components.get_entry(key) {
                self.patch.set_raw(key.clone(), data.clone());
            }
        }
    }

    /// Copies block state properties to this item (for blocks like `note_block`).
    ///
    /// Vanilla `CopyBlockState.run`: each named property present on the source state is
    /// written into the item's `BLOCK_STATE` component; missing properties are skipped.
    pub fn copy_block_state<R: rand::Rng>(
        &mut self,
        _block: &Identifier,
        properties: &[&str],
        ctx: &crate::loot_table::LootContext<'_, R>,
    ) {
        use crate::blocks::block_state_ext::BlockStateExt as _;

        let Some(state) = ctx.block_state else {
            return;
        };

        let mut item_state = self
            .get(BLOCK_STATE)
            .cloned()
            .unwrap_or_else(BlockItemStateProperties::empty);
        let mut copied = item_state.properties().clone();
        for property in properties {
            if let Some(value) = state.get_property_str(property) {
                copied.insert((*property).to_owned(), value);
            }
        }
        item_state = BlockItemStateProperties::new(copied);

        if !item_state.is_empty() {
            self.set(BLOCK_STATE, item_state);
        }
    }

    /// Sets components from the datapack's JSON component map.
    ///
    /// Vanilla `SetComponentsFunction.run`. The payload is parsed as SNBT, which accepts the
    /// quoted-key/quoted-string JSON that datapacks emit; each entry is then decoded by its
    /// component's own persistent codec. Entries that are unknown or fail to decode are
    /// skipped rather than applied partially.
    ///
    /// Note: a component whose JSON codec differs from its NBT codec (typed number suffixes,
    /// for example) will not round-trip through this path. No vanilla loot table currently
    /// hits that case — all uses are `minecraft:trim` — but a future one could, so this
    /// should move to a real JSON codec if the extractor starts emitting richer payloads.
    pub fn set_components_from_json(&mut self, components: &str) {
        let Ok(compound) = steel_utils::nbt::parse_snbt_compound(components) else {
            return;
        };

        for (key, value) in compound.iter() {
            let Ok(id) = key.to_str().parse::<Identifier>() else {
                continue;
            };
            let Some(entry) = REGISTRY.data_components.by_key(&id) else {
                continue;
            };
            if let Some(data) = entry.read_nbt_owned(value) {
                self.patch.set_raw(id, data);
            }
        }
    }

    /// Merges custom NBT data into this item's `custom_data` component.
    pub fn set_custom_data(&mut self, value: &CustomData) {
        let merged = self
            .get(CUSTOM_DATA)
            .cloned()
            .unwrap_or_default()
            .merged_with(value);
        if merged.is_empty() {
            self.remove(CUSTOM_DATA);
        } else {
            self.set(CUSTOM_DATA, merged);
        }
    }

    /// Applies furnace smelting to convert this item (e.g., raw iron -> iron ingot).
    pub fn apply_furnace_smelt(&mut self, use_input_count: bool) {
        if let Some(result) = REGISTRY.recipes.find_smelting_result(self, use_input_count) {
            *self = result;
        }
    }

    /// Creates an exploration map pointing to a structure.
    pub const fn create_exploration_map(
        &mut self,
        _destination: &Identifier,
        _decoration: &Identifier,
        _zoom: i32,
        _skip_existing_chunks: bool,
    ) {
        // DEFERRED (Phase 4-8): Build the exploration map (swap to `filled_map`, set
        // `MAP_DECORATIONS` and the destination). `MapDecorations` is ready, but this needs a
        // synchronous structure-locate against the world, which only steel-core can do —
        // `/locate` runs its search asynchronously across suspended chunk requests.
        // 13 vanilla uses (3 loot tables, 10 cartographer trades).
    }

    /// Sets the custom name or item name of this item.
    ///
    /// Vanilla `SetNameFunction.run`. `name` is the datapack's JSON text component; a
    /// malformed payload is ignored rather than replacing the name with an error string.
    pub fn set_name(&mut self, name: &str, target: crate::loot_table::NameTarget) {
        let Ok(component) = serde_json::from_str::<TextComponent>(name) else {
            return;
        };
        match target {
            crate::loot_table::NameTarget::CustomName => self.set(CUSTOM_NAME, component),
            crate::loot_table::NameTarget::ItemName => self.set(ITEM_NAME, component),
        }
    }

    /// Sets the ominous bottle amplifier component.
    pub fn set_ominous_bottle_amplifier(&mut self, amplifier: i32) {
        self.set(
            OMINOUS_BOTTLE_AMPLIFIER,
            OminousBottleAmplifier::new(amplifier),
        );
    }

    /// Sets the potion type for this item.
    ///
    /// Vanilla `SetPotionFunction.run`: updates `POTION_CONTENTS` from `EMPTY` via
    /// `withPotion`, preserving any custom color/effects/name already present.
    pub fn set_potion(&mut self, id: &Identifier) {
        let Some(potion) = REGISTRY.potions.by_key(id) else {
            return;
        };
        let updated = self
            .get(POTION_CONTENTS)
            .cloned()
            .unwrap_or_else(PotionContents::empty)
            .with_potion(RegistryReference::new(potion));
        self.set(POTION_CONTENTS, updated);
    }

    /// Dyes this item with `rolls` randomly chosen dye colors, averaged together.
    ///
    /// Vanilla `SetRandomDyesFunction.run` -> `DyedItemColor.applyDyes`: each dye contributes
    /// its diffuse color, and the mean is rescaled so the brightest channel keeps the mean
    /// intensity of the inputs.
    pub fn set_random_dyes<R: rand::Rng>(&mut self, rolls: i32, rng: &mut R) {
        use crate::data_components::vanilla_components::{DYED_COLOR, DyedItemColor};
        use crate::dye_color::DyeColor;

        if rolls <= 0 {
            return;
        }

        let mut totals = [0i32; 3];
        let mut intensity_total = 0i32;
        let mut count = 0i32;
        let mut accumulate = |color: i32| {
            let (red, green, blue) = ((color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF);
            intensity_total += red.max(green.max(blue));
            totals[0] += red;
            totals[1] += green;
            totals[2] += blue;
            count += 1;
        };

        if let Some(current) = self.get(DYED_COLOR) {
            accumulate(current.rgb());
        }
        for _ in 0..rolls {
            let dye = DyeColor::VALUES[rng.random_range(0..DyeColor::VALUES.len())];
            accumulate(dye.texture_diffuse_color());
        }
        if count == 0 {
            return;
        }

        let mean = [totals[0] / count, totals[1] / count, totals[2] / count];
        let average_intensity = intensity_total as f32 / count as f32;
        let result_intensity = mean[0].max(mean[1].max(mean[2])) as f32;
        if result_intensity <= 0.0 {
            self.set(DYED_COLOR, DyedItemColor::new(0));
            return;
        }

        let scale = |channel: i32| (channel as f32 * average_intensity / result_intensity) as i32;
        self.set(
            DYED_COLOR,
            DyedItemColor::new((scale(mean[0]) << 16) | (scale(mean[1]) << 8) | scale(mean[2])),
        );
    }

    /// Sets a random potion, optionally restricted to a potion tag.
    ///
    /// Vanilla `SetRandomPotionFunction.run`. With no tag, any registered potion may be chosen.
    pub fn set_random_potion<R: rand::Rng>(&mut self, options: Option<&Identifier>, rng: &mut R) {
        let candidates: Vec<_> = match options {
            Some(tag) => REGISTRY.potions.get_tag(tag).unwrap_or_default(),
            None => REGISTRY.potions.iter().map(|(_, potion)| potion).collect(),
        };
        if candidates.is_empty() {
            return;
        }

        let potion = candidates[rng.random_range(0..candidates.len())];
        let updated = self
            .get(POTION_CONTENTS)
            .cloned()
            .unwrap_or_else(PotionContents::empty)
            .with_potion(RegistryReference::new(potion));
        self.set(POTION_CONTENTS, updated);
    }

    /// Adds one randomly chosen suspicious stew effect to this item.
    ///
    /// Vanilla `SetStewEffectFunction.run`: only applies to suspicious stew, picks a single
    /// entry at random, and converts the duration from seconds to ticks unless the effect is
    /// instantaneous.
    pub fn set_stew_effects<R: rand::Rng>(
        &mut self,
        effects: &[crate::loot_table::StewEffect],
        rng: &mut R,
    ) {
        if !self.is(&vanilla_items::SUSPICIOUS_STEW) || effects.is_empty() {
            return;
        }

        let entry = &effects[rng.random_range(0..effects.len())];
        let Some(effect) = REGISTRY.mob_effects.by_key(&entry.effect_type) else {
            return;
        };

        let mut duration = entry.duration.get_int(rng);
        if !effect.is_instantaneous() {
            duration *= 20;
        }

        let mut current = self
            .get(SUSPICIOUS_STEW_EFFECTS)
            .cloned()
            .unwrap_or_else(SuspiciousStewEffects::empty);
        let mut entries = current.effects().to_vec();
        entries.push(SuspiciousStewEffect::new(effect, duration));
        current = SuspiciousStewEffects::new(entries);
        self.set(SUSPICIOUS_STEW_EFFECTS, current);
    }

    pub fn set_enchantments(&mut self, enchantments: &[(Identifier, u32)], add: bool) {
        let mut current = self
            .get(self.enchantment_component())
            .cloned()
            .unwrap_or_else(ItemEnchantments::empty);

        for (key, level) in enchantments {
            if add {
                let existing = current.get_level(key);
                current.set(key.clone(), existing + *level);
            } else {
                current.set(key.clone(), *level);
            }
        }

        self.set(self.enchantment_component(), current);
    }

    /// Vanilla `ItemStack.enchant` → `Mutable.upgrade`: keeps the higher of existing vs new level.
    pub fn upgrade_enchantment(&mut self, enchantment: Identifier, level: u32) {
        let mut current = self
            .get(self.enchantment_component())
            .cloned()
            .unwrap_or_else(ItemEnchantments::empty);
        current.upgrade(enchantment, level);
        self.set(self.enchantment_component(), current);
    }

    /// Changes the item type entirely.
    pub fn set_item(&mut self, new_item: &Identifier) {
        if let Some(item_ref) = REGISTRY.items.by_key(new_item) {
            self.item = item_ref;
            self.patch.sanitize_against(&item_ref.components);
        }
    }

    // The remaining loot-function setters below are deliberately unimplemented. None of
    // them is reachable from vanilla data: a sweep of all 1355 vanilla loot tables and 388
    // villager trades finds zero uses of `set_lore`, `set_contents`, `modify_contents`,
    // `set_loot_table`, `set_attributes`, `fill_player_head`, `copy_custom_data`,
    // `set_banner_pattern`, `set_fireworks`, `set_firework_explosion`, `set_book_cover`,
    // `set_written_book_pages`, `set_writable_book_pages`, or `copy_name`. The backing
    // components all exist, so each becomes a small implementation once datapacks or
    // plugins can supply tables that reach them.

    /// Copies the name from a source entity/block to this item.
    pub const fn copy_name<R: rand::Rng>(
        &mut self,
        _source: crate::loot_table::CopySource,
        _ctx: &crate::loot_table::LootContext<'_, R>,
    ) {
    }

    /// Sets lore lines on this item.
    pub const fn set_lore(&mut self, _lore: &[&str], _mode: crate::loot_table::ListOperation) {}

    /// Sets container inventory contents.
    pub const fn set_contents<R: rand::Rng>(
        &mut self,
        _entries: &[crate::loot_table::LootEntry],
        _component_type: &Identifier,
        _ctx: &mut crate::loot_table::LootContext<'_, R>,
    ) {
    }

    /// Modifies existing container contents.
    pub const fn modify_contents<R: rand::Rng>(
        &mut self,
        _modifier: &[crate::loot_table::ConditionalLootFunction],
        _component_type: &Identifier,
        _ctx: &mut crate::loot_table::LootContext<'_, R>,
    ) {
    }

    /// Sets the container's loot table reference.
    pub const fn set_loot_table(&mut self, _loot_table: &Identifier, _seed: Option<i64>) {}

    /// Sets attribute modifiers on this item.
    pub const fn set_attributes<R: rand::Rng>(
        &mut self,
        _modifiers: &[crate::loot_table::AttributeModifier],
        _replace: bool,
        _rng: &mut R,
    ) {
    }

    /// Fills a player head with texture from an entity.
    pub const fn fill_player_head<R: rand::Rng>(
        &mut self,
        _entity: crate::loot_table::LootContextEntity,
        _ctx: &crate::loot_table::LootContext<'_, R>,
    ) {
    }

    /// Copies custom NBT data from a source.
    pub const fn copy_custom_data<R: rand::Rng>(
        &mut self,
        _source: crate::loot_table::CopySource,
        _operations: &[crate::loot_table::CopyDataOperation],
        _ctx: &crate::loot_table::LootContext<'_, R>,
    ) {
    }

    /// Sets banner pattern layers.
    pub const fn set_banner_pattern(
        &mut self,
        _patterns: &[crate::loot_table::BannerPattern],
        _append: bool,
    ) {
    }

    /// Sets firework rocket properties.
    pub const fn set_fireworks(
        &mut self,
        _explosions: Option<&[crate::loot_table::FireworkExplosion]>,
        _flight_duration: Option<i32>,
    ) {
    }

    /// Sets firework star explosion properties.
    pub const fn set_firework_explosion(
        &mut self,
        _explosion: &crate::loot_table::FireworkExplosion,
    ) {
    }

    /// Sets book cover (title/author for written books).
    pub const fn set_book_cover(
        &mut self,
        _title: Option<&str>,
        _author: Option<&str>,
        _generation: Option<i32>,
    ) {
    }

    /// Sets written book page contents.
    pub const fn set_written_book_pages(
        &mut self,
        _pages: &[&str],
        _mode: crate::loot_table::ListOperation,
    ) {
    }

    /// Sets writable book page contents.
    pub const fn set_writable_book_pages(
        &mut self,
        _pages: &[&str],
        _mode: crate::loot_table::ListOperation,
    ) {
    }

    /// Runs vanilla `ToggleTooltips`: each boolean says whether the component is shown.
    pub fn toggle_tooltips(&mut self, toggles: &[(Identifier, bool)]) {
        use crate::data_components::vanilla_components::{TOOLTIP_DISPLAY, TooltipDisplay};

        let mut display = self
            .get(TOOLTIP_DISPLAY)
            .cloned()
            .unwrap_or(TooltipDisplay::DEFAULT);
        for (component, shown) in toggles {
            display = display.with_hidden_key(component.clone(), !shown);
        }
        self.set(TOOLTIP_DISPLAY, display);
    }

    #[must_use]
    pub fn components_equal(&self, other: &Self) -> bool {
        let mut all_keys = rustc_hash::FxHashSet::default();

        for key in self.prototype().keys() {
            if !self.patch.is_removed(key) {
                all_keys.insert(key);
            }
        }
        for (key, entry) in self.patch.iter() {
            if matches!(entry, ComponentPatchEntry::Set(_)) {
                all_keys.insert(key);
            }
        }
        for key in other.prototype().keys() {
            if !other.patch.is_removed(key) {
                all_keys.insert(key);
            }
        }
        for (key, entry) in other.patch.iter() {
            if matches!(entry, ComponentPatchEntry::Set(_)) {
                all_keys.insert(key);
            }
        }
        for key in all_keys {
            let val_a = self.get_effective_value_raw(key);
            let val_b = other.get_effective_value_raw(key);

            match (val_a, val_b) {
                (Some(a), Some(b)) => {
                    if a != b {
                        return false;
                    }
                }
                (None, None) => {}
                _ => return false,
            }
        }

        true
    }

    /// Vanilla `ItemStack.getCustomName`: an explicit custom name, or a
    /// nonblank written-book title.
    #[must_use]
    pub fn custom_name(&self) -> Option<Cow<'_, TextComponent>> {
        if let Some(name) = self.get(CUSTOM_NAME) {
            return Some(Cow::Borrowed(name));
        }

        let title = self.get(WRITTEN_BOOK_CONTENT)?.title().raw();
        (!java::is_blank(title)).then(|| Cow::Owned(TextComponent::plain(title.to_owned())))
    }

    /// Returns the custom name if set, otherwise the effective `ITEM_NAME`
    /// component.
    ///
    /// This does not apply item-class name overrides such as potion contents;
    /// callers with access to item behaviors should use their behavior-aware
    /// hover-name API.
    #[must_use]
    pub fn custom_or_component_name(&self) -> Cow<'_, TextComponent> {
        self.custom_name()
            .or_else(|| self.get(ITEM_NAME).map(Cow::Borrowed))
            .unwrap_or(Cow::Borrowed(&EMPTY_NAME))
    }
}

static EMPTY_NAME: TextComponent = TextComponent::new();

fn validate_contained_item_sizes<'a>(
    items: impl IntoIterator<Item = &'a ItemStackTemplate>,
) -> Result<()> {
    for item in items {
        let max_stack_size = item.max_stack_size();
        if item.count() > max_stack_size {
            return Err(std::io::Error::other(format!(
                "Item stack with count of {} was larger than maximum: {max_stack_size}",
                item.count()
            )));
        }
    }
    Ok(())
}

impl From<&ItemStack> for ItemStack {
    fn from(stack: &Self) -> Self {
        stack.to_owned()
    }
}

impl From<ItemRef> for ItemStack {
    fn from(item: ItemRef) -> Self {
        Self::new(item)
    }
}

impl From<&'static std::sync::LazyLock<Item>> for ItemStack {
    fn from(item: &'static std::sync::LazyLock<Item>) -> Self {
        Self::new(item)
    }
}

impl std::fmt::Display for ItemStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "Empty")
        } else {
            write!(f, "{} {}", self.count, self.item.key)
        }
    }
}

impl WriteTo for ItemStack {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        if self.is_empty() {
            VarInt(0).write(writer)?;
        } else {
            VarInt(self.count).write(writer)?;
            // Write item ID as VarInt
            VarInt(self.item.id() as i32).write(writer)?;
            // Write DataComponentPatch
            self.patch.write(writer)?;
        }
        Ok(())
    }
}

impl ReadFrom for ItemStack {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let count = VarInt::read(data)?.0;
        if count <= 0 {
            return Ok(Self::empty());
        }

        let item_id = VarInt::read(data)?.0;
        let item_id = usize::try_from(item_id)
            .map_err(|_| std::io::Error::other(format!("Negative item id: {item_id}")))?;
        let item = REGISTRY
            .items
            .by_id(item_id)
            .ok_or_else(|| std::io::Error::other(format!("Unknown item id: {item_id}")))?;

        // Read DataComponentPatch
        let patch = DataComponentPatch::read(data)?;

        Ok(Self::with_count_and_patch(item, count, patch))
    }
}

impl ItemStack {
    /// Reads an item stack using the delimited (untrusted) component format.
    ///
    /// Vanilla uses this for serverbound packets where component data is
    /// length-prefixed (e.g., `ServerboundSetCreativeModeSlotPacket`).
    pub fn read_untrusted(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let count = VarInt::read(data)?.0;
        if count <= 0 {
            return Ok(Self::empty());
        }

        let item_id = VarInt::read(data)?.0;
        let item_id = usize::try_from(item_id)
            .map_err(|_| std::io::Error::other(format!("Negative item id: {item_id}")))?;
        let item = REGISTRY
            .items
            .by_id(item_id)
            .ok_or_else(|| std::io::Error::other(format!("Unknown item id: {item_id}")))?;
        let patch = DataComponentPatch::read_delimited(data)?;

        let stack = Self::with_count_and_patch(item, count, patch);
        stack.validate_persistent_encoding()?;
        Ok(stack)
    }
}

use simdnbt::{
    FromNbtTag, ToNbtTag,
    borrow::{NbtCompound as NbtCompoundView, NbtTag as BorrowedNbtTag},
    owned::NbtCompound,
};
use steel_utils::nbt::NbtNumeric as _;

impl ToNbtTag for ItemStack {
    /// Converts this item stack to an NBT tag for persistent storage.
    ///
    /// Format (matching vanilla Minecraft):
    /// ```text
    /// {
    ///     id: "minecraft:stone",
    ///     count: 64,
    ///     components: { ... }  // Only present if patch is non-empty
    /// }
    /// ```
    fn to_nbt_tag(self) -> simdnbt::owned::NbtTag {
        self.to_nbt_tag_ref()
    }
}

impl ItemStack {
    /// Checks that this stack can be encoded by Vanilla's persistent
    /// `ItemStack.CODEC` before untrusted network data enters server state.
    ///
    /// This is an ingress check rather than a type invariant: programmatic
    /// component mutation can still create values whose save codec reports
    /// and omits invalid fields.
    pub fn validate_persistent_encoding(&self) -> Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        if !(1..=99).contains(&self.count) {
            return Err(std::io::Error::other(format!(
                "Item stack count {} is outside the persistent range 1..=99",
                self.count
            )));
        }
        self.patch.try_to_nbt_tag_ref().map(|_| ())
    }

    /// Converts this item stack to an NBT tag for persistent storage without consuming it.
    #[must_use]
    pub fn to_nbt_tag_ref(&self) -> simdnbt::owned::NbtTag {
        if self.is_empty() {
            // Empty stacks are represented as an empty compound
            return simdnbt::owned::NbtTag::Compound(NbtCompound::new());
        }

        let mut compound = NbtCompound::new();

        // id: The item identifier
        compound.insert("id", self.item.key.to_string());

        compound.insert("count", self.count);

        // components: The component patch (only if non-empty)
        if !self.patch.is_empty() {
            compound.insert("components", self.patch.to_nbt_tag_ref());
        }

        simdnbt::owned::NbtTag::Compound(compound)
    }
}

impl FromNbtTag for ItemStack {
    /// Parses an item stack from an NBT tag.
    ///
    /// Accepts the vanilla format:
    /// ```text
    /// {
    ///     id: "minecraft:stone",
    ///     count: 64,
    ///     components: { ... }
    /// }
    /// ```
    fn from_nbt_tag(tag: BorrowedNbtTag) -> Option<Self> {
        let compound = tag.compound()?;

        // Get the item ID
        let id_str = compound.get("id")?.string()?.to_str();
        let id = id_str.parse::<Identifier>().ok()?;

        // Look up the item in the registry
        let item = REGISTRY.items.by_key(&id)?;

        let count = decode_persistent_count(compound.get("count"))?;

        let patch = match compound.get("components") {
            Some(tag) => DataComponentPatch::from_nbt_tag(tag)?,
            None => DataComponentPatch::new(),
        };

        Some(Self::with_count_and_patch(item, count, patch))
    }
}

impl ItemStack {
    /// Parses an `ItemStack` from a borrowed `NbtCompoundView`.
    ///
    /// This is useful for loading items from disk where we have borrowed NBT data
    /// and want to avoid the overhead of converting to an owned tag first.
    #[must_use]
    pub fn from_borrowed_compound(compound: &NbtCompoundView<'_, '_>) -> Option<Self> {
        // Get the item ID
        let id_str = compound.string("id")?.to_str();
        let id = id_str.parse::<Identifier>().ok()?;

        // Look up the item in the registry
        let item = REGISTRY.items.by_key(&id)?;

        let count = decode_persistent_count(compound.get("count"))?;

        let patch = match compound.get("components") {
            Some(tag) => DataComponentPatch::from_nbt_tag(tag)?,
            None => DataComponentPatch::new(),
        };

        Some(Self::with_count_and_patch(item, count, patch))
    }
}

fn decode_persistent_count(tag: Option<BorrowedNbtTag<'_, '_>>) -> Option<i32> {
    let count = match tag {
        Some(tag) => tag.codec_i32()?,
        None => 1,
    };
    (1..=99).contains(&count).then_some(count)
}

#[cfg(test)]
mod enchantment_tests {
    use super::ItemStack;
    use crate::{test_support::init_test_registry, vanilla_enchantments, vanilla_items};

    #[test]
    fn stored_book_enchantments_are_not_active_item_enchantments() {
        init_test_registry();
        let mut book = ItemStack::new(&vanilla_items::ENCHANTED_BOOK);
        book.upgrade_enchantment(vanilla_enchantments::SHARPNESS.key.clone(), 3);

        assert_eq!(
            book.get_enchantment_level(&vanilla_enchantments::SHARPNESS.key),
            0
        );
        assert_eq!(
            book.get_enchantments_for_crafting().map(|enchantments| {
                enchantments.get_level(&vanilla_enchantments::SHARPNESS.key)
            }),
            Some(3)
        );
    }
}

#[cfg(test)]
mod name_tests {
    use text_components::TextComponent;

    use super::ItemStack;
    use crate::data_components::components::{Filterable, WrittenBookContent};
    use crate::data_components::vanilla_components::{CUSTOM_NAME, WRITTEN_BOOK_CONTENT};
    use crate::test_support::init_test_registry;
    use crate::vanilla_items;

    fn written_book(raw_title: &str, filtered_title: Option<&str>) -> ItemStack {
        let content = WrittenBookContent::new(
            Filterable::new(raw_title.to_owned(), filtered_title.map(ToOwned::to_owned)),
            "Author".to_owned(),
            0,
            Vec::new(),
            true,
        );
        let Ok(content) = content else {
            panic!("test written-book content should be valid");
        };
        let mut book = ItemStack::new(&vanilla_items::WRITTEN_BOOK);
        book.set(WRITTEN_BOOK_CONTENT, content);
        book
    }

    #[test]
    fn written_book_raw_title_is_its_custom_and_hover_name() {
        init_test_registry();
        let book = written_book("Raw title", Some("Filtered title"));
        let expected = TextComponent::plain("Raw title");

        assert_eq!(book.custom_name().as_deref(), Some(&expected));
        assert_eq!(book.custom_or_component_name().as_ref(), &expected);
    }

    #[test]
    fn explicit_custom_name_takes_precedence_over_written_book_title() {
        init_test_registry();
        let mut book = written_book("Book title", None);
        let explicit = TextComponent::plain("Explicit name");
        book.set(CUSTOM_NAME, explicit.clone());

        assert_eq!(book.custom_name().as_deref(), Some(&explicit));
        assert_eq!(book.custom_or_component_name().as_ref(), &explicit);
    }

    #[test]
    fn written_book_title_uses_java_blank_rules() {
        init_test_registry();
        let blank = written_book("\u{00a0}\u{202f}", None);
        assert!(blank.custom_name().is_none());
        let Some(default_name) = blank.get(crate::data_components::vanilla_components::ITEM_NAME)
        else {
            panic!("written book should have a default item name");
        };
        assert_eq!(blank.custom_or_component_name().as_ref(), default_name);

        let next_line = written_book("\u{0085}", None);
        assert_eq!(
            next_line.custom_name().as_deref(),
            Some(&TextComponent::plain("\u{0085}"))
        );
    }
}

#[cfg(test)]
mod durability_tests {
    use steel_utils::random::xoroshiro::Xoroshiro;

    use super::ItemStack;
    use crate::data_components::vanilla_components::{ENCHANTMENTS, ItemEnchantments};
    use crate::test_support::init_test_registry;
    use crate::{vanilla_enchantments, vanilla_items};

    fn with_unbreaking(item: crate::items::ItemRef, level: u32) -> ItemStack {
        let mut stack = ItemStack::new(item);
        let mut enchantments = ItemEnchantments::empty();
        enchantments.set(vanilla_enchantments::UNBREAKING.key.clone(), level);
        stack.set(ENCHANTMENTS, enchantments);
        stack
    }

    #[test]
    fn item_damage_uses_generated_unbreaking_tool_requirements() {
        init_test_registry();
        let mut armor = with_unbreaking(&vanilla_items::DIAMOND_CHESTPLATE, 3);
        let mut tool = with_unbreaking(&vanilla_items::DIAMOND_PICKAXE, 3);
        let mut armor_random = Xoroshiro::from_seed_unmixed(42);
        let mut tool_random = Xoroshiro::from_seed_unmixed(42);

        assert!(!armor.hurt_and_break_with_random(100, false, &mut armor_random));
        assert!(!tool.hurt_and_break_with_random(100, false, &mut tool_random));
        assert_eq!(armor.get_damage_value(), 68);
        assert_eq!(tool.get_damage_value(), 18);

        let effects = vanilla_enchantments::UNBREAKING.effects.item_damage;
        assert_eq!(
            effects[0].requirements.and_then(|requirements| {
                requirements.matches_item_context(&vanilla_items::DIAMOND_CHESTPLATE)
            }),
            Some(true)
        );
        assert_eq!(
            effects[1].requirements.and_then(|requirements| {
                requirements.matches_item_context(&vanilla_items::DIAMOND_CHESTPLATE)
            }),
            Some(false)
        );
        assert_eq!(
            effects[0].requirements.and_then(|requirements| {
                requirements.matches_item_context(&vanilla_items::DIAMOND_PICKAXE)
            }),
            Some(false)
        );
        assert_eq!(
            effects[1].requirements.and_then(|requirements| {
                requirements.matches_item_context(&vanilla_items::DIAMOND_PICKAXE)
            }),
            Some(true)
        );
    }

    #[test]
    fn match_tool_supports_generated_direct_item_sets() {
        init_test_registry();
        let requirements = vanilla_enchantments::INFINITY.effects.ammo_use[0]
            .requirements
            .expect("Infinity ammo use should have a match_tool requirement");

        assert_eq!(
            requirements.matches_item_context(&vanilla_items::ARROW),
            Some(true)
        );
        assert_eq!(
            requirements.matches_item_context(&vanilla_items::SPECTRAL_ARROW),
            Some(false)
        );
    }
}

#[cfg(test)]
mod persistence_tests {
    use std::io::Cursor;

    use simdnbt::FromNbtTag;
    use simdnbt::borrow::{NbtTag as BorrowedNbtTag, read_tag};
    use simdnbt::owned::{NbtCompound, NbtTag};
    use steel_utils::codec::VarInt;
    use steel_utils::serial::WriteTo;

    use super::ItemStack;
    use crate::data_components::vanilla_components::{
        CUSTOM_DATA, JUKEBOX_PLAYABLE, LORE, MAX_DAMAGE, MAX_STACK_SIZE, TOOLTIP_DISPLAY,
    };
    use crate::data_components::{CustomData, JukeboxPlayable};
    use crate::test_support::init_test_registry;
    use crate::{REGISTRY, RegistryEntry, RegistryExt, vanilla_items, vanilla_jukebox_songs};

    fn with_borrowed_tag<R>(tag: NbtTag, visitor: impl FnOnce(BorrowedNbtTag<'_, '_>) -> R) -> R {
        let mut bytes = Vec::new();
        tag.write(&mut bytes);
        let borrowed =
            read_tag(&mut Cursor::new(bytes.as_slice())).expect("owned test tag should parse");
        visitor(borrowed.as_tag())
    }

    fn parse_stack(compound: NbtCompound) -> Option<ItemStack> {
        with_borrowed_tag(NbtTag::Compound(compound), ItemStack::from_nbt_tag)
    }

    fn stone_stack_nbt() -> NbtCompound {
        let mut compound = NbtCompound::new();
        compound.insert("id", "minecraft:stone");
        compound
    }

    fn untrusted_stack_bytes(
        count: i32,
        component: Option<(&steel_utils::Identifier, Vec<u8>)>,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        VarInt(count)
            .write(&mut bytes)
            .expect("test stack count should encode");
        VarInt(vanilla_items::STONE.id() as i32)
            .write(&mut bytes)
            .expect("test item id should encode");

        if let Some((component, value)) = component {
            VarInt(1)
                .write(&mut bytes)
                .expect("added component count should encode");
            VarInt(0)
                .write(&mut bytes)
                .expect("removed component count should encode");
            let component_id = REGISTRY
                .data_components
                .id_from_key(component)
                .expect("test component should be registered");
            VarInt(component_id as i32)
                .write(&mut bytes)
                .expect("component id should encode");
            VarInt(value.len() as i32)
                .write(&mut bytes)
                .expect("component length should encode");
            bytes.extend_from_slice(&value);
        } else {
            VarInt(0)
                .write(&mut bytes)
                .expect("added component count should encode");
            VarInt(0)
                .write(&mut bytes)
                .expect("removed component count should encode");
        }
        bytes
    }

    #[test]
    fn persistent_item_count_uses_vanilla_integer_codec() {
        init_test_registry();
        let mut compound = stone_stack_nbt();
        compound.insert("count", 5.9_f64);
        assert_eq!(parse_stack(compound).map(|stack| stack.count()), Some(5));

        let mut compound = stone_stack_nbt();
        compound.insert("count", 100);
        assert!(parse_stack(compound).is_none());

        let mut compound = stone_stack_nbt();
        compound.insert("count", "5");
        assert!(parse_stack(compound).is_none());
    }

    #[test]
    fn malformed_present_component_patch_rejects_the_item_stack() {
        init_test_registry();
        let mut components = NbtCompound::new();
        components.insert("minecraft:max_stack_size", 0);
        let mut compound = stone_stack_nbt();
        compound.insert("components", components);

        assert!(parse_stack(compound).is_none());
    }

    #[test]
    fn component_patches_stay_sanitized_against_the_item_prototype() {
        init_test_registry();
        let mut patch = crate::data_components::DataComponentPatch::new();
        patch.set(MAX_STACK_SIZE, 64);
        patch.remove(CUSTOM_DATA);
        let mut stack = ItemStack::with_count_and_patch(&vanilla_items::STONE, 1, patch);
        assert!(stack.components_patch().is_empty());

        stack.set(MAX_STACK_SIZE, 16);
        assert_eq!(stack.components_patch().len(), 1);
        stack.set(MAX_STACK_SIZE, 64);
        assert!(stack.components_patch().is_empty());

        stack.remove(MAX_STACK_SIZE);
        assert!(stack.components_patch().is_removed(&MAX_STACK_SIZE.key));
        stack.set(MAX_STACK_SIZE, 64);
        assert!(stack.components_patch().is_empty());

        stack.remove(CUSTOM_DATA);
        assert!(stack.components_patch().is_empty());

        stack.set(MAX_STACK_SIZE, 16);
        stack.set_item(&vanilla_items::ENDER_PEARL.key);
        assert_eq!(stack.max_stack_size(), 16);
        assert!(stack.components_patch().is_empty());
    }

    #[test]
    fn strict_validation_checks_components_even_when_the_stack_is_empty() {
        init_test_registry();
        let mut patch = crate::data_components::DataComponentPatch::new();
        patch.set(MAX_DAMAGE, 1);
        let stack = ItemStack::with_count_and_patch(&vanilla_items::STONE, 0, patch);

        assert!(stack.is_empty());
        assert!(stack.validate_strict().is_err());
    }

    #[test]
    fn default_count_is_always_present_in_persistent_encoding() {
        init_test_registry();
        let stack = ItemStack::new(&vanilla_items::STONE);
        let NbtTag::Compound(compound) = stack.to_nbt_tag_ref() else {
            panic!("item stack should encode as a compound");
        };

        assert_eq!(compound.get("count"), Some(&NbtTag::Int(1)));
    }

    #[test]
    fn untrusted_stack_rejects_direct_jukebox_holders() {
        init_test_registry();
        let mut component_bytes = Vec::new();
        VarInt(0)
            .write(&mut component_bytes)
            .expect("direct holder discriminator should encode");
        let bytes = untrusted_stack_bytes(1, Some((&JUKEBOX_PLAYABLE.key, component_bytes)));

        assert!(ItemStack::read_untrusted(&mut Cursor::new(bytes.as_slice())).is_err());
    }

    #[test]
    fn untrusted_stack_accepts_persistable_registry_holders() {
        init_test_registry();
        let reference = JukeboxPlayable::new(&vanilla_jukebox_songs::CAT);
        let mut component_bytes = Vec::new();
        reference
            .write(&mut component_bytes)
            .expect("registry holder should have a network representation");
        let bytes = untrusted_stack_bytes(1, Some((&JUKEBOX_PLAYABLE.key, component_bytes)));

        let stack = ItemStack::read_untrusted(&mut Cursor::new(bytes.as_slice()))
            .expect("persistable untrusted stack should decode");
        assert_eq!(stack.get(JUKEBOX_PLAYABLE), Some(&reference));
    }

    #[test]
    fn untrusted_stack_uses_persistent_count_range() {
        init_test_registry();
        let bytes = untrusted_stack_bytes(100, None);

        assert!(ItemStack::read_untrusted(&mut Cursor::new(bytes.as_slice())).is_err());
    }

    #[test]
    fn untrusted_stack_validates_component_persistent_constraints() {
        init_test_registry();
        let mut component_bytes = Vec::new();
        VarInt(0)
            .write(&mut component_bytes)
            .expect("max stack size should encode on the network");
        let bytes = untrusted_stack_bytes(
            1,
            Some((
                &crate::data_components::vanilla_components::MAX_STACK_SIZE.key,
                component_bytes,
            )),
        );

        assert!(ItemStack::read_untrusted(&mut Cursor::new(bytes.as_slice())).is_err());
    }

    #[test]
    fn save_omits_invalid_component_value_but_keeps_present_patch_field() {
        init_test_registry();
        let mut stack = ItemStack::new(&vanilla_items::STONE);
        stack.set(MAX_STACK_SIZE, 0);

        assert!(stack.validate_persistent_encoding().is_err());
        let NbtTag::Compound(compound) = stack.to_nbt_tag_ref() else {
            panic!("item stack should still encode as a compound");
        };
        assert_eq!(
            compound.string("id").map(|value| value.to_str()),
            Some("minecraft:stone".into())
        );
        assert!(
            compound
                .compound("components")
                .is_some_and(simdnbt::owned::NbtCompound::is_empty)
        );
    }

    #[test]
    fn toggle_tooltips_updates_the_typed_display_component() {
        init_test_registry();
        let mut stack = ItemStack::new(&vanilla_items::STONE);

        stack.toggle_tooltips(&[(LORE.key.clone(), false)]);
        let display = stack
            .get(TOOLTIP_DISPLAY)
            .expect("tooltip display should be set");
        assert!(!display.shows(LORE));

        stack.toggle_tooltips(&[(LORE.key.clone(), true)]);
        assert!(
            stack
                .get(TOOLTIP_DISPLAY)
                .expect("tooltip display should remain set")
                .shows(LORE)
        );
    }

    #[test]
    fn set_custom_data_recursively_merges_and_removes_empty_values() {
        init_test_registry();
        let mut stack = ItemStack::new(&vanilla_items::STONE);
        let empty = CustomData::default();
        stack.set_custom_data(&empty);
        assert!(stack.get(CUSTOM_DATA).is_none());

        let mut nested = NbtCompound::new();
        nested.insert("kept", 1);
        nested.insert("changed", 1);
        let mut first = NbtCompound::new();
        first.insert("nested", nested);
        stack.set_custom_data(
            &CustomData::try_from_compound(first).expect("first value should be valid"),
        );

        let mut nested = NbtCompound::new();
        nested.insert("changed", 2);
        let mut second = NbtCompound::new();
        second.insert("nested", nested);
        stack.set_custom_data(
            &CustomData::try_from_compound(second).expect("second value should be valid"),
        );

        let nested = stack
            .get(CUSTOM_DATA)
            .and_then(|data| data.as_compound().compound("nested"))
            .expect("nested custom data should remain");
        assert_eq!(nested.int("kept"), Some(1));
        assert_eq!(nested.int("changed"), Some(2));
    }
}

#[cfg(test)]
mod loot_function_tests {
    use super::ItemStack;
    use crate::data_components::vanilla_components::{
        BLOCK_STATE, CUSTOM_NAME, POTION_CONTENTS, SUSPICIOUS_STEW_EFFECTS, TRIM,
    };
    use crate::loot_table::{EnchantmentOptions, NameTarget};
    use crate::test_support::init_test_registry;
    use crate::{REGISTRY, RegistryExt as _, vanilla_items};
    use steel_utils::Identifier;

    /// `SetItemDamageFunction` sets *durability*, so a fraction of 1.0 must leave the item
    /// undamaged and 0.0 must leave it one hit from breaking.
    #[test]
    fn set_damage_fraction_maps_durability_not_damage() {
        init_test_registry();
        let mut pickaxe = ItemStack::new(&vanilla_items::DIAMOND_PICKAXE);
        let max = pickaxe.get_max_damage();
        assert!(max > 0, "diamond pickaxe should be damageable");

        pickaxe.set_damage_fraction(1.0, false);
        assert_eq!(pickaxe.get_damage_value(), 0);

        pickaxe.set_damage_fraction(0.0, false);
        assert_eq!(pickaxe.get_damage_value(), max);

        // Vanilla is `floor(remaining_durability * max)`, which is not the same as
        // `max - max / 2` for an odd max (1561 -> 780, not 781).
        pickaxe.set_damage_fraction(0.5, false);
        assert_eq!(
            pickaxe.get_damage_value(),
            (0.5 * max as f32).floor() as i32
        );
    }

    /// A non-damageable item must be left alone rather than gaining a DAMAGE component.
    #[test]
    fn set_damage_fraction_ignores_non_damageable_items() {
        init_test_registry();
        let mut stone = ItemStack::new(&vanilla_items::STONE);
        stone.set_damage_fraction(0.5, false);
        assert_eq!(stone.get_damage_value(), 0);
    }

    /// The datapack payload is a JSON text component, not a literal string.
    #[test]
    fn set_name_parses_json_text_component() {
        init_test_registry();
        let mut map = ItemStack::new(&vanilla_items::MAP);
        map.set_name(
            r#"{"translate":"filled_map.buried_treasure"}"#,
            NameTarget::CustomName,
        );
        assert!(map.get(CUSTOM_NAME).is_some());

        // Malformed payloads must not replace the name with garbage.
        let mut other = ItemStack::new(&vanilla_items::MAP);
        other.set_name("not json", NameTarget::CustomName);
        assert!(other.get(CUSTOM_NAME).is_none());
    }

    #[test]
    fn set_potion_preserves_existing_custom_fields() {
        init_test_registry();
        let mut bottle = ItemStack::new(&vanilla_items::POTION);
        bottle.set_potion(&Identifier::vanilla_static("swiftness"));

        let contents = bottle.get(POTION_CONTENTS).expect("potion contents set");
        assert_eq!(
            contents.potion().map(|potion| potion.value().key.clone()),
            REGISTRY
                .potions
                .by_key(&Identifier::vanilla_static("swiftness"))
                .map(|potion| potion.key.clone())
        );
    }

    /// All six vanilla `set_components` uses are `minecraft:trim`; this pins the
    /// SNBT-shaped JSON path that decodes them.
    #[test]
    fn set_components_from_json_decodes_armor_trim() {
        init_test_registry();
        let mut boots = ItemStack::new(&vanilla_items::DIAMOND_BOOTS);
        boots.set_components_from_json(
            r#"{"minecraft:trim":{"material":"minecraft:copper","pattern":"minecraft:bolt"}}"#,
        );
        assert!(
            boots.get(TRIM).is_some(),
            "trim component should decode from the datapack's JSON form"
        );
    }

    /// Unknown or malformed entries must be skipped, not applied partially.
    #[test]
    fn set_components_from_json_skips_unknown_components() {
        init_test_registry();
        let mut boots = ItemStack::new(&vanilla_items::DIAMOND_BOOTS);
        boots.set_components_from_json(r#"{"minecraft:not_a_component":{"a":"b"}}"#);
        assert!(boots.components_patch().is_empty());
    }

    /// Vanilla only applies stew effects to suspicious stew.
    #[test]
    fn set_stew_effects_only_applies_to_suspicious_stew() {
        init_test_registry();
        let effects = [crate::loot_table::StewEffect {
            effect_type: Identifier::vanilla_static("night_vision"),
            duration: crate::loot_table::NumberProvider::Constant(8.0),
        }];

        let mut bowl = ItemStack::new(&vanilla_items::BOWL);
        bowl.set_stew_effects(&effects, &mut rand::rng());
        assert!(bowl.get(SUSPICIOUS_STEW_EFFECTS).is_none());

        let mut stew = ItemStack::new(&vanilla_items::SUSPICIOUS_STEW);
        stew.set_stew_effects(&effects, &mut rand::rng());
        let applied = stew.get(SUSPICIOUS_STEW_EFFECTS).expect("effects set");
        // Non-instantaneous effects convert seconds to ticks.
        assert_eq!(applied.effects()[0].duration(), 8 * 20);
    }

    /// Enchanting a plain book must yield an enchanted book, per `EnchantmentHelper`.
    #[test]
    fn enchant_randomly_promotes_book_to_enchanted_book() {
        init_test_registry();
        const SHARPNESS: &[Identifier] = &[Identifier::vanilla_static("sharpness")];
        let mut book = ItemStack::new(&vanilla_items::BOOK);
        book.enchant_randomly(
            &EnchantmentOptions::List(SHARPNESS),
            true,
            false,
            &mut rand::rng(),
        );
        assert!(book.is(&vanilla_items::ENCHANTED_BOOK));
    }

    /// An empty candidate set must leave the item untouched rather than panicking on an
    /// empty-range sample.
    #[test]
    fn enchant_randomly_with_no_candidates_is_a_no_op() {
        init_test_registry();
        let mut sword = ItemStack::new(&vanilla_items::DIAMOND_SWORD);
        sword.enchant_randomly(
            &EnchantmentOptions::List(&[]),
            true,
            false,
            &mut rand::rng(),
        );
        assert!(sword.components_patch().is_empty());
        assert!(sword.is(&vanilla_items::DIAMOND_SWORD));
    }

    /// `copy_state` writes only the properties the source state actually has.
    #[test]
    fn copy_block_state_without_context_state_is_a_no_op() {
        init_test_registry();
        let mut item = ItemStack::new(&vanilla_items::NOTE_BLOCK);
        let mut rng = rand::rng();
        let ctx = crate::loot_table::LootContext::new(&mut rng);
        item.copy_block_state(&Identifier::vanilla_static("note_block"), &["note"], &ctx);
        assert!(item.get(BLOCK_STATE).is_none());
    }
}

#[cfg(test)]
mod copy_components_tests {
    use super::ItemStack;
    use crate::data_components::DataComponentPatch;
    use crate::data_components::vanilla_components::CUSTOM_NAME;
    use crate::loot_table::{BlockEntityRef, CopySource, LootContext};
    use crate::test_support::init_test_registry;
    use crate::vanilla_items;
    use steel_utils::Identifier;
    use text_components::TextComponent;

    /// 71 vanilla block loot tables use `copy_components` with `source: block_entity`; a
    /// named chest must carry its name onto the dropped item.
    #[test]
    fn copies_listed_components_from_the_block_entity() {
        init_test_registry();
        let mut source = DataComponentPatch::new();
        source.set(CUSTOM_NAME, TextComponent::plain("Storage"));

        let mut rng = rand::rng();
        let ctx = LootContext::new(&mut rng).with_block_entity(BlockEntityRef {
            block_entity_type: None,
            custom_name: None,
            inventory: None,
            components: Some(&source),
        });

        let mut chest = ItemStack::new(&vanilla_items::CHEST);
        chest.copy_components(
            CopySource::BlockEntity,
            &[Identifier::vanilla_static("custom_name")],
            &ctx,
        );
        assert!(chest.get(CUSTOM_NAME).is_some());
    }

    /// Components the table did not list must not leak onto the item.
    #[test]
    fn does_not_copy_unlisted_components() {
        init_test_registry();
        let mut source = DataComponentPatch::new();
        source.set(CUSTOM_NAME, TextComponent::plain("Storage"));

        let mut rng = rand::rng();
        let ctx = LootContext::new(&mut rng).with_block_entity(BlockEntityRef {
            block_entity_type: None,
            custom_name: None,
            inventory: None,
            components: Some(&source),
        });

        let mut chest = ItemStack::new(&vanilla_items::CHEST);
        chest.copy_components(
            CopySource::BlockEntity,
            &[Identifier::vanilla_static("lock")],
            &ctx,
        );
        assert!(chest.get(CUSTOM_NAME).is_none());
    }

    /// Without a block entity in context the function must be inert rather than panicking.
    #[test]
    fn is_a_no_op_without_a_block_entity() {
        init_test_registry();
        let mut rng = rand::rng();
        let ctx = LootContext::new(&mut rng);
        let mut chest = ItemStack::new(&vanilla_items::CHEST);
        chest.copy_components(
            CopySource::BlockEntity,
            &[Identifier::vanilla_static("custom_name")],
            &ctx,
        );
        assert!(chest.components_patch().is_empty());
    }
}
