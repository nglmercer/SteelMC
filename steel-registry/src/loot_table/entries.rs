use steel_utils::random::Random;

use super::{
    ConditionalLootFunction, Identifier, ItemStack, LootCondition, LootContext, LootType,
    NumberProvider, REGISTRY, RegistryExt, TaggedRegistryExt,
};

/// A loot table entry that can generate items.
#[derive(Debug, Clone)]
pub enum LootEntry {
    /// Drop a specific item.
    Item {
        name: Identifier,
        weight: i32,
        quality: i32,
        conditions: &'static [LootCondition],
        functions: &'static [ConditionalLootFunction],
    },
    /// Reference another loot table by name.
    LootTableRef {
        name: Identifier,
        weight: i32,
        quality: i32,
        conditions: &'static [LootCondition],
        functions: &'static [ConditionalLootFunction],
    },
    /// Inline loot table (embedded pools directly in entry).
    InlineLootTable {
        pools: &'static [LootPool],
        weight: i32,
        quality: i32,
        conditions: &'static [LootCondition],
        functions: &'static [ConditionalLootFunction],
    },
    /// Drop items from a tag.
    Tag {
        name: Identifier,
        expand: bool,
        weight: i32,
        quality: i32,
        conditions: &'static [LootCondition],
        functions: &'static [ConditionalLootFunction],
    },
    /// Try children in order, use first that matches.
    Alternatives {
        children: &'static [LootEntry],
        conditions: &'static [LootCondition],
    },
    /// Use all children.
    Group {
        children: &'static [LootEntry],
        conditions: &'static [LootCondition],
    },
    /// Use children in sequence until one fails.
    Sequence {
        children: &'static [LootEntry],
        conditions: &'static [LootCondition],
    },
    /// Empty entry (no drop).
    Empty {
        weight: i32,
        conditions: &'static [LootCondition],
    },
    /// Dynamic content (e.g., block entity contents).
    Dynamic {
        name: Identifier,
        conditions: &'static [LootCondition],
    },
    /// Select items from specific block entity slots.
    Slots {
        /// Slots to select from (can be single slot or range).
        slots: SlotRange,
        conditions: &'static [LootCondition],
        functions: &'static [ConditionalLootFunction],
    },
}

/// A range of slots for the Slots entry type.
#[derive(Debug, Clone, Copy)]
pub enum SlotRange {
    /// A single specific slot index.
    Single(i32),
    /// A range of slots (inclusive).
    Range { min: i32, max: i32 },
    /// All contents slots.
    Contents,
    /// Specific named slots (for entities).
    Named(&'static [&'static str]),
}

impl LootEntry {
    /// Get the weight of this entry for random selection.
    #[must_use]
    pub const fn weight(&self) -> i32 {
        match self {
            Self::Item { weight, .. } => *weight,
            Self::LootTableRef { weight, .. } => *weight,
            Self::InlineLootTable { weight, .. } => *weight,
            Self::Tag { weight, .. } => *weight,
            Self::Empty { weight, .. } => *weight,
            // Composite entries don't have weight
            Self::Alternatives { .. }
            | Self::Group { .. }
            | Self::Sequence { .. }
            | Self::Dynamic { .. }
            | Self::Slots { .. } => 1,
        }
    }

    /// Get the quality modifier for luck-based weight adjustment.
    #[must_use]
    pub const fn quality(&self) -> i32 {
        match self {
            Self::Item { quality, .. } => *quality,
            Self::LootTableRef { quality, .. } => *quality,
            Self::InlineLootTable { quality, .. } => *quality,
            Self::Tag { quality, .. } => *quality,
            Self::Empty { .. }
            | Self::Alternatives { .. }
            | Self::Group { .. }
            | Self::Sequence { .. }
            | Self::Dynamic { .. }
            | Self::Slots { .. } => 0,
        }
    }

    /// Get the effective weight adjusted for luck.
    /// Formula: max(floor(weight + quality * luck), 0)
    #[must_use]
    pub fn effective_weight(&self, luck: f32) -> i32 {
        let base = self.weight() as f32;
        let quality = self.quality() as f32;
        (base + quality * luck).floor().max(0.0) as i32
    }

    /// Get the conditions for this entry.
    #[must_use]
    pub const fn conditions(&self) -> &'static [LootCondition] {
        match self {
            Self::Item { conditions, .. } => conditions,
            Self::LootTableRef { conditions, .. } => conditions,
            Self::InlineLootTable { conditions, .. } => conditions,
            Self::Tag { conditions, .. } => conditions,
            Self::Alternatives { conditions, .. } => conditions,
            Self::Group { conditions, .. } => conditions,
            Self::Sequence { conditions, .. } => conditions,
            Self::Empty { conditions, .. } => conditions,
            Self::Dynamic { conditions, .. } => conditions,
            Self::Slots { conditions, .. } => conditions,
        }
    }

    /// Get the functions for this entry.
    #[must_use]
    pub const fn functions(&self) -> &'static [ConditionalLootFunction] {
        match self {
            Self::Item { functions, .. } => functions,
            Self::LootTableRef { functions, .. } => functions,
            Self::InlineLootTable { functions, .. } => functions,
            Self::Tag { functions, .. } => functions,
            Self::Slots { functions, .. } => functions,
            Self::Empty { .. }
            | Self::Alternatives { .. }
            | Self::Group { .. }
            | Self::Sequence { .. }
            | Self::Dynamic { .. } => &[],
        }
    }
}

/// A pool of loot entries with roll counts.
#[derive(Debug, Clone)]
pub struct LootPool {
    pub rolls: NumberProvider,
    pub bonus_rolls: f32,
    pub entries: &'static [LootEntry],
    pub conditions: &'static [LootCondition],
    pub functions: &'static [ConditionalLootFunction],
}

/// A complete loot table definition.
#[derive(Debug)]
pub struct LootTable {
    pub key: Identifier,
    pub loot_type: LootType,
    pub pools: &'static [LootPool],
    pub functions: &'static [ConditionalLootFunction],
    pub random_sequence: Option<Identifier>,
}

impl LootTable {
    /// Generate random items from this loot table.
    ///
    /// # Arguments
    /// * `ctx` - The loot context containing RNG, luck, block state, tool, etc.
    ///
    /// This follows vanilla's approach:
    /// 1. For each pool, check conditions
    /// 2. Roll `rolls + floor(bonus_rolls * luck)` times
    /// 3. Each roll does weighted random selection among valid entries
    /// 4. Apply entry-level functions to each item
    /// 5. Apply pool-level functions to all items from that pool
    /// 6. Apply table-level functions to all items from the table
    pub fn get_random_items<R: Random>(&self, ctx: &mut LootContext<'_, R>) -> Vec<ItemStack> {
        let mut result = Vec::new();
        for pool in self.pools {
            pool.add_random_items(ctx, &mut result);
        }

        // Apply table-level functions to all items
        if !self.functions.is_empty() {
            for item in &mut result {
                for cond_func in self.functions {
                    if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                        cond_func.function.apply(item, ctx);
                    }
                }
            }
            // Remove items with zero count after applying functions
            result.retain(|item| item.count > 0);
        }

        result
    }

    /// Fills a container's empty slots with loot from this table.
    ///
    /// Vanilla `LootTable.fill`: rolls [`get_random_items`](Self::get_random_items),
    /// then distributes the stacks over the empty slots of `items`, splitting
    /// multi-count stacks and shuffling exactly like vanilla so a seeded context
    /// reproduces vanilla container contents slot for slot.
    pub fn fill<R: Random>(&self, items: &mut [ItemStack], ctx: &mut LootContext<'_, R>) {
        let mut result = self.get_random_items(ctx);
        let mut available_slots = Self::available_slots(items, ctx.rng);
        Self::shuffle_and_split_items(&mut result, available_slots.len(), ctx.rng);

        for stack in result {
            let Some(slot) = available_slots.pop() else {
                log::warn!("Tried to over-fill a container");
                return;
            };
            items[slot] = stack;
        }
    }

    /// Vanilla `LootTable.getAvailableSlots`: indices of the empty slots, shuffled.
    fn available_slots<R: Random>(items: &[ItemStack], rng: &mut R) -> Vec<usize> {
        let mut slots: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_empty())
            .map(|(index, _)| index)
            .collect();
        Self::shuffle(&mut slots, rng);
        slots
    }

    /// Vanilla `LootTable.shuffleAndSplitItems`: moves multi-count stacks aside and
    /// splits them while free slots remain, so loot spreads over more slots; the
    /// leftover splittable stacks are appended and the final list is shuffled.
    fn shuffle_and_split_items<R: Random>(
        result: &mut Vec<ItemStack>,
        available_slots: usize,
        rng: &mut R,
    ) {
        let mut splittable: Vec<ItemStack> = Vec::new();
        let previous = std::mem::take(result);
        for stack in previous {
            if stack.is_empty() {
                continue;
            }
            if stack.count > 1 {
                splittable.push(stack);
            } else {
                result.push(stack);
            }
        }

        while available_slots as i32 - result.len() as i32 - splittable.len() as i32 > 0
            && !splittable.is_empty()
        {
            // Vanilla `splittableItems.remove(index)` shifts the remaining items;
            // keep the same list order so later index draws pick the same stack.
            let index = rng.next_i32_bounded(splittable.len() as i32) as usize;
            let mut stack = splittable.remove(index);
            let split_amount = rng.next_i32_between(1, stack.count / 2);
            let split = stack.split(split_amount);

            if stack.count > 1 && rng.next_bool() {
                splittable.push(stack);
            } else {
                result.push(stack);
            }

            if split.count > 1 && rng.next_bool() {
                splittable.push(split);
            } else {
                result.push(split);
            }
        }

        result.extend(splittable);
        Self::shuffle(result, rng);
    }

    /// Vanilla `Util.shuffle`: Fisher-Yates from the back, drawing `nextInt(i)`
    /// per step.
    fn shuffle<R: Random, T>(list: &mut [T], rng: &mut R) {
        let size = list.len();
        for i in (2..=size).rev() {
            let swap_to = rng.next_i32_bounded(i as i32) as usize;
            list.swap(i - 1, swap_to);
        }
    }
}

impl LootPool {
    /// Add random items from this pool to the result.
    fn add_random_items<R: Random>(
        &self,
        ctx: &mut LootContext<'_, R>,
        result: &mut Vec<ItemStack>,
    ) {
        // Check pool conditions
        for condition in self.conditions {
            if !condition.test(ctx) {
                return;
            }
        }

        // Track where items from this pool start
        let start_index = result.len();

        // Calculate number of rolls
        let roll_count = self.rolls.get_int(ctx.rng) + (self.bonus_rolls * ctx.luck).floor() as i32;

        for _ in 0..roll_count {
            self.add_random_item(ctx, result);
        }

        // Apply pool-level functions to all items generated by this pool
        if !self.functions.is_empty() {
            for item in result.iter_mut().skip(start_index) {
                for cond_func in self.functions {
                    if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                        cond_func.function.apply(item, ctx);
                    }
                }
            }
            // Remove items with zero count after applying functions
            result.retain(|item| item.count > 0);
        }
    }

    /// Select and add a single random item from this pool.
    fn add_random_item<R: Random>(
        &self,
        ctx: &mut LootContext<'_, R>,
        result: &mut Vec<ItemStack>,
    ) {
        // Collect valid entries with their effective weights
        let mut valid_entries: Vec<(&LootEntry, i32)> = Vec::new();
        let mut total_weight = 0;

        for entry in self.entries {
            // Check entry conditions
            let passes_conditions = entry.conditions().iter().all(|c| c.test(ctx));

            if !passes_conditions {
                continue;
            }

            let weight = entry.effective_weight(ctx.luck);
            if weight > 0 {
                valid_entries.push((entry, weight));
                total_weight += weight;
            }
        }

        if total_weight == 0 || valid_entries.is_empty() {
            return;
        }

        // Weighted random selection
        let selected = if valid_entries.len() == 1 {
            valid_entries[0].0
        } else {
            let mut index = ctx.rng.next_i32_bounded(total_weight);
            let mut selected_entry = valid_entries[0].0;
            for (entry, weight) in &valid_entries {
                index -= weight;
                if index < 0 {
                    selected_entry = entry;
                    break;
                }
            }
            selected_entry
        };

        // Generate item(s) from the selected entry
        selected.create_items(ctx, result);
    }
}

impl LootEntry {
    /// Create items from this entry and add them to the result.
    fn create_items<R: Random>(
        &self,
        ctx: &mut LootContext<'_, R>,
        result: &mut Vec<ItemStack>,
    ) {
        match self {
            LootEntry::Item {
                name, functions, ..
            } => {
                if let Some(item_ref) = REGISTRY.items.by_key(name) {
                    let mut item = ItemStack::new(item_ref);

                    // Apply functions
                    for cond_func in *functions {
                        if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                            cond_func.function.apply(&mut item, ctx);
                        }
                    }

                    if item.count > 0 {
                        result.push(item);
                    }
                }
            }
            LootEntry::LootTableRef {
                name, functions, ..
            } => {
                // Recursively get items from referenced loot table
                if let Some(table) = REGISTRY.loot_tables.by_key(name) {
                    let mut items = table.get_random_items(ctx);
                    // Apply functions to all items from the referenced table
                    for item in &mut items {
                        for cond_func in *functions {
                            if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                                cond_func.function.apply(item, ctx);
                            }
                        }
                    }
                    result.extend(items.into_iter().filter(|i| i.count > 0));
                }
            }
            LootEntry::InlineLootTable {
                pools, functions, ..
            } => {
                // Process inline loot table pools directly
                let mut items = Vec::new();
                for pool in *pools {
                    pool.add_random_items(ctx, &mut items);
                }
                // Apply functions to all items from the inline table
                for item in &mut items {
                    for cond_func in *functions {
                        if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                            cond_func.function.apply(item, ctx);
                        }
                    }
                }
                result.extend(items.into_iter().filter(|i| i.count > 0));
            }
            LootEntry::Tag {
                name,
                expand,
                functions,
                ..
            } => {
                // Get all items in the tag
                if let Some(items) = REGISTRY.items.get_tag(name) {
                    if *expand {
                        // Pick one random item from the tag (weighted equally)
                        if !items.is_empty() {
                            let index =
                                ctx.rng.next_i32_bounded(items.len() as i32) as usize;
                            let mut item = ItemStack::new(items[index]);
                            for cond_func in *functions {
                                if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                                    cond_func.function.apply(&mut item, ctx);
                                }
                            }
                            if item.count > 0 {
                                result.push(item);
                            }
                        }
                    } else {
                        // Drop all items from the tag
                        for item_ref in items {
                            let mut item = ItemStack::new(item_ref);
                            for cond_func in *functions {
                                if cond_func.conditions.iter().all(|c| c.test(ctx)) {
                                    cond_func.function.apply(&mut item, ctx);
                                }
                            }
                            if item.count > 0 {
                                result.push(item);
                            }
                        }
                    }
                }
            }
            LootEntry::Alternatives { children, .. } => {
                // Try children in order, use first that passes conditions and produces items
                for child in *children {
                    // Check child's conditions first
                    let passes_conditions = child.conditions().iter().all(|c| c.test(ctx));
                    if !passes_conditions {
                        continue; // Try next alternative
                    }

                    let before_len = result.len();
                    child.create_items(ctx, result);
                    if result.len() > before_len {
                        break; // First successful child that produced items, stop
                    }
                }
            }
            LootEntry::Group { children, .. } => {
                // Use all children that pass their conditions
                for child in *children {
                    let passes_conditions = child.conditions().iter().all(|c| c.test(ctx));
                    if passes_conditions {
                        child.create_items(ctx, result);
                    }
                }
            }
            LootEntry::Sequence { children, .. } => {
                // Use children in sequence until one fails its conditions
                // Note: Unlike Alternatives, Sequence stops when conditions FAIL,
                // not when items are produced. A child can produce nothing but still "succeed".
                for child in *children {
                    let passes_conditions = child.conditions().iter().all(|c| c.test(ctx));
                    if !passes_conditions {
                        break; // Condition failed, stop sequence
                    }
                    child.create_items(ctx, result);
                }
            }
            LootEntry::Empty { .. } => {
                // Empty entry produces nothing
            }
            // Deliberately unimplemented here. `minecraft:dynamic` names a drop the *block*
            // supplies, not something this crate can resolve: the only vanilla user is
            // `blocks/decorated_pot.json` (`minecraft:sherds`), and shulker boxes take the same
            // shape. Both are handled by a `get_drops` override on the block behavior in
            // steel-core, which has the block entity in hand. A generic implementation would
            // need a block-entity handle in `LootContext` for no additional coverage.
            LootEntry::Dynamic { name, .. } => {
                let _ = name;
            }
            // Deliberately unimplemented: selects items from specific block-entity slots, but no
            // vanilla loot table emits this variant. Reachable only from datapacks/plugins.
            LootEntry::Slots {
                slots, functions, ..
            } => {
                let _ = (slots, functions);
            }
        }
    }
}
