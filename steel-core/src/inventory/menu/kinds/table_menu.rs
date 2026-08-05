//! Simple table menus — cartography, smithing, stonecutter, beacon.

use steel_registry::vanilla_menu_types;
use steel_utils::locks::IntoShared as _;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Cartography table: 3 slots (map, paper, result) — simplified as 3.
#[must_use]
pub fn cartography_table(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    _pos: steel_utils::BlockPos,
    _world: &std::sync::Arc<crate::world::World>,
) -> Menu {
    let mut builder = MenuBuilder::new(&vanilla_menu_types::CARTOGRAPHY_TABLE, container_id);
    let table = builder.section_all(SimpleContainer::new(3).into_shared());
    let player = builder.player_inventory(&inventory);
    builder.route(table, player.all(), FillDirection::Backward);
    builder.route(player.all(), table, FillDirection::Forward);
    builder.build(TableKind { menu_type: "cartography" })
}

/// Smithing table: 3 slots.
#[must_use]
pub fn smithing_table(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    _pos: steel_utils::BlockPos,
    _world: &std::sync::Arc<crate::world::World>,
) -> Menu {
    let mut builder = MenuBuilder::new(&vanilla_menu_types::SMITHING, container_id);
    let table = builder.section_all(SimpleContainer::new(3).into_shared());
    let player = builder.player_inventory(&inventory);
    builder.route(table, player.all(), FillDirection::Backward);
    builder.route(player.all(), table, FillDirection::Forward);
    builder.build(TableKind { menu_type: "smithing" })
}

/// Stonecutter: 2 slots (input, result).
#[must_use]
pub fn stonecutter(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    _pos: steel_utils::BlockPos,
    _world: &std::sync::Arc<crate::world::World>,
) -> Menu {
    let mut builder = MenuBuilder::new(&vanilla_menu_types::STONECUTTER, container_id);
    let table = builder.section_all(SimpleContainer::new(2).into_shared());
    let player = builder.player_inventory(&inventory);
    builder.route(table, player.all(), FillDirection::Backward);
    builder.route(player.all(), table, FillDirection::Forward);
    builder.build(TableKind { menu_type: "stonecutter" })
}

/// Beacon: 1 payment slot (transient).
#[must_use]
pub fn beacon(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    _pos: steel_utils::BlockPos,
    _world: &std::sync::Arc<crate::world::World>,
) -> Menu {
    let container = SimpleContainer::new(1).into_shared();
    let mut builder = MenuBuilder::new(&vanilla_menu_types::BEACON, container_id);
    let beacon_section = builder.section_all(container.clone());
    let player = builder.player_inventory(&inventory);
    builder.route(beacon_section, player.all(), FillDirection::Backward);
    builder.route(player.all(), beacon_section, FillDirection::Forward);
    builder.build(TableKind { menu_type: "beacon" })
}

struct TableKind { menu_type: &'static str }
unsafe impl steel_utils::DowncastType for TableKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey = steel_utils::DowncastTypeKey::new("steel:menu/table");
}
impl MenuKind for TableKind {
    fn still_valid(&self, _behavior: &MenuBehavior, _player: &Player) -> bool { true }
}
