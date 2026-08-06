//! Vanilla player statistics.
//!
//! Mirrors `net.minecraft.stats.Stats` / `ServerStatsCounter`. Statistics are per-player and
//! cross-dimension, so they persist alongside `GlobalPlayerData` rather than the per-domain
//! snapshot.
//!
//! Vanilla hardcodes the custom-stat list in `Stats.java` rather than shipping it as datapack
//! data, so the list below is transcribed from that file in declaration order — the order that
//! defines each entry's registry id on the wire. Move it to extracted data if SteelExtractor
//! starts emitting the stat registries.

use rustc_hash::FxHashMap;
use steel_utils::Identifier;

/// A `minecraft:custom` statistic key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomStat(&'static str);

impl CustomStat {
    /// Returns the vanilla identifier path (e.g. `jump`).
    #[must_use]
    pub const fn path(self) -> &'static str {
        self.0
    }

    /// Returns the full vanilla identifier.
    #[must_use]
    pub fn key(self) -> Identifier {
        Identifier::vanilla_static(self.0)
    }

    /// Registry id, which vanilla derives from registration order in `Stats.java`.
    #[must_use]
    pub fn id(self) -> Option<usize> {
        Self::ALL.iter().position(|stat| *stat == self)
    }

    /// Looks a stat up by its vanilla path.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|stat| stat.0 == path)
    }

    /// `minecraft:leave_game`
    pub const LEAVE_GAME: CustomStat = CustomStat("leave_game");
    /// `minecraft:play_time`
    pub const PLAY_TIME: CustomStat = CustomStat("play_time");
    /// `minecraft:total_world_time`
    pub const TOTAL_WORLD_TIME: CustomStat = CustomStat("total_world_time");
    /// `minecraft:time_since_death`
    pub const TIME_SINCE_DEATH: CustomStat = CustomStat("time_since_death");
    /// `minecraft:time_since_rest`
    pub const TIME_SINCE_REST: CustomStat = CustomStat("time_since_rest");
    /// `minecraft:sneak_time`
    pub const SNEAK_TIME: CustomStat = CustomStat("sneak_time");
    /// `minecraft:walk_one_cm`
    pub const WALK_ONE_CM: CustomStat = CustomStat("walk_one_cm");
    /// `minecraft:crouch_one_cm`
    pub const CROUCH_ONE_CM: CustomStat = CustomStat("crouch_one_cm");
    /// `minecraft:sprint_one_cm`
    pub const SPRINT_ONE_CM: CustomStat = CustomStat("sprint_one_cm");
    /// `minecraft:walk_on_water_one_cm`
    pub const WALK_ON_WATER_ONE_CM: CustomStat = CustomStat("walk_on_water_one_cm");
    /// `minecraft:fall_one_cm`
    pub const FALL_ONE_CM: CustomStat = CustomStat("fall_one_cm");
    /// `minecraft:climb_one_cm`
    pub const CLIMB_ONE_CM: CustomStat = CustomStat("climb_one_cm");
    /// `minecraft:fly_one_cm`
    pub const FLY_ONE_CM: CustomStat = CustomStat("fly_one_cm");
    /// `minecraft:walk_under_water_one_cm`
    pub const WALK_UNDER_WATER_ONE_CM: CustomStat = CustomStat("walk_under_water_one_cm");
    /// `minecraft:minecart_one_cm`
    pub const MINECART_ONE_CM: CustomStat = CustomStat("minecart_one_cm");
    /// `minecraft:boat_one_cm`
    pub const BOAT_ONE_CM: CustomStat = CustomStat("boat_one_cm");
    /// `minecraft:pig_one_cm`
    pub const PIG_ONE_CM: CustomStat = CustomStat("pig_one_cm");
    /// `minecraft:happy_ghast_one_cm`
    pub const HAPPY_GHAST_ONE_CM: CustomStat = CustomStat("happy_ghast_one_cm");
    /// `minecraft:horse_one_cm`
    pub const HORSE_ONE_CM: CustomStat = CustomStat("horse_one_cm");
    /// `minecraft:aviate_one_cm`
    pub const AVIATE_ONE_CM: CustomStat = CustomStat("aviate_one_cm");
    /// `minecraft:swim_one_cm`
    pub const SWIM_ONE_CM: CustomStat = CustomStat("swim_one_cm");
    /// `minecraft:strider_one_cm`
    pub const STRIDER_ONE_CM: CustomStat = CustomStat("strider_one_cm");
    /// `minecraft:nautilus_one_cm`
    pub const NAUTILUS_ONE_CM: CustomStat = CustomStat("nautilus_one_cm");
    /// `minecraft:jump`
    pub const JUMP: CustomStat = CustomStat("jump");
    /// `minecraft:drop`
    pub const DROP: CustomStat = CustomStat("drop");
    /// `minecraft:damage_dealt`
    pub const DAMAGE_DEALT: CustomStat = CustomStat("damage_dealt");
    /// `minecraft:damage_dealt_absorbed`
    pub const DAMAGE_DEALT_ABSORBED: CustomStat = CustomStat("damage_dealt_absorbed");
    /// `minecraft:damage_dealt_resisted`
    pub const DAMAGE_DEALT_RESISTED: CustomStat = CustomStat("damage_dealt_resisted");
    /// `minecraft:damage_taken`
    pub const DAMAGE_TAKEN: CustomStat = CustomStat("damage_taken");
    /// `minecraft:damage_blocked_by_shield`
    pub const DAMAGE_BLOCKED_BY_SHIELD: CustomStat = CustomStat("damage_blocked_by_shield");
    /// `minecraft:damage_absorbed`
    pub const DAMAGE_ABSORBED: CustomStat = CustomStat("damage_absorbed");
    /// `minecraft:damage_resisted`
    pub const DAMAGE_RESISTED: CustomStat = CustomStat("damage_resisted");
    /// `minecraft:deaths`
    pub const DEATHS: CustomStat = CustomStat("deaths");
    /// `minecraft:mob_kills`
    pub const MOB_KILLS: CustomStat = CustomStat("mob_kills");
    /// `minecraft:animals_bred`
    pub const ANIMALS_BRED: CustomStat = CustomStat("animals_bred");
    /// `minecraft:player_kills`
    pub const PLAYER_KILLS: CustomStat = CustomStat("player_kills");
    /// `minecraft:fish_caught`
    pub const FISH_CAUGHT: CustomStat = CustomStat("fish_caught");
    /// `minecraft:talked_to_villager`
    pub const TALKED_TO_VILLAGER: CustomStat = CustomStat("talked_to_villager");
    /// `minecraft:traded_with_villager`
    pub const TRADED_WITH_VILLAGER: CustomStat = CustomStat("traded_with_villager");
    /// `minecraft:eat_cake_slice`
    pub const EAT_CAKE_SLICE: CustomStat = CustomStat("eat_cake_slice");
    /// `minecraft:fill_cauldron`
    pub const FILL_CAULDRON: CustomStat = CustomStat("fill_cauldron");
    /// `minecraft:use_cauldron`
    pub const USE_CAULDRON: CustomStat = CustomStat("use_cauldron");
    /// `minecraft:clean_armor`
    pub const CLEAN_ARMOR: CustomStat = CustomStat("clean_armor");
    /// `minecraft:clean_banner`
    pub const CLEAN_BANNER: CustomStat = CustomStat("clean_banner");
    /// `minecraft:clean_shulker_box`
    pub const CLEAN_SHULKER_BOX: CustomStat = CustomStat("clean_shulker_box");
    /// `minecraft:interact_with_brewingstand`
    pub const INTERACT_WITH_BREWINGSTAND: CustomStat = CustomStat("interact_with_brewingstand");
    /// `minecraft:interact_with_beacon`
    pub const INTERACT_WITH_BEACON: CustomStat = CustomStat("interact_with_beacon");
    /// `minecraft:inspect_dropper`
    pub const INSPECT_DROPPER: CustomStat = CustomStat("inspect_dropper");
    /// `minecraft:inspect_hopper`
    pub const INSPECT_HOPPER: CustomStat = CustomStat("inspect_hopper");
    /// `minecraft:inspect_dispenser`
    pub const INSPECT_DISPENSER: CustomStat = CustomStat("inspect_dispenser");
    /// `minecraft:play_noteblock`
    pub const PLAY_NOTEBLOCK: CustomStat = CustomStat("play_noteblock");
    /// `minecraft:tune_noteblock`
    pub const TUNE_NOTEBLOCK: CustomStat = CustomStat("tune_noteblock");
    /// `minecraft:pot_flower`
    pub const POT_FLOWER: CustomStat = CustomStat("pot_flower");
    /// `minecraft:trigger_trapped_chest`
    pub const TRIGGER_TRAPPED_CHEST: CustomStat = CustomStat("trigger_trapped_chest");
    /// `minecraft:open_enderchest`
    pub const OPEN_ENDERCHEST: CustomStat = CustomStat("open_enderchest");
    /// `minecraft:enchant_item`
    pub const ENCHANT_ITEM: CustomStat = CustomStat("enchant_item");
    /// `minecraft:play_record`
    pub const PLAY_RECORD: CustomStat = CustomStat("play_record");
    /// `minecraft:interact_with_furnace`
    pub const INTERACT_WITH_FURNACE: CustomStat = CustomStat("interact_with_furnace");
    /// `minecraft:interact_with_crafting_table`
    pub const INTERACT_WITH_CRAFTING_TABLE: CustomStat = CustomStat("interact_with_crafting_table");
    /// `minecraft:open_chest`
    pub const OPEN_CHEST: CustomStat = CustomStat("open_chest");
    /// `minecraft:sleep_in_bed`
    pub const SLEEP_IN_BED: CustomStat = CustomStat("sleep_in_bed");
    /// `minecraft:open_shulker_box`
    pub const OPEN_SHULKER_BOX: CustomStat = CustomStat("open_shulker_box");
    /// `minecraft:open_barrel`
    pub const OPEN_BARREL: CustomStat = CustomStat("open_barrel");
    /// `minecraft:interact_with_blast_furnace`
    pub const INTERACT_WITH_BLAST_FURNACE: CustomStat = CustomStat("interact_with_blast_furnace");
    /// `minecraft:interact_with_smoker`
    pub const INTERACT_WITH_SMOKER: CustomStat = CustomStat("interact_with_smoker");
    /// `minecraft:interact_with_lectern`
    pub const INTERACT_WITH_LECTERN: CustomStat = CustomStat("interact_with_lectern");
    /// `minecraft:interact_with_campfire`
    pub const INTERACT_WITH_CAMPFIRE: CustomStat = CustomStat("interact_with_campfire");
    /// `minecraft:interact_with_cartography_table`
    pub const INTERACT_WITH_CARTOGRAPHY_TABLE: CustomStat =
        CustomStat("interact_with_cartography_table");
    /// `minecraft:interact_with_loom`
    pub const INTERACT_WITH_LOOM: CustomStat = CustomStat("interact_with_loom");
    /// `minecraft:interact_with_stonecutter`
    pub const INTERACT_WITH_STONECUTTER: CustomStat = CustomStat("interact_with_stonecutter");
    /// `minecraft:bell_ring`
    pub const BELL_RING: CustomStat = CustomStat("bell_ring");
    /// `minecraft:raid_trigger`
    pub const RAID_TRIGGER: CustomStat = CustomStat("raid_trigger");
    /// `minecraft:raid_win`
    pub const RAID_WIN: CustomStat = CustomStat("raid_win");
    /// `minecraft:interact_with_anvil`
    pub const INTERACT_WITH_ANVIL: CustomStat = CustomStat("interact_with_anvil");
    /// `minecraft:interact_with_grindstone`
    pub const INTERACT_WITH_GRINDSTONE: CustomStat = CustomStat("interact_with_grindstone");
    /// `minecraft:target_hit`
    pub const TARGET_HIT: CustomStat = CustomStat("target_hit");
    /// `minecraft:interact_with_smithing_table`
    pub const INTERACT_WITH_SMITHING_TABLE: CustomStat = CustomStat("interact_with_smithing_table");

    /// Every vanilla custom stat, in registration order.
    pub const ALL: [Self; 77] = [
        Self::LEAVE_GAME,
        Self::PLAY_TIME,
        Self::TOTAL_WORLD_TIME,
        Self::TIME_SINCE_DEATH,
        Self::TIME_SINCE_REST,
        Self::SNEAK_TIME,
        Self::WALK_ONE_CM,
        Self::CROUCH_ONE_CM,
        Self::SPRINT_ONE_CM,
        Self::WALK_ON_WATER_ONE_CM,
        Self::FALL_ONE_CM,
        Self::CLIMB_ONE_CM,
        Self::FLY_ONE_CM,
        Self::WALK_UNDER_WATER_ONE_CM,
        Self::MINECART_ONE_CM,
        Self::BOAT_ONE_CM,
        Self::PIG_ONE_CM,
        Self::HAPPY_GHAST_ONE_CM,
        Self::HORSE_ONE_CM,
        Self::AVIATE_ONE_CM,
        Self::SWIM_ONE_CM,
        Self::STRIDER_ONE_CM,
        Self::NAUTILUS_ONE_CM,
        Self::JUMP,
        Self::DROP,
        Self::DAMAGE_DEALT,
        Self::DAMAGE_DEALT_ABSORBED,
        Self::DAMAGE_DEALT_RESISTED,
        Self::DAMAGE_TAKEN,
        Self::DAMAGE_BLOCKED_BY_SHIELD,
        Self::DAMAGE_ABSORBED,
        Self::DAMAGE_RESISTED,
        Self::DEATHS,
        Self::MOB_KILLS,
        Self::ANIMALS_BRED,
        Self::PLAYER_KILLS,
        Self::FISH_CAUGHT,
        Self::TALKED_TO_VILLAGER,
        Self::TRADED_WITH_VILLAGER,
        Self::EAT_CAKE_SLICE,
        Self::FILL_CAULDRON,
        Self::USE_CAULDRON,
        Self::CLEAN_ARMOR,
        Self::CLEAN_BANNER,
        Self::CLEAN_SHULKER_BOX,
        Self::INTERACT_WITH_BREWINGSTAND,
        Self::INTERACT_WITH_BEACON,
        Self::INSPECT_DROPPER,
        Self::INSPECT_HOPPER,
        Self::INSPECT_DISPENSER,
        Self::PLAY_NOTEBLOCK,
        Self::TUNE_NOTEBLOCK,
        Self::POT_FLOWER,
        Self::TRIGGER_TRAPPED_CHEST,
        Self::OPEN_ENDERCHEST,
        Self::ENCHANT_ITEM,
        Self::PLAY_RECORD,
        Self::INTERACT_WITH_FURNACE,
        Self::INTERACT_WITH_CRAFTING_TABLE,
        Self::OPEN_CHEST,
        Self::SLEEP_IN_BED,
        Self::OPEN_SHULKER_BOX,
        Self::OPEN_BARREL,
        Self::INTERACT_WITH_BLAST_FURNACE,
        Self::INTERACT_WITH_SMOKER,
        Self::INTERACT_WITH_LECTERN,
        Self::INTERACT_WITH_CAMPFIRE,
        Self::INTERACT_WITH_CARTOGRAPHY_TABLE,
        Self::INTERACT_WITH_LOOM,
        Self::INTERACT_WITH_STONECUTTER,
        Self::BELL_RING,
        Self::RAID_TRIGGER,
        Self::RAID_WIN,
        Self::INTERACT_WITH_ANVIL,
        Self::INTERACT_WITH_GRINDSTONE,
        Self::TARGET_HIT,
        Self::INTERACT_WITH_SMITHING_TABLE,
    ];
}

/// A statistic type: vanilla's nine `StatType` registry entries.
///
/// The discriminants are registry ids, taken from declaration order in `Stats.java`, and are
/// what a `ClientboundAwardStatsPacket` puts on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StatType {
    /// `minecraft:mined`, keyed by block.
    BlockMined = 0,
    /// `minecraft:crafted`, keyed by item.
    ItemCrafted = 1,
    /// `minecraft:used`, keyed by item.
    ItemUsed = 2,
    /// `minecraft:broken`, keyed by item.
    ItemBroken = 3,
    /// `minecraft:picked_up`, keyed by item.
    ItemPickedUp = 4,
    /// `minecraft:dropped`, keyed by item.
    ItemDropped = 5,
    /// `minecraft:killed`, keyed by entity type.
    EntityKilled = 6,
    /// `minecraft:killed_by`, keyed by entity type.
    EntityKilledBy = 7,
    /// `minecraft:custom`, keyed by a [`CustomStat`].
    Custom = 8,
}

impl StatType {
    /// Vanilla registry id, used as the wire discriminant.
    #[must_use]
    pub const fn id(self) -> usize {
        self as usize
    }

    /// Vanilla registry key path (e.g. `mined`).
    #[must_use]
    pub const fn path(self) -> &'static str {
        match self {
            Self::BlockMined => "mined",
            Self::ItemCrafted => "crafted",
            Self::ItemUsed => "used",
            Self::ItemBroken => "broken",
            Self::ItemPickedUp => "picked_up",
            Self::ItemDropped => "dropped",
            Self::EntityKilled => "killed",
            Self::EntityKilledBy => "killed_by",
            Self::Custom => "custom",
        }
    }
}

/// One statistic: a type plus the registry entry (or custom stat) it is keyed by.
///
/// Registry-keyed families store the entry's identifier rather than a typed ref so a single
/// counter can hold blocks, items, and entity types together, exactly as vanilla's
/// `Object2IntMap<Stat<?>>` does.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stat {
    /// A `minecraft:custom` statistic.
    Custom(CustomStat),
    /// A registry-keyed statistic, e.g. `mined` of `minecraft:stone`.
    Keyed {
        /// Which family this belongs to.
        stat_type: StatType,
        /// The registry entry it counts.
        key: Identifier,
    },
}

impl Stat {
    /// Convenience constructor for a registry-keyed statistic.
    #[must_use]
    pub const fn keyed(stat_type: StatType, key: Identifier) -> Self {
        Self::Keyed { stat_type, key }
    }

    /// The family this statistic belongs to.
    #[must_use]
    pub const fn stat_type(&self) -> StatType {
        match self {
            Self::Custom(_) => StatType::Custom,
            Self::Keyed { stat_type, .. } => *stat_type,
        }
    }

    /// The persisted key, as `<type path>/<entry path>` (vanilla's `Stat.buildName` shape).
    #[must_use]
    pub fn storage_key(&self) -> String {
        match self {
            Self::Custom(stat) => format!("custom/{}", stat.path()),
            Self::Keyed { stat_type, key } => format!("{}/{}", stat_type.path(), key),
        }
    }
}

/// Vanilla `ServerStatsCounter`: a player's statistic totals plus the dirty set awaiting sync.
#[derive(Debug, Default)]
pub struct StatsCounter {
    values: FxHashMap<Stat, i32>,
    dirty: Vec<Stat>,
}

impl StatsCounter {
    /// Creates an empty counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current total for one statistic.
    #[must_use]
    pub fn get(&self, stat: &Stat) -> i32 {
        self.values.get(stat).copied().unwrap_or(0)
    }

    /// Vanilla `StatsCounter.increment`: adds `amount`, saturating rather than overflowing.
    ///
    /// Distance statistics accumulate in centimetres over a long session, so wrapping would
    /// turn a large total negative.
    pub fn increment(&mut self, stat: Stat, amount: i32) {
        let updated = self.get(&stat).saturating_add(amount);
        self.set(stat, updated);
    }

    /// Vanilla `StatsCounter.setValue`.
    pub fn set(&mut self, stat: Stat, value: i32) {
        if !self.dirty.contains(&stat) {
            self.dirty.push(stat.clone());
        }
        self.values.insert(stat, value);
    }

    /// Drains the statistics changed since the last sync, for `ClientboundAwardStatsPacket`.
    pub fn drain_dirty(&mut self) -> Vec<(Stat, i32)> {
        let dirty: Vec<_> = self.dirty.drain(..).collect();
        dirty
            .into_iter()
            .map(|stat| {
                let value = self.values.get(&stat).copied().unwrap_or(0);
                (stat, value)
            })
            .collect()
    }

    /// Every recorded statistic, for persistence and `/stats`.
    pub fn entries(&self) -> impl Iterator<Item = (&Stat, i32)> {
        self.values.iter().map(|(stat, value)| (stat, *value))
    }

    /// Restores counters from persisted `(storage key, value)` pairs.
    ///
    /// Unknown keys are skipped so a save written by a newer version still loads.
    pub fn load_from_pairs<'a>(&mut self, pairs: impl Iterator<Item = (&'a str, i32)>) {
        for (key, value) in pairs {
            if let Some(stat) = parse_storage_key(key) {
                self.values.insert(stat, value);
            }
        }
        self.dirty.clear();
    }
}

/// Parses a `<type path>/<entry>` storage key back into a [`Stat`].
fn parse_storage_key(key: &str) -> Option<Stat> {
    let (type_path, entry) = key.split_once('/')?;
    if type_path == StatType::Custom.path() {
        return CustomStat::from_path(entry).map(Stat::Custom);
    }

    let stat_type = [
        StatType::BlockMined,
        StatType::ItemCrafted,
        StatType::ItemUsed,
        StatType::ItemBroken,
        StatType::ItemPickedUp,
        StatType::ItemDropped,
        StatType::EntityKilled,
        StatType::EntityKilledBy,
    ]
    .into_iter()
    .find(|candidate| candidate.path() == type_path)?;

    Some(Stat::keyed(stat_type, entry.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::{CustomStat, Stat, StatType, StatsCounter, parse_storage_key};
    use steel_utils::Identifier;

    /// Registry ids are positional, so an accidental reorder silently changes the wire
    /// format for every stat after the edit.
    #[test]
    fn custom_stat_ids_follow_vanilla_declaration_order() {
        assert_eq!(CustomStat::ALL.len(), 77);
        assert_eq!(CustomStat::LEAVE_GAME.id(), Some(0));
        assert_eq!(CustomStat::PLAY_TIME.id(), Some(1));
        assert_eq!(
            CustomStat::INTERACT_WITH_SMITHING_TABLE.id(),
            Some(CustomStat::ALL.len() - 1)
        );
    }

    /// Same hazard for the nine stat types; `custom` must stay last.
    #[test]
    fn stat_type_ids_follow_vanilla_declaration_order() {
        assert_eq!(StatType::BlockMined.id(), 0);
        assert_eq!(StatType::ItemUsed.id(), 2);
        assert_eq!(StatType::EntityKilledBy.id(), 7);
        assert_eq!(StatType::Custom.id(), 8);
    }

    #[test]
    fn increment_accumulates_and_marks_dirty_once() {
        let mut stats = StatsCounter::new();
        let jump = Stat::Custom(CustomStat::JUMP);
        stats.increment(jump.clone(), 2);
        stats.increment(jump.clone(), 3);
        assert_eq!(stats.get(&jump), 5);

        assert_eq!(stats.drain_dirty(), vec![(jump, 5)]);
        assert!(stats.drain_dirty().is_empty());
    }

    /// Distance stats accumulate in centimetres over a long session, so the counter must not
    /// wrap into negative territory.
    #[test]
    fn increment_saturates_instead_of_overflowing() {
        let mut stats = StatsCounter::new();
        let walked = Stat::Custom(CustomStat::WALK_ONE_CM);
        stats.set(walked.clone(), i32::MAX - 1);
        stats.increment(walked.clone(), 100);
        assert_eq!(stats.get(&walked), i32::MAX);
    }

    /// The two families share one counter, exactly as vanilla's `Object2IntMap<Stat<?>>` does.
    #[test]
    fn custom_and_keyed_stats_coexist() {
        let mut stats = StatsCounter::new();
        let used_bow = Stat::keyed(StatType::ItemUsed, Identifier::vanilla_static("bow"));
        let mined_stone = Stat::keyed(StatType::BlockMined, Identifier::vanilla_static("stone"));

        stats.increment(Stat::Custom(CustomStat::JUMP), 1);
        stats.increment(used_bow.clone(), 4);
        stats.increment(mined_stone.clone(), 9);

        assert_eq!(stats.get(&used_bow), 4);
        assert_eq!(stats.get(&mined_stone), 9);
        assert_eq!(stats.get(&Stat::Custom(CustomStat::JUMP)), 1);
        // Same entry path in a different family must not collide.
        assert_eq!(
            stats.get(&Stat::keyed(
                StatType::ItemBroken,
                Identifier::vanilla_static("bow")
            )),
            0
        );
    }

    /// Storage keys must round-trip so saved statistics reload into the right family.
    #[test]
    fn storage_keys_round_trip() {
        for stat in [
            Stat::Custom(CustomStat::JUMP),
            Stat::keyed(StatType::ItemUsed, Identifier::vanilla_static("bow")),
            Stat::keyed(
                StatType::EntityKilled,
                Identifier::vanilla_static("creeper"),
            ),
        ] {
            let key = stat.storage_key();
            assert_eq!(parse_storage_key(&key).as_ref(), Some(&stat), "key {key}");
        }
    }

    #[test]
    fn load_skips_unknown_keys() {
        let mut stats = StatsCounter::new();
        stats.load_from_pairs(
            [
                ("custom/jump", 7),
                ("custom/not_a_real_stat", 3),
                ("not_a_family/stone", 5),
                ("used/minecraft:bow", 2),
            ]
            .into_iter(),
        );
        assert_eq!(stats.get(&Stat::Custom(CustomStat::JUMP)), 7);
        assert_eq!(
            stats.get(&Stat::keyed(
                StatType::ItemUsed,
                Identifier::vanilla_static("bow")
            )),
            2
        );
        assert!(stats.drain_dirty().is_empty());
    }
}
