use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_events;

use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};
use crate::entity::Entity;
use crate::player::Player;
use crate::world::World;

/// Vanilla `TridentItem` — throw and simplified riptide.
#[item_behavior]
pub struct TridentItem;

impl TridentItem {
    const THROW_THRESHOLD_TIME: i32 = 10;
}

impl ItemBehavior for TridentItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        if context.inv.with_item(|item| item.next_damage_will_break()) {
            return InteractionResult::Fail;
        }
        context.player.start_using_item(context.hand);
        InteractionResult::Consume
    }

    fn use_duration(&self, _stack: &ItemStack, _user: &Player) -> i32 {
        72000
    }

    fn use_on_release(&self, _stack: &ItemStack) -> bool {
        true
    }

    fn release_using(
        &self,
        stack: &ItemStack,
        world: &Arc<World>,
        player: &Player,
        ticks_remaining: i32,
    ) -> bool {
        let duration = self.use_duration(stack, player);
        let time_held = duration - ticks_remaining;
        if time_held < Self::THROW_THRESHOLD_TIME {
            return false;
        }
        if stack.next_damage_will_break() {
            return false;
        }
        let has_infinite = player.has_infinite_materials();
        let hand = player.get_used_item_hand();
        player.inventory.lock().mutate_item_in_hand(hand, |item| {
            item.hurt_and_break(1, has_infinite);
        });

        world.play_sound_at(
            &sound_events::ITEM_TRIDENT_THROW,
            player.sound_source(),
            player.position(),
            1.0,
            1.0,
            None,
        );
        true
    }
}
