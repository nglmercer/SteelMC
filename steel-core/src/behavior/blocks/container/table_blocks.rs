//! Cartography, smithing and stonecutter table behaviors.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::player::Player;
use crate::world::World;

/// Vanilla `CartographyTableBlock` — opens the cartography table menu.
#[block_behavior]
pub struct CartographyTableBlock {
    block: BlockRef,
}
impl CartographyTableBlock {
    /// Creates a cartography table block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}
impl BlockBehavior for CartographyTableBlock {
    fn get_state_for_placement(&self, _ctx: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let inv = player.inventory.clone();
        let w = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_CARTOGRAPHY_TABLE.msg()),
            move |ctx| {
                crate::inventory::menu::kinds::cartography_table(inv, ctx.container_id, pos, &w)
            },
        );
        InteractionResult::Success
    }
}

/// Vanilla `SmithingTableBlock` — opens the smithing table menu.
#[block_behavior]
pub struct SmithingTableBlock {
    block: BlockRef,
}
impl SmithingTableBlock {
    /// Creates a smithing table block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}
impl BlockBehavior for SmithingTableBlock {
    fn get_state_for_placement(&self, _ctx: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let inv = player.inventory.clone();
        let w = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_UPGRADE.msg()),
            move |ctx| {
                crate::inventory::menu::kinds::smithing_table(inv, ctx.container_id, pos, &w)
            },
        );
        InteractionResult::Success
    }
}

/// Vanilla `StonecutterBlock` — opens the stonecutter menu.
#[block_behavior]
pub struct StonecutterBlock {
    block: BlockRef,
}
impl StonecutterBlock {
    /// Creates a stonecutter block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}
impl BlockBehavior for StonecutterBlock {
    fn get_state_for_placement(&self, ctx: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        use steel_registry::blocks::block_state_ext::BlockStateExt;
        use steel_registry::blocks::properties::BlockStateProperties;
        Some(self.block.default_state().set_value(
            &BlockStateProperties::HORIZONTAL_FACING,
            ctx.horizontal_direction().opposite(),
        ))
    }
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let inv = player.inventory.clone();
        let w = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_STONECUTTER.msg()),
            move |ctx| crate::inventory::menu::kinds::stonecutter(inv, ctx.container_id, pos, &w),
        );
        InteractionResult::Success
    }
}
