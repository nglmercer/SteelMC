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
    furnace_like(
        inventory,
        container_id,
        container,
        &vanilla_menu_types::FURNACE,
    )
}

/// Builds a smoker menu.
#[must_use]
pub fn smoker(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    furnace_like(
        inventory,
        container_id,
        container,
        &vanilla_menu_types::SMOKER,
    )
}

/// Builds a blast furnace menu.
#[must_use]
pub fn blast_furnace(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    furnace_like(
        inventory,
        container_id,
        container,
        &vanilla_menu_types::BLAST_FURNACE,
    )
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

struct BrewingStandKind {
    container: ContainerRef,
}
unsafe impl steel_utils::DowncastType for BrewingStandKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/brewing_stand");
}
impl MenuKind for BrewingStandKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
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
    let lit_time = builder.data_slot(0);
    let lit_duration = builder.data_slot(0);
    let cooking_progress = builder.data_slot(0);
    let cooking_total = builder.data_slot(0);

    builder.route(furnace_section, player.all(), FillDirection::Backward);
    builder.route(player.all(), furnace_section, FillDirection::Forward);

    builder.build(FurnaceMenuKind {
        container,
        lit_time,
        lit_duration,
        cooking_progress,
        cooking_total,
    })
}

struct FurnaceMenuKind {
    container: ContainerRef,
    lit_time: DataSlot,
    lit_duration: DataSlot,
    cooking_progress: DataSlot,
    cooking_total: DataSlot,
}

unsafe impl steel_utils::DowncastType for FurnaceMenuKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/furnace");
}

impl MenuKind for FurnaceMenuKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }

    fn on_tick(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        // Sync furnace data slots from the backing container each tick, matching vanilla
        // `AbstractFurnaceMenu` ContainerData. Use clamped i16 for client.
        let Some(container) = guard.get(self.container.container_id()) else {
            return;
        };
        use steel_utils::Downcast as _;
        if let Some(furnace) =
            container.downcast_ref::<crate::block_entity::entities::FurnaceContainer>()
        {
            self.lit_time.set(
                behavior,
                furnace.lit_time_remaining.clamp(0, i32::from(i16::MAX)) as i16,
            );
            self.lit_duration.set(
                behavior,
                furnace.lit_duration.clamp(0, i32::from(i16::MAX)) as i16,
            );
            self.cooking_progress.set(
                behavior,
                furnace.cooking_progress.clamp(0, i32::from(i16::MAX)) as i16,
            );
            self.cooking_total.set(
                behavior,
                furnace.cooking_total_time.clamp(0, i32::from(i16::MAX)) as i16,
            );
        }
    }
}
