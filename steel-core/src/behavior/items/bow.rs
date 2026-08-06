use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_events;
use steel_registry::vanilla_items;

use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};
use crate::entity::Entity;
use crate::inventory::container::Container;
use crate::player::Player;
use crate::world::World;

/// Vanilla `BowItem` — draws arrows and fires on release.
///
/// Mirrors `BowItem.use` / `releaseUsing` / `getPowerForTime`.
#[item_behavior]
pub struct BowItem;

impl BowItem {
    fn power_for_time(time_held: i32) -> f32 {
        let mut pow = time_held as f32 / 20.0;
        pow = (pow * pow + pow * 2.0) / 3.0;
        if pow > 1.0 {
            pow = 1.0;
        }
        pow
    }

    fn find_arrow(player: &Player) -> bool {
        if player.has_infinite_materials() {
            return true;
        }
        let inv = player.inventory.lock();
        for slot in 0..inv.get_container_size() {
            let stack = inv.get_item(slot);
            if stack.is(&vanilla_items::ARROW)
                || stack.is(&vanilla_items::SPECTRAL_ARROW)
                || stack.is(&vanilla_items::TIPPED_ARROW)
            {
                return true;
            }
        }
        false
    }

    fn consume_arrow(player: &Player) {
        if player.has_infinite_materials() {
            return;
        }
        let mut inv = player.inventory.lock();
        for slot in 0..inv.get_container_size() {
            let stack = inv.get_item(slot).clone();
            if stack.is(&vanilla_items::ARROW)
                || stack.is(&vanilla_items::SPECTRAL_ARROW)
                || stack.is(&vanilla_items::TIPPED_ARROW)
            {
                let mut new_stack = stack;
                new_stack.count -= 1;
                if new_stack.count <= 0 {
                    inv.set_item(slot, ItemStack::empty());
                } else {
                    inv.set_item(slot, new_stack);
                }
                break;
            }
        }
    }
}

impl ItemBehavior for BowItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        if !Self::find_arrow(context.player) {
            return InteractionResult::Fail;
        }
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
        let pow = Self::power_for_time(time_held);
        if pow < 0.1 {
            return false;
        }
        if !Self::find_arrow(player) {
            return false;
        }
        world.play_sound_at(
            &sound_events::ENTITY_ARROW_SHOOT,
            player.sound_source(),
            player.position(),
            1.0,
            1.0,
            None,
        );
        let has_infinite = player.has_infinite_materials();
        let hand = player.get_used_item_hand();
        player.inventory.lock().mutate_item_in_hand(hand, |item| {
            item.hurt_and_break(1, has_infinite);
        });
        Self::consume_arrow(player);
        true
    }
}
