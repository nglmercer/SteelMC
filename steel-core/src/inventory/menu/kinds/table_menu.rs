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
    builder.build(TableKind {
        menu_type: "cartography",
    })
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
    builder.build(TableKind {
        menu_type: "smithing",
    })
}

/// Stonecutter: input + result, with stonecutting recipes.
#[must_use]
pub fn stonecutter(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    _pos: steel_utils::BlockPos,
    _world: &std::sync::Arc<crate::world::World>,
) -> Menu {
    let input = SimpleContainer::new(1).into_shared();
    let result = SimpleContainer::new(1).into_shared();
    let mut builder = MenuBuilder::new(&vanilla_menu_types::STONECUTTER, container_id);
    let input_section = builder.section(&ContainerRef::from(input.clone()), 1);
    let result_section = builder.section(&ContainerRef::from(result.clone()), 1);
    let player = builder.player_inventory(&inventory);
    builder.route(result_section, player.all(), FillDirection::Backward);
    builder.route(player.all(), input_section, FillDirection::Forward);
    builder.route(input_section, result_section, FillDirection::Forward);
    builder.build(StonecutterKind { input, result })
}

struct StonecutterKind {
    input: Shared<SimpleContainer>,
    result: Shared<SimpleContainer>,
}

unsafe impl steel_utils::DowncastType for StonecutterKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/stonecutter");
}

impl MenuKind for StonecutterKind {
    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        let input_stack = self.input.lock().get_item(0).clone();
        let result_stack = if input_stack.is_empty() {
            steel_registry::item_stack::ItemStack::empty()
        } else {
            steel_registry::REGISTRY
                .recipes
                .find_stonecutting_result(&input_stack)
                .unwrap_or(steel_registry::item_stack::ItemStack::empty())
        };
        self.result.lock().set_item(0, result_stack);
    }

    fn on_slot_clicked(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        click: crate::inventory::click::Click,
        _player: &Player,
    ) -> crate::inventory::click::ClickOutcome {
        // Result slot is global slot 1 (after input slot 0)
        if let crate::inventory::click::Click::Pickup { slot: 1, .. }
        | crate::inventory::click::Click::QuickMove { slot: 1 } = click
        {
            if !self.result.lock().get_item(0).is_empty()
                && !self.input.lock().get_item(0).is_empty()
            {
                let mut input = self.input.lock().get_item(0).clone();
                input.shrink(1);
                self.input.lock().set_item(0, input);
                // result will be recomputed via slots_changed on next tick, but update now
                // keep result for player to take (default handling will move it)
            }
        }
        crate::inventory::click::ClickOutcome::Fallthrough
    }

    fn still_valid(&self, _behavior: &MenuBehavior, _player: &Player) -> bool {
        true
    }
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
    builder.build(TableKind {
        menu_type: "beacon",
    })
}

struct TableKind {
    // Retained to distinguish cartography/smithing/beacon menu types for debugging/downcasting.
    #[expect(dead_code, reason = "distinguishes table menu variants")]
    menu_type: &'static str,
}
unsafe impl steel_utils::DowncastType for TableKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/table");
}
impl MenuKind for TableKind {
    fn still_valid(&self, _behavior: &MenuBehavior, _player: &Player) -> bool {
        true
    }
}
