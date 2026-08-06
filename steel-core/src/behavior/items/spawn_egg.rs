//! Spawn egg item behavior (`SpawnEggItem`).
//!
//! The entity a spawn egg produces comes from its `ENTITY_DATA` component, which the
//! extractor already writes onto every vanilla spawn egg item.

use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::data_components::components::EntityData;
use steel_registry::data_components::vanilla_components::ENTITY_DATA;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::{blocks::properties::Direction, vanilla_blocks};
use steel_registry::{vanilla_game_events, vanilla_game_rules};
use steel_utils::types::Difficulty;
use steel_utils::{BlockPos, Downcast as _, translations};
use text_components::TextComponent;

use crate::behavior::context::{InteractionResult, InventoryAccess, UseItemContext, UseOnContext};
use crate::behavior::item::ItemBehavior;
use crate::behavior::{BLOCK_BEHAVIORS, BlockCollisionContext};
use crate::block_entity::entities::SpawnerBlockEntity;
use crate::entity::{EntitySpawnReason, EntityTypeSpawnExt as _};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{RaytraceAction, World};

/// Behavior for every `*_spawn_egg` item.
#[item_behavior(class = "SpawnEggItem")]
pub struct SpawnEggItem;

impl ItemBehavior for SpawnEggItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let Some(entity_type) = context.inv.with_item(|item| spawned_entity_type(item)) else {
            return InteractionResult::Fail;
        };
        if !can_spawn(entity_type, context.world) {
            return InteractionResult::Fail;
        }

        let clicked_pos = context.hit_result.block_pos;
        let clicked_face = context.hit_result.direction;

        // Vanilla reprograms a clicked spawner instead of spawning the mob.
        if let Some(block_entity) = context.world.get_block_entity(clicked_pos)
            && let Some(spawner) = block_entity.downcast_ref::<SpawnerBlockEntity>()
        {
            if !context
                .world
                .get_game_rule(&vanilla_game_rules::SPAWNER_BLOCKS_WORK)
            {
                context.player.send_message(&TextComponent::from(
                    &translations::ADV_MODE_NOT_ENABLED_SPAWNER,
                ));
                return InteractionResult::Fail;
            }

            spawner.set_entity_id(entity_type);
            context.world.send_block_updated(clicked_pos);
            context.world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                clicked_pos,
                &GameEventContext::new(Some(context.player), None),
            );
            context.inv.with_item(|item| item.shrink(1));
            return InteractionResult::Success;
        }

        let state = context.world.get_block_state(clicked_pos);
        let has_collision = !BLOCK_BEHAVIORS
            .get_behavior(state.get_block())
            .get_collision_boxes(
                state,
                context.world.as_ref(),
                clicked_pos,
                BlockCollisionContext::empty(),
            )
            .is_empty();

        let spawn_pos = if has_collision {
            clicked_pos.relative(clicked_face)
        } else {
            clicked_pos
        };

        spawn_mob(
            entity_type,
            context.player,
            &mut context.inv,
            context.world,
            spawn_pos,
            true,
            spawn_pos != clicked_pos && clicked_face == Direction::Up,
        )
    }

    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let Some(entity_type) = context.inv.with_item(|item| spawned_entity_type(item)) else {
            return InteractionResult::Fail;
        };
        if !can_spawn(entity_type, context.world) {
            return InteractionResult::Fail;
        }

        // Vanilla `getPlayerPOVHitResult(level, player, ClipContext.Fluid.SOURCE_ONLY)`.
        let (start, end) = context.player.get_ray_endpoints();
        let (hit_block, _) = context.world.raytrace(start, end, |pos, world| {
            let state = world.get_block_state(pos);
            if state.get_block() == &vanilla_blocks::AIR {
                return RaytraceAction::Pass;
            }

            let fluid_state = state.get_fluid_state();
            if fluid_state.is_source() {
                return RaytraceAction::ImmediateHit;
            }
            if !fluid_state.is_empty() {
                return RaytraceAction::Pass;
            }

            RaytraceAction::CheckShape
        });

        let Some(hit_pos) = hit_block else {
            return InteractionResult::Pass;
        };

        // Vanilla only spawns into a `LiquidBlock`; solid hits pass so the block's own
        // interaction can run.
        let hit_block = context.world.get_block_state(hit_pos).get_block();
        if !BLOCK_BEHAVIORS.get_behavior(hit_block).is_liquid_block() {
            return InteractionResult::Pass;
        }

        if !context.world.may_interact(context.player, hit_pos) {
            return InteractionResult::Fail;
        }

        let result = spawn_mob(
            entity_type,
            context.player,
            &mut context.inv,
            context.world,
            hit_pos,
            false,
            false,
        );
        if result == InteractionResult::Success {
            let used_item = context.inv.with_item(|item| item.item());
            context.player.award_item_used(used_item);
        }

        result
    }
}

/// Returns vanilla `SpawnEggItem.getType`.
fn spawned_entity_type(stack: &ItemStack) -> Option<EntityTypeRef> {
    stack.get(ENTITY_DATA).map(EntityData::entity_type)
}

/// Returns vanilla `EntityType.canSpawn(Level)`.
///
/// Vanilla also gates on the type's required feature flags, which Steel does not model.
fn can_spawn(entity_type: EntityTypeRef, world: &World) -> bool {
    entity_type.allowed_in_peaceful || world.difficulty() != Difficulty::Peaceful
}

/// Returns vanilla `SpawnEggItem.spawnMob`.
fn spawn_mob(
    entity_type: EntityTypeRef,
    player: &Player,
    inv: &mut InventoryAccess,
    world: &Arc<World>,
    spawn_pos: BlockPos,
    try_move_down: bool,
    moved_up: bool,
) -> InteractionResult {
    let stack = inv.with_item(|item| item.clone());
    if entity_type
        .spawn_at_block(
            world,
            Some(&stack),
            spawn_pos,
            EntitySpawnReason::SpawnItemUse,
            try_move_down,
            moved_up,
        )
        .is_none()
    {
        return InteractionResult::Fail;
    }

    // Vanilla `ItemStack.consume(1, user)`.
    if !player.has_infinite_materials() {
        inv.with_item(|item| item.shrink(1));
    }
    world.game_event(
        &vanilla_game_events::ENTITY_PLACE,
        spawn_pos,
        &GameEventContext::new(Some(player), None),
    );

    InteractionResult::Success
}
