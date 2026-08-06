//! Vanilla `SpawnData` and its weighted spawn-potentials list.

use std::sync::Arc;

use rand::RngExt as _;
use simdnbt::borrow::NbtCompound as NbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_utils::BlockPos;

use crate::chunk::light::LightLayer;
use crate::world::World;

/// Vanilla `SpawnData.ENTITY_TAG`.
pub(super) const ENTITY_TAG: &str = "entity";

/// Vanilla light limits are always within `[0, 15]`.
const LIGHT_RANGE: InclusiveRange = InclusiveRange {
    min_inclusive: 0,
    max_inclusive: 15,
};

/// Vanilla `InclusiveRange<Integer>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InclusiveRange {
    min_inclusive: i32,
    max_inclusive: i32,
}

impl InclusiveRange {
    /// Returns vanilla `InclusiveRange.isValueInRange`.
    const fn contains_value(self, value: i32) -> bool {
        value >= self.min_inclusive && value <= self.max_inclusive
    }

    const fn is_within(self, outer: Self) -> bool {
        self.min_inclusive >= outer.min_inclusive && self.max_inclusive <= outer.max_inclusive
    }

    /// Reads vanilla `ExtraCodecs.intervalCodec`: either a bare value or a
    /// `{min_inclusive, max_inclusive}` compound.
    fn read(tag: Option<&NbtTag>, fallback: Self) -> Self {
        let Some(tag) = tag else { return fallback };

        if let Some(value) = tag.int() {
            return Self {
                min_inclusive: value,
                max_inclusive: value,
            };
        }

        let Some(compound) = tag.compound() else {
            return fallback;
        };
        let (Some(min_inclusive), Some(max_inclusive)) = (
            compound.get("min_inclusive").and_then(NbtTag::int),
            compound.get("max_inclusive").and_then(NbtTag::int),
        ) else {
            return fallback;
        };

        let range = Self {
            min_inclusive,
            max_inclusive,
        };
        // Vanilla rejects inverted and out-of-bounds ranges during decoding; a rejected
        // range leaves the field at its default.
        if min_inclusive > max_inclusive || !range.is_within(LIGHT_RANGE) {
            return fallback;
        }
        range
    }

    fn write(self) -> NbtTag {
        if self.min_inclusive == self.max_inclusive {
            return NbtTag::Int(self.min_inclusive);
        }
        let mut compound = NbtCompound::new();
        compound.insert("min_inclusive", self.min_inclusive);
        compound.insert("max_inclusive", self.max_inclusive);
        NbtTag::Compound(compound)
    }
}

/// Vanilla `SpawnData.CustomSpawnRules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CustomSpawnRules {
    block_light_limit: InclusiveRange,
    sky_light_limit: InclusiveRange,
}

impl CustomSpawnRules {
    /// Returns vanilla `CustomSpawnRules.isValidPosition`.
    pub(super) fn is_valid_position(self, pos: BlockPos, world: &Arc<World>) -> bool {
        let block_light = i32::from(world.light_value_at(LightLayer::Block, pos));
        // Vanilla `LevelReader.getEffectiveSkyBrightness`, which subtracts as a plain
        // `int` and may go negative during a storm.
        let sky_light = i32::from(world.light_value_at(LightLayer::Sky, pos))
            - i32::from(world.sky_darkening());
        self.block_light_limit.contains_value(block_light)
            && self.sky_light_limit.contains_value(sky_light)
    }

    fn read(compound: &NbtCompound) -> Self {
        Self {
            block_light_limit: InclusiveRange::read(compound.get("block_light_limit"), LIGHT_RANGE),
            sky_light_limit: InclusiveRange::read(compound.get("sky_light_limit"), LIGHT_RANGE),
        }
    }

    fn write(self) -> NbtCompound {
        let mut compound = NbtCompound::new();
        compound.insert("block_light_limit", self.block_light_limit.write());
        compound.insert("sky_light_limit", self.sky_light_limit.write());
        compound
    }
}

/// Vanilla `SpawnData`.
///
/// Vanilla's third field, `equipment` (an `EquipmentTable`), is not modeled: it is a
/// loot-table-driven equipment roll that Steel has no equivalent for yet. Only
/// datapack-authored spawners set it.
#[derive(Debug, Clone, Default)]
pub(super) struct SpawnData {
    entity_to_spawn: NbtCompound,
    custom_spawn_rules: Option<CustomSpawnRules>,
}

impl SpawnData {
    pub(super) const fn entity_to_spawn(&self) -> &NbtCompound {
        &self.entity_to_spawn
    }

    pub(super) const fn entity_to_spawn_mut(&mut self) -> &mut NbtCompound {
        &mut self.entity_to_spawn
    }

    pub(super) const fn custom_spawn_rules(&self) -> Option<CustomSpawnRules> {
        self.custom_spawn_rules
    }

    pub(super) fn read(tag: &NbtTag) -> Option<Self> {
        let compound = tag.compound()?;
        Some(Self {
            entity_to_spawn: compound
                .get(ENTITY_TAG)
                .and_then(NbtTag::compound)
                .cloned()
                .unwrap_or_default(),
            custom_spawn_rules: compound
                .get("custom_spawn_rules")
                .and_then(NbtTag::compound)
                .map(CustomSpawnRules::read),
        })
    }

    pub(super) fn write(&self) -> NbtCompound {
        let mut compound = NbtCompound::new();
        compound.insert(ENTITY_TAG, NbtTag::Compound(self.entity_to_spawn.clone()));
        if let Some(rules) = self.custom_spawn_rules {
            compound.insert("custom_spawn_rules", NbtTag::Compound(rules.write()));
        }
        compound
    }
}

/// Vanilla `WeightedList<SpawnData>` as stored in spawner NBT.
#[derive(Debug, Clone, Default)]
pub(super) struct SpawnPotentials {
    entries: Vec<WeightedSpawnData>,
    total_weight: i32,
}

#[derive(Debug, Clone)]
struct WeightedSpawnData {
    data: SpawnData,
    weight: i32,
}

impl SpawnPotentials {
    pub(super) fn of(data: SpawnData) -> Self {
        Self::from_entries(vec![WeightedSpawnData { data, weight: 1 }])
    }

    fn from_entries(entries: Vec<WeightedSpawnData>) -> Self {
        let total_weight = entries.iter().map(|entry| entry.weight).sum();
        Self {
            entries,
            total_weight,
        }
    }

    /// Returns vanilla `WeightedList.getRandom`.
    pub(super) fn random(&self, rng: &mut impl rand::Rng) -> Option<&SpawnData> {
        if self.total_weight <= 0 {
            return None;
        }

        let mut remaining = rng.random_range(0..self.total_weight);
        for entry in &self.entries {
            remaining -= entry.weight;
            if remaining < 0 {
                return Some(&entry.data);
            }
        }
        None
    }

    pub(super) fn read(nbt: &NbtCompoundView<'_, '_>, key: &str) -> Option<Self> {
        let list = nbt.list(key)?.compounds()?;
        let entries = list
            .into_iter()
            .filter_map(|entry| {
                let owned = NbtTag::Compound(entry.to_owned());
                let compound = owned.compound()?;
                Some(WeightedSpawnData {
                    data: SpawnData::read(compound.get("data")?)?,
                    // Vanilla `WeightedList` defaults a missing weight to 1.
                    weight: compound.get("weight").and_then(NbtTag::int).unwrap_or(1),
                })
            })
            .collect();
        Some(Self::from_entries(entries))
    }

    pub(super) fn write(&self) -> NbtList {
        NbtList::Compound(
            self.entries
                .iter()
                .map(|entry| {
                    let mut compound = NbtCompound::new();
                    compound.insert("data", NbtTag::Compound(entry.data.write()));
                    compound.insert("weight", entry.weight);
                    compound
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use rand::{SeedableRng, rngs::StdRng};
    use simdnbt::owned::{NbtCompound, NbtTag};

    use super::{InclusiveRange, LIGHT_RANGE, SpawnData, SpawnPotentials};

    fn light_range(min_inclusive: i32, max_inclusive: i32) -> NbtTag {
        let mut compound = NbtCompound::new();
        compound.insert("min_inclusive", min_inclusive);
        compound.insert("max_inclusive", max_inclusive);
        NbtTag::Compound(compound)
    }

    #[test]
    fn inclusive_range_rejects_inverted_and_out_of_bounds_light_limits() {
        // Vanilla's codec validates both, so a bad range must fall back to the default
        // rather than silently disabling or inverting the spawn check.
        assert_eq!(
            InclusiveRange::read(Some(&light_range(9, 3)), LIGHT_RANGE),
            LIGHT_RANGE
        );
        assert_eq!(
            InclusiveRange::read(Some(&light_range(0, 16)), LIGHT_RANGE),
            LIGHT_RANGE
        );
        assert_eq!(
            InclusiveRange::read(Some(&light_range(3, 9)), LIGHT_RANGE),
            InclusiveRange {
                min_inclusive: 3,
                max_inclusive: 9,
            }
        );
    }

    #[test]
    fn inclusive_range_reads_a_bare_value_as_a_single_point() {
        // Vanilla `intervalCodec` accepts an unwrapped element for min == max.
        assert_eq!(
            InclusiveRange::read(Some(&NbtTag::Int(7)), LIGHT_RANGE),
            InclusiveRange {
                min_inclusive: 7,
                max_inclusive: 7,
            }
        );
    }

    #[test]
    fn empty_spawn_potentials_never_yields_an_entry() {
        // `getRandom` on an empty weighted list must return nothing rather than
        // panicking on an empty sampling range.
        let potentials = SpawnPotentials::default();
        assert!(potentials.random(&mut StdRng::seed_from_u64(1)).is_none());
    }

    #[test]
    fn spawn_potentials_never_selects_a_zero_weight_entry() {
        let mut zero_weighted = SpawnData::default();
        zero_weighted
            .entity_to_spawn_mut()
            .insert("id", "minecraft:zombie");
        let mut only_reachable = SpawnData::default();
        only_reachable
            .entity_to_spawn_mut()
            .insert("id", "minecraft:skeleton");

        let potentials = SpawnPotentials::from_entries(vec![
            super::WeightedSpawnData {
                data: zero_weighted,
                weight: 0,
            },
            super::WeightedSpawnData {
                data: only_reachable,
                weight: 1,
            },
        ]);

        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..32 {
            let Some(selected) = potentials.random(&mut rng) else {
                panic!("a positive total weight should always select an entry");
            };
            assert_eq!(
                selected
                    .entity_to_spawn()
                    .string("id")
                    .map(ToString::to_string),
                Some("minecraft:skeleton".to_owned())
            );
        }
    }
}
