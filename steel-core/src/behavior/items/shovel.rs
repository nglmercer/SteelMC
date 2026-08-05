use steel_macros::item_behavior;
use steel_registry::{
    blocks::{
        Block,
        block_state_ext::BlockStateExt,
        properties::{BlockStateProperties, BoolProperty},
    },
    level_events,
    vanilla_block_tags::BlockTag,
    vanilla_blocks, vanilla_game_events,
};
use steel_utils::{Direction, Downcast};
use steel_utils::types::UpdateFlags;

use crate::{
    behavior::{InteractionResult, ItemBehavior, UseOnContext},
    entity::Entity as _,
    world::game_event::GameEventContext,
};

const FLATTENABLES: [&Block; 6] = [
    &vanilla_blocks::GRASS_BLOCK,
    &vanilla_blocks::DIRT,
    &vanilla_blocks::PODZOL,
    &vanilla_blocks::COARSE_DIRT,
    &vanilla_blocks::MYCELIUM,
    &vanilla_blocks::ROOTED_DIRT,
];

const LIT_PROPERTY: BoolProperty = BlockStateProperties::LIT;

/// Behavior for Shovels, extinguishes campfires and turns grass blocks into paths
#[item_behavior]
pub struct ShovelItem;

impl ItemBehavior for ShovelItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        if context.hit_result.direction == Direction::Down {
            return InteractionResult::Pass;
        }

        let block_state = context.world.get_block_state(context.hit_result.block_pos);
        let block = block_state.get_block();

        // Flattenables — vanilla checks these first
        if FLATTENABLES.contains(&block) {
            if !context
                .world
                .get_block_state(context.hit_result.block_pos.above())
                .is_air()
            {
                return InteractionResult::Pass;
            }
            context.world.play_block_sound(
                &steel_registry::sound_events::ITEM_SHOVEL_FLATTEN,
                context.hit_result.block_pos,
                1.0,
                1.0,
                Some(context.player.id()),
            );
            let infinite_materials = context.player.has_infinite_materials();
            context
                .inv
                .with_item(|item| item.hurt_and_break(1, infinite_materials));
            let updated_state = vanilla_blocks::DIRT_PATH.default_state();
            context.world.set_block(
                context.hit_result.block_pos,
                updated_state,
                UpdateFlags::UPDATE_ALL_IMMEDIATE,
            );
            context.world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                context.hit_result.block_pos,
                &GameEventContext::new(Some(context.player), Some(updated_state)),
            );
            return InteractionResult::Success;
        }

        // Campfire extinguishing
        if block.has_tag(&BlockTag::CAMPFIRES) {
            if !block_state.get_value(&LIT_PROPERTY) {
                return InteractionResult::Pass;
            }
            context.world.level_event(
                level_events::SOUND_EXTINGUISH_FIRE,
                context.hit_result.block_pos,
                0,
                None,
            );
            // Vanilla `CampfireBlock.dowse` — extinguish and eject cooking items.
            if let Some(be) = context.world.get_block_entity(context.hit_result.block_pos) {
                if let Some(campfire) = be.downcast_ref::<crate::block_entity::entities::CampfireBlockEntity>() {
                    // `pre_remove_side_effects` drops items; for dowse we replicate drop without removing BE.
                    let items = campfire.take_items_for_dowse();
                    for item in items {
                        if !item.is_empty() {
                            context.world.drop_item_stack(context.hit_result.block_pos, item);
                        }
                    }
                    campfire.clear_cooking_state();
                }
            }
            let updated_state = block_state.set_value(&LIT_PROPERTY, false);
            context.world.set_block(
                context.hit_result.block_pos,
                updated_state,
                UpdateFlags::UPDATE_ALL_IMMEDIATE,
            );
            let infinite = context.player.has_infinite_materials();
            context.inv.with_item(|item| item.hurt_and_break(1, infinite));
            context.world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                context.hit_result.block_pos,
                &GameEventContext::new(Some(context.player), Some(updated_state)),
            );
            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }
}
