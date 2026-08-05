//! Vanilla menu kind implementations.

mod anvil_menu;
mod basic_menu;
mod chest_menu;
mod container_menu;
mod crafting_menu;
mod enchantment_menu;
mod furnace_menu;
mod grindstone_menu;
mod inventory_menu;
mod loom_menu;
mod table_menu;

pub use anvil_menu::{AnvilKind, anvil};
pub use basic_menu::BasicKind;
pub use chest_menu::{ChestKind, DoubleChestKind, chest, double_chest};
pub use container_menu::{SingleContainerKind, dispenser, hopper};
pub use crafting_menu::{CraftingKind, crafting};
pub use enchantment_menu::{EnchantmentKind, enchantment};
pub use furnace_menu::{blast_furnace, brewing_stand, furnace, smoker};
pub use table_menu::{beacon, cartography_table, smithing_table, stonecutter};
pub use grindstone_menu::{GrindstoneKind, grindstone};
pub use inventory_menu::{INVENTORY_MENU_CONTAINER_ID, InventoryKind, inventory_menu};
pub use loom_menu::{LoomKind, loom};
