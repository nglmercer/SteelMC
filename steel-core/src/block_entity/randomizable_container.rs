//! Vanilla `RandomizableContainer` support for structure loot.
//!
//! Chests, barrels, and shulker boxes placed by structures carry a pending
//! `LootTable`/`LootTableSeed` pair. Vanilla resolves the pair into concrete
//! items on the first container access
//! (`RandomizableContainer.unpackLootTable`), so an unopened chest keeps its
//! table reference across save/load and always rolls the same contents for a
//! given seed.

use std::str::FromStr;

use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::LootContext;
use steel_registry::{REGISTRY, RegistryExt as _, vanilla_attributes};
use steel_utils::random::{RandomSource, legacy_random::LegacyRandom};
use steel_utils::{BlockPos, Identifier};

use crate::entity::{LivingEntity as _, entity_loot_ref};
use crate::player::Player;

/// Pending structure loot for a randomizable container.
///
/// Mirrors the `lootTable`/`lootTableSeed` fields of vanilla's
/// `RandomizableContainerBlockEntity`.
#[derive(Debug, Clone, Default)]
pub struct RandomizableContainerState {
    loot_table: Option<Identifier>,
    loot_table_seed: i64,
}

impl RandomizableContainerState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a loot table still waits to be resolved.
    #[must_use]
    pub fn has_pending_loot(&self) -> bool {
        self.loot_table.is_some()
    }

    /// Vanilla `RandomizableContainer.setLootTable(lootTable, seed)`.
    pub fn set_loot_table(&mut self, loot_table: Identifier, seed: i64) {
        self.loot_table = Some(loot_table);
        self.loot_table_seed = seed;
    }

    /// The pending loot table key, if any.
    #[must_use]
    pub const fn loot_table(&self) -> Option<&Identifier> {
        self.loot_table.as_ref()
    }

    /// The pending loot table seed.
    #[must_use]
    pub const fn loot_table_seed(&self) -> i64 {
        self.loot_table_seed
    }

    /// Vanilla `RandomizableContainer.tryLoadLootTable`.
    ///
    /// Returns `true` when a pending table was loaded; the caller must then
    /// skip reading `Items`, matching vanilla.
    pub fn try_load_loot_table(&mut self, nbt: &NbtCompoundView<'_, '_>) -> bool {
        self.loot_table = None;
        self.loot_table_seed = 0;
        self.loot_table = nbt
            .string("LootTable")
            .and_then(|value| Identifier::from_str(&value.to_string()).ok());
        self.loot_table_seed = nbt.long("LootTableSeed").unwrap_or(0);
        self.loot_table.is_some()
    }

    /// Vanilla `RandomizableContainer.trySaveLootTable`.
    ///
    /// Returns `true` when the pending table was written; the caller must then
    /// skip writing `Items`, matching vanilla.
    pub fn try_save_loot_table(&self, nbt: &mut NbtCompound) -> bool {
        let Some(loot_table) = self.loot_table.as_ref() else {
            return false;
        };

        nbt.insert("LootTable", loot_table.to_string());
        if self.loot_table_seed != 0 {
            nbt.insert("LootTableSeed", self.loot_table_seed);
        }

        true
    }

    /// Vanilla `RandomizableContainer.unpackLootTable`.
    ///
    /// Resolves the pending loot table into the empty slots of `items` and
    /// clears the reference. `player` is the container opener when there is
    /// one; vanilla applies the opener's luck and passes it as `THIS_ENTITY`.
    pub fn unpack_loot_table(
        &mut self,
        items: &mut [ItemStack],
        pos: BlockPos,
        player: Option<&Player>,
    ) {
        let Some(key) = self.loot_table.take() else {
            return;
        };
        // TODO: vanilla triggers the `GENERATE_LOOT` advancement criterion here
        // for server players; advancements are not implemented yet.
        let Some(table) = REGISTRY.loot_tables.by_key(&key) else {
            log::warn!("container at {pos:?} references unknown loot table {key}");
            return;
        };

        // Vanilla `LootContext.withOptionalRandomSeed`: a non-zero seed creates a
        // legacy LCG, seed 0 falls back to the level random.
        let seed = self.loot_table_seed;
        let mut rng = if seed != 0 {
            RandomSource::Legacy(LegacyRandom::from_seed(seed as u64))
        } else {
            RandomSource::create_thread_safe()
        };

        let mut context = LootContext::new(&mut rng).with_origin(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );
        if let Some(player) = player {
            let luck = player
                .attributes()
                .lock()
                .get_value(vanilla_attributes::LUCK)
                .unwrap_or(0.0);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "luck is bounded to [-1024, 1024]"
            )]
            let luck = luck as f32;
            context = context
                .with_luck(luck)
                .with_this_entity(entity_loot_ref(player));
        }

        table.fill(items, &mut context);
    }
}
