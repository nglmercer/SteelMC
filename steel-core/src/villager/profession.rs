//! Villager profession helpers.
//!
//! Wraps `steel_registry::vanilla_villager_professions` and provides workstation
//! lookup matching vanilla `VillagerProfession` secondary POI / job-site mapping.

use steel_registry::{REGISTRY, RegistryExt};
use steel_utils::Identifier;

/// Stable villager profession kind mirroring vanilla registry order.
///
/// Registry IDs (from `build_assets/villager_professions.json`):
/// 0 none, 1 armorer … 14 weaponsmith (nitwit has no workstation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VillagerProfessionKind {
    /// Unemployed villager.
    None = 0,
    /// Armorer profession.
    Armorer = 1,
    /// Butcher profession.
    Butcher = 2,
    /// Cartographer profession.
    Cartographer = 3,
    /// Cleric profession.
    Cleric = 4,
    /// Farmer profession.
    Farmer = 5,
    /// Fisherman profession.
    Fisherman = 6,
    /// Fletcher profession.
    Fletcher = 7,
    /// Leatherworker profession.
    Leatherworker = 8,
    /// Librarian profession.
    Librarian = 9,
    /// Mason profession.
    Mason = 10,
    /// Nitwit profession (no workstation, cannot level).
    Nitwit = 11,
    /// Shepherd profession.
    Shepherd = 12,
    /// Toolsmith profession.
    Toolsmith = 13,
    /// Weaponsmith profession.
    Weaponsmith = 14,
}

impl VillagerProfessionKind {
    /// Converts a numeric registry id to a profession kind.
    #[must_use]
    pub const fn from_id(id: i32) -> Option<Self> {
        match id {
            0 => Some(Self::None),
            1 => Some(Self::Armorer),
            2 => Some(Self::Butcher),
            3 => Some(Self::Cartographer),
            4 => Some(Self::Cleric),
            5 => Some(Self::Farmer),
            6 => Some(Self::Fisherman),
            7 => Some(Self::Fletcher),
            8 => Some(Self::Leatherworker),
            9 => Some(Self::Librarian),
            10 => Some(Self::Mason),
            11 => Some(Self::Nitwit),
            12 => Some(Self::Shepherd),
            13 => Some(Self::Toolsmith),
            14 => Some(Self::Weaponsmith),
            _ => None,
        }
    }

    /// Returns the numeric registry id for this profession.
    #[must_use]
    pub const fn id(self) -> i32 {
        self as i32
    }

    /// Returns the registry identifier for this profession.
    #[must_use]
    pub fn key(self) -> Identifier {
        let reg = &REGISTRY.villager_professions;
        // SAFETY: registry is frozen before any villager is created
        let entry = reg
            .by_id(self.id() as usize)
            .expect("villager profession id in range");
        entry.key.clone()
    }
}

/// Returns the workstation block identifier for a profession, if any.
///
/// Mirrors vanilla workstation mapping:
/// armorer→blast_furnace, butcher→smoker, cartographer→cartography_table,
/// cleric→brewing_stand, farmer→composter, fisherman→barrel, fletcher→fletching_table,
/// leatherworker→cauldron, librarian→lectern, mason→stonecutter,
/// shepherd→loom, toolsmith→smithing_table, weaponsmith→grindstone.
/// `none` and `nitwit` have no workstation.
#[must_use]
pub fn workstation_for_profession(kind: VillagerProfessionKind) -> Option<Identifier> {
    let name = match kind {
        VillagerProfessionKind::None | VillagerProfessionKind::Nitwit => return None,
        VillagerProfessionKind::Armorer => "blast_furnace",
        VillagerProfessionKind::Butcher => "smoker",
        VillagerProfessionKind::Cartographer => "cartography_table",
        VillagerProfessionKind::Cleric => "brewing_stand",
        VillagerProfessionKind::Farmer => "composter",
        VillagerProfessionKind::Fisherman => "barrel",
        VillagerProfessionKind::Fletcher => "fletching_table",
        VillagerProfessionKind::Leatherworker => "cauldron",
        VillagerProfessionKind::Librarian => "lectern",
        VillagerProfessionKind::Mason => "stonecutter",
        VillagerProfessionKind::Shepherd => "loom",
        VillagerProfessionKind::Toolsmith => "smithing_table",
        VillagerProfessionKind::Weaponsmith => "grindstone",
    };
    Some(Identifier::vanilla_static(name))
}

/// Returns true if a profession can level up (i.e. is not none/nitwit).
#[must_use]
pub const fn can_level_up(kind: VillagerProfessionKind) -> bool {
    !matches!(
        kind,
        VillagerProfessionKind::None | VillagerProfessionKind::Nitwit
    )
}

/// Max villager level is 5 (master). Mirrors `VillagerData.getMaxXpPerLevel`.
#[must_use]
pub const fn max_level() -> i32 {
    5
}

/// XP required to reach next level - mirrors vanilla progression (0,10,70,150,250).
#[must_use]
pub const fn xp_for_level(level: i32) -> i32 {
    match level {
        1 => 0,
        2 => 10,
        3 => 70,
        4 => 150,
        5 => 250,
        _ => i32::MAX,
    }
}
