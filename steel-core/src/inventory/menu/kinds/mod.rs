//! Vanilla menu kind implementations.

mod anvil_menu;
mod basic_menu;
mod chest_menu;
mod container_menu;
mod crafting_menu;
mod inventory_menu;

pub use anvil_menu::{AnvilKind, anvil};
pub use basic_menu::BasicKind;
pub use chest_menu::{ChestKind, DoubleChestKind, chest, double_chest};
pub use container_menu::{SingleContainerKind, dispenser, hopper};
pub use crafting_menu::{CraftingKind, crafting};
pub use inventory_menu::{INVENTORY_MENU_CONTAINER_ID, InventoryKind, inventory_menu};
