//! Gossip / reputation for villagers.
//!
//! Mirrors `net.minecraft.world.entity.ai.gossip.GossipContainer` + `GossipType`
//! + `ReputationEventType` at the level needed for gameplay: trade, hurt, killed, cured.

use std::collections::HashMap;

use uuid::Uuid;

/// Gossip type weight - mirrors vanilla `GossipType.weight` and reputation deltas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GossipType {
    /// Major negative gossip (villager killed: -25).
    MajorNegative,
    /// Minor negative gossip (villager hurt: -25).
    MinorNegative,
    /// Major positive gossip (zombie cured: +20).
    MajorPositive,
    /// Minor positive gossip (zombie cured: +25).
    MinorPositive,
    /// Trading gossip (trade: +2).
    Trading,
}

impl GossipType {
    /// Weight contribution for this gossip type.
    #[must_use]
    pub const fn weight(self) -> i32 {
        match self {
            Self::MajorNegative => -25,
            Self::MinorNegative => -25,
            Self::MajorPositive => 20,
            Self::MinorPositive => 25,
            Self::Trading => 2,
        }
    }

    /// Maximum stored value for this gossip type.
    #[must_use]
    pub const fn max_value(self) -> i32 {
        match self {
            Self::MajorNegative => 25,
            Self::MinorNegative => 25,
            Self::MajorPositive => 20,
            Self::MinorPositive => 25,
            Self::Trading => 20,
        }
    }
}

/// Per-player gossip entry.
#[derive(Debug, Clone, Default)]
struct GossipEntry {
    values: [i32; 5],
}

impl GossipEntry {
    fn get(&self, ty: GossipType) -> i32 {
        self.values[ty as usize]
    }
    fn set(&mut self, ty: GossipType, value: i32) {
        self.values[ty as usize] = value.clamp(0, ty.max_value());
    }
    fn add(&mut self, ty: GossipType, delta: i32) {
        let cur = self.get(ty);
        self.set(ty, cur + delta);
    }
    fn reputation(&self) -> i32 {
        // Simplified: sum of weighted values (vanilla applies decay and caps)
        self.values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let ty = match i {
                    0 => GossipType::MajorNegative,
                    1 => GossipType::MinorNegative,
                    2 => GossipType::MajorPositive,
                    3 => GossipType::MinorPositive,
                    4 => GossipType::Trading,
                    _ => unreachable!(),
                };
                if ty == GossipType::MajorNegative || ty == GossipType::MinorNegative {
                    -v * 5
                } else {
                    ty.weight() * *v / ty.max_value() * 5
                }
            })
            .sum()
    }
    fn decay(&mut self) {
        for i in 0..5 {
            let v = self.values[i];
            if v > 0 {
                self.values[i] = (v - 1).max(0);
            }
        }
    }
}

/// Container for all player reputations known by a villager.
#[derive(Debug, Clone, Default)]
pub struct GossipContainer {
    entries: HashMap<Uuid, GossipEntry>,
}

impl GossipContainer {
    /// Creates an empty gossip container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `delta` gossip of type `ty` for `player`.
    pub fn add(&mut self, player: Uuid, ty: GossipType, delta: i32) {
        let entry = self.entries.entry(player).or_default();
        entry.add(ty, delta);
    }

    /// Returns the combined reputation for `player`.
    #[must_use]
    pub fn reputation(&self, player: Uuid) -> i32 {
        self.entries.get(&player).map_or(0, |e| e.reputation())
    }

    /// Decays all gossip values by one step.
    pub fn decay(&mut self) {
        for e in self.entries.values_mut() {
            e.decay();
        }
        self.entries.retain(|_, e| e.values.iter().any(|v| *v != 0));
    }

    /// Clears all gossip entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns the number of players with gossip entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no gossip is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
