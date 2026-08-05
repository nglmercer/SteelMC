//! Menus that are a single block container plus the player inventory.
//!
//! Chests use their own builder because their row count picks the menu type; these are the
//! fixed-layout containers such as hoppers and dispensers.

use steel_registry::menu_type::MenuTypeRef;
use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Vanilla's hopper holds five slots in one row.
const HOPPER_SLOTS: usize = 5;
/// Vanilla's dispenser and dropper hold a three-by-three grid.
const DISPENSER_SLOTS: usize = 9;

/// Builds a hopper menu: five container slots plus the player inventory.
#[must_use]
pub fn hopper(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    single_container(
        inventory,
        container_id,
        container,
        &vanilla_menu_types::HOPPER,
        HOPPER_SLOTS,
    )
}

/// Builds a dispenser or dropper menu: a three-by-three grid plus the player inventory.
#[must_use]
pub fn dispenser(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    single_container(
        inventory,
        container_id,
        container,
        &vanilla_menu_types::GENERIC_3X3,
        DISPENSER_SLOTS,
    )
}

fn single_container(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    menu_type: MenuTypeRef,
    slots: usize,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(menu_type, container_id);
    let block = builder.section(&container, slots);
    let player = builder.player_inventory(&inventory);

    builder.route(block, player.all(), FillDirection::Backward);
    builder.route(player.all(), block, FillDirection::Forward);

    builder.build(SingleContainerKind { container })
}

/// Per-menu state: the backing container, for the reachability check.
pub struct SingleContainerKind {
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for SingleContainerKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/single_container");
}

impl MenuKind for SingleContainerKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}
