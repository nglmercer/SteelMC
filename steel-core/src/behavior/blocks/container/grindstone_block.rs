//! Grindstone block behavior.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::blocks::redstone::face_attached_horizontal_directional_block::FaceAttachedHorizontalDirectionalBlock;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::entity::ai::path::PathComputationType;
use crate::inventory::menu::kinds::grindstone;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Vanilla `GrindstoneBlock` behavior.
#[block_behavior]
pub struct GrindstoneBlock {
    face_attached: FaceAttachedHorizontalDirectionalBlock,
}

impl GrindstoneBlock {
    /// Creates a new grindstone behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            face_attached: FaceAttachedHorizontalDirectionalBlock::new(block),
        }
    }
}

impl BlockBehavior for GrindstoneBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.face_attached.state_for_placement(context)
    }

    /// A grindstone never pops off its support, unlike the other face-attached blocks.
    fn can_survive(&self, _state: BlockStateId, _world: &dyn LevelReader, _pos: BlockPos) -> bool {
        true
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let inventory = player.inventory.clone();
        let world = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_GRINDSTONE_TITLE.msg()),
            move |context| grindstone(inventory, context.container_id, pos, &world),
        );
        InteractionResult::Success
    }

    fn use_item_on(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        _player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        InteractionResult::TryEmptyHandInteraction
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }
}
