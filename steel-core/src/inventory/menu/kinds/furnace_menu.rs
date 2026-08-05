//! Furnace, smoker and blast furnace menus.

use steel_registry::menu_type::MenuTypeRef;
use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a furnace menu.
#[must_use]
pub fn furnace(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    furnace_like(inventory, container_id, container, &vanilla_menu_types::FURNACE)
}

/// Builds a smoker menu.
#[must_use]
pub fn smoker(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    furnace_like(inventory, container_id, container, &vanilla_menu_types::SMOKER)
}

/// Builds a blast furnace menu.
#[must_use]
pub fn blast_furnace(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    furnace_like(inventory, container_id, container, &vanilla_menu_types::BLAST_FURNACE)
}

/// Builds a brewing stand menu.
#[must_use]
pub fn brewing_stand(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    let container = container.into();
    let mut builder = MenuBuilder::new(&vanilla_menu_types::BREWING_STAND, container_id);
    let brewing = builder.section(&container, 5);
    let player = builder.player_inventory(&inventory);
    let _fuel = builder.data_slot(0);
    let _brew_time = builder.data_slot(0);
    builder.route(brewing, player.all(), FillDirection::Backward);
    builder.route(player.all(), brewing, FillDirection::Forward);
    builder.build(BrewingStandKind { container })
}

struct BrewingStandKind { container: ContainerRef }
unsafe impl steel_utils::DowncastType for BrewingStandKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey = steel_utils::DowncastTypeKey::new("steel:menu/brewing_stand");
}
impl MenuKind for BrewingStandKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool { self.container.still_valid(player) }
}

fn furnace_like(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    menu_type: MenuTypeRef,
) -> Menu {
    let container = container.into();
    let mut builder = MenuBuilder::new(menu_type, container_id);
    // Furnace has 3 slots: input, fuel, result
    let furnace_section = builder.section(&container, 3);
    let player = builder.player_inventory(&inventory);
    // Data slots for lit time, lit duration, cooking progress, cooking total — initial 0
    let _d0 = builder.data_slot(0);
    let _d1 = builder.data_slot(0);
    let _d2 = builder.data_slot(0);
    let _d3 = builder.data_slot(0);

    builder.route(furnace_section, player.all(), FillDirection::Backward);
    builder.route(player.all(), furnace_section, FillDirection::Forward);

    builder.build(FurnaceMenuKind { container })
}

struct FurnaceMenuKind {
    container: ContainerRef,
}

unsafe impl steel_utils::DowncastType for FurnaceMenuKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey = steel_utils::DowncastTypeKey::new("steel:menu/furnace");
}

impl MenuKind for FurnaceMenuKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}
