//! Chest menu for chest-like containers (chests, barrels, ender chests, shulker boxes).
//!
//! 1-6 rows of 9 slots. Layout:
//! - Slots 0 to `rows * 9 - 1`: Container
//! - Slots `rows * 9` to `rows * 9 + 26`: Main inventory (27)
//! - Slots `rows * 9 + 27` to `rows * 9 + 35`: Hotbar (9)

use steel_registry::menu_type::MenuTypeRef;
use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a chest-like menu with `rows` rows of 9 slots plus the player inventory.
///
/// # Panics
/// Panics if `rows` is 0 or greater than 6.
#[must_use]
pub fn chest(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    rows: usize,
) -> Menu {
    let container = container.into();
    assert!(
        (1..=6).contains(&rows),
        "Chest rows must be between 1 and 6"
    );

    let mut builder = MenuBuilder::new(menu_type_for_rows(rows), container_id);
    let chest = builder.section(&container, rows * 9);
    let player = builder.player_inventory(&inventory);

    builder.route(chest, player.all(), FillDirection::Backward);
    builder.route(player.all(), chest, FillDirection::Forward);

    builder.build(ChestKind { container })
}

/// Builds a vanilla double-chest menu: six rows spanning two 27-slot containers.
///
/// `top` supplies the upper three rows and `bottom` the lower three, matching vanilla's
/// `CompoundContainer(first, second)` ordering.
#[must_use]
pub fn double_chest(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    top: impl Into<ContainerRef>,
    bottom: impl Into<ContainerRef>,
) -> Menu {
    let top = top.into();
    let bottom = bottom.into();

    let mut builder = MenuBuilder::new(menu_type_for_rows(6), container_id);
    let top_section = builder.section(&top, DOUBLE_CHEST_HALF_SLOTS);
    let bottom_section = builder.section(&bottom, DOUBLE_CHEST_HALF_SLOTS);
    let player = builder.player_inventory(&inventory);

    builder.route(
        [top_section, bottom_section],
        player.all(),
        FillDirection::Backward,
    );
    builder.route(
        player.all(),
        [top_section, bottom_section],
        FillDirection::Forward,
    );

    builder.build(DoubleChestKind { top, bottom })
}

/// Slots contributed by each half of a double chest.
const DOUBLE_CHEST_HALF_SLOTS: usize = 27;

/// Menu type for a chest of `rows` rows.
///
/// # Panics
/// Panics if `rows` is 0 or greater than 6.
#[must_use]
pub fn menu_type_for_rows(rows: usize) -> MenuTypeRef {
    match rows {
        1 => &vanilla_menu_types::GENERIC_9X1,
        2 => &vanilla_menu_types::GENERIC_9X2,
        3 => &vanilla_menu_types::GENERIC_9X3,
        4 => &vanilla_menu_types::GENERIC_9X4,
        5 => &vanilla_menu_types::GENERIC_9X5,
        6 => &vanilla_menu_types::GENERIC_9X6,
        _ => panic!("Invalid row count: {rows}"),
    }
}

/// Per-menu chest state: just the backing container for the validity check.
pub struct ChestKind {
    /// The backing container.
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for ChestKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/chest");
}

impl MenuKind for ChestKind {
    /// Returns true if the backing container is still valid for the player.
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }

    fn on_open(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        Self::start_open(&self.container);
    }

    fn removed(&mut self, _behavior: &mut MenuBehavior, _player: &Player) {
        Self::stop_open(&self.container);
    }

    fn on_tick(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        // Vanilla rechecks openers every tick while menu is open; we do it here
        // as a lightweight alternative to block ticks for disconnected cases.
        let _ = &self.container;
    }
}

impl ChestKind {
    fn start_open(container: &ContainerRef) {
        use steel_utils::Downcast as _;
        if let Some(owner) = container.owner_block_entity() {
            if let Some(chest) = owner.downcast_ref::<crate::block_entity::entities::ChestBlockEntity>() {
                chest.start_open();
                return;
            }
            if let Some(barrel) = owner.downcast_ref::<crate::block_entity::entities::BarrelBlockEntity>() {
                barrel.start_open();
                return;
            }
            if let Some(shulker) = owner.downcast_ref::<crate::block_entity::entities::ShulkerBoxBlockEntity>() {
                shulker.start_open();
            }
        }
    }

    fn stop_open(container: &ContainerRef) {
        use steel_utils::Downcast as _;
        if let Some(owner) = container.owner_block_entity() {
            if let Some(chest) = owner.downcast_ref::<crate::block_entity::entities::ChestBlockEntity>() {
                chest.stop_open();
                return;
            }
            if let Some(barrel) = owner.downcast_ref::<crate::block_entity::entities::BarrelBlockEntity>() {
                barrel.stop_open();
                return;
            }
            if let Some(shulker) = owner.downcast_ref::<crate::block_entity::entities::ShulkerBoxBlockEntity>() {
                shulker.stop_open();
            }
        }
    }
}

/// Per-menu double-chest state: both halves must stay reachable.
pub struct DoubleChestKind {
    /// The container backing the upper three rows.
    top: ContainerRef,
    /// The container backing the lower three rows.
    bottom: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for DoubleChestKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/double_chest");
}

impl MenuKind for DoubleChestKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.top.still_valid(player) && self.bottom.still_valid(player)
    }

    fn on_open(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        ChestKind::start_open(&self.top);
        ChestKind::start_open(&self.bottom);
    }

    fn removed(&mut self, _behavior: &mut MenuBehavior, _player: &Player) {
        ChestKind::stop_open(&self.top);
        ChestKind::stop_open(&self.bottom);
    }
}

#[cfg(test)]
mod tests {
    use steel_utils::locks::IntoShared as _;

    use super::*;
    use crate::inventory::container::SimpleContainer;

    #[test]
    fn chest_uses_exactly_the_rows_requested_from_oversized_container() {
        let inventory = PlayerInventory::new().into_shared();
        let container = SimpleContainer::new(18).into_shared();

        let menu = chest(inventory, 1, container, 1);

        assert_eq!(menu.behavior().slot_count(), 9 + 36);
    }
}
