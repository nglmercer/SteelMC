//! Brush item behavior for excavating brushable blocks.

use std::sync::Arc;

use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_events;
use steel_utils::Downcast as _;

use crate::behavior::context::{InteractionResult, UseOnContext};
use crate::behavior::item::ItemBehavior;
use crate::behavior::{BLOCK_BEHAVIORS, Brushable};
use crate::block_entity::entities::BrushableBlockEntity;
use crate::player::Player;
use crate::world::{ClipBlockShape, ClipFluid, ClipHitResult, World};

/// Vanilla `BrushItem.USE_DURATION`.
const USE_DURATION: i32 = 200;
/// Vanilla `BrushItem.ANIMATION_DURATION`: one full brush stroke.
const ANIMATION_DURATION: i32 = 10;

/// Behavior for the brush item.
#[item_behavior]
pub struct BrushItem;

impl BrushItem {
    /// Vanilla `BrushItem.calculateHitResult`: the block hit on the player's view vector.
    fn calculate_hit_result(player: &Player, world: &Arc<World>) -> ClipHitResult {
        let (start, end) = player.get_ray_endpoints();
        world.clip(start, end, ClipBlockShape::Collider, ClipFluid::None)
    }
}

impl ItemBehavior for BrushItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        // Vanilla only begins brushing while actually targeting a block.
        if !Self::calculate_hit_result(context.player, context.world).is_miss() {
            context.player.start_using_item(context.hand);
        }

        InteractionResult::Consume
    }

    fn use_duration(&self, _stack: &ItemStack, _user: &Player) -> i32 {
        USE_DURATION
    }

    fn on_use_tick(
        &self,
        world: &Arc<World>,
        player: &Player,
        stack: &ItemStack,
        ticks_remaining: i32,
    ) {
        if ticks_remaining < 0 {
            player.release_using_item();
            return;
        }

        let hit = Self::calculate_hit_result(player, world);
        if hit.is_miss() {
            player.release_using_item();
            return;
        }

        // Vanilla brushes once per stroke, on the tick just before the backswing.
        let time_elapsed = USE_DURATION - ticks_remaining + 1;
        if time_elapsed % ANIMATION_DURATION != ANIMATION_DURATION / 2 {
            return;
        }

        let pos = hit.block_pos;
        let state = world.get_block_state(pos);
        let brush_sound = BLOCK_BEHAVIORS
            .get_behavior_for_state(state)
            .and_then(|behavior| behavior.as_brushable())
            .map_or(
                &sound_events::ITEM_BRUSH_BRUSHING_GENERIC,
                Brushable::brush_sound,
            );
        world.play_sound(brush_sound, SoundSource::Blocks, pos, 1.0, 1.0, None);

        // DEFERRED (Phase 4-8): Vanilla also spawns dust particles here (`BrushItem.spawnDustParticles`).
        let Some(block_entity) = world.get_block_entity(pos) else {
            return;
        };
        let Some(brushable) = block_entity.downcast_ref::<BrushableBlockEntity>() else {
            return;
        };

        let updated = brushable.brush(world.game_time(), world, player, hit.direction, stack);
        if updated {
            let hand = player.get_used_item_hand();
            let has_infinite_materials = player.has_infinite_materials();
            player
                .inventory
                .lock()
                .hurt_item_in_hand(hand, 1, has_infinite_materials);
        }
    }
}
