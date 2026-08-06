use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::data_components::vanilla_components::CHARGED_PROJECTILES;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_events;
use steel_registry::vanilla_items;

use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};
use crate::entity::Entity;
use crate::inventory::container::Container;
use crate::player::Player;
use crate::world::World;

/// Vanilla `CrossbowItem` — charging and shooting.
#[item_behavior]
pub struct CrossbowItem;

impl CrossbowItem {
    const DEFAULT_CHARGE_TICKS: i32 = 25;

    fn is_charged(stack: &ItemStack) -> bool {
        stack
            .get(CHARGED_PROJECTILES)
            .is_some_and(|c| !c.items().is_empty())
    }

    fn has_projectile(player: &Player) -> bool {
        if player.has_infinite_materials() {
            return true;
        }
        let inv = player.inventory.lock();
        for slot in 0..inv.get_container_size() {
            let stack = inv.get_item(slot);
            if stack.is(&vanilla_items::ARROW)
                || stack.is(&vanilla_items::SPECTRAL_ARROW)
                || stack.is(&vanilla_items::TIPPED_ARROW)
                || stack.is(&vanilla_items::FIREWORK_ROCKET)
            {
                return true;
            }
        }
        false
    }
}

impl ItemBehavior for CrossbowItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let stack = context.inv.with_item(|item| item.clone());
        if Self::is_charged(&stack) {
            let has_infinite = context.player.has_infinite_materials();
            let hand = context.hand;
            context.inv.with_inventory(|inv| {
                inv.mutate_item_in_hand(hand, |item| {
                    item.hurt_and_break(1, has_infinite);
                    if item.has(CHARGED_PROJECTILES) {
                        item.remove(CHARGED_PROJECTILES);
                    }
                });
            });
            context.world.play_sound_at(
                &sound_events::ITEM_CROSSBOW_SHOOT,
                context.player.sound_source(),
                context.player.position(),
                1.0,
                1.0,
                None,
            );
            return InteractionResult::Consume;
        }
        if !Self::has_projectile(context.player) {
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

    fn on_use_tick(
        &self,
        world: &Arc<World>,
        player: &Player,
        stack: &ItemStack,
        ticks_remaining: i32,
    ) {
        let duration = self.use_duration(stack, player);
        let time_held = duration - ticks_remaining;
        let percent = time_held as f32 / Self::DEFAULT_CHARGE_TICKS as f32;
        if (0.2..0.25).contains(&percent) {
            world.play_sound_at(
                &sound_events::ITEM_CROSSBOW_LOADING_START,
                player.sound_source(),
                player.position(),
                0.5,
                1.0,
                None,
            );
        } else if (0.5..0.55).contains(&percent) {
            world.play_sound_at(
                &sound_events::ITEM_CROSSBOW_LOADING_MIDDLE,
                player.sound_source(),
                player.position(),
                0.5,
                1.0,
                None,
            );
        } else if percent >= 1.0 && !Self::is_charged(stack) {
            world.play_sound_at(
                &sound_events::ITEM_CROSSBOW_LOADING_END,
                player.sound_source(),
                player.position(),
                1.0,
                1.0,
                None,
            );
        }
    }

    fn release_using(
        &self,
        stack: &ItemStack,
        _world: &Arc<World>,
        player: &Player,
        ticks_remaining: i32,
    ) -> bool {
        let duration = self.use_duration(stack, player);
        let time_held = duration - ticks_remaining;
        let pow = time_held as f32 / Self::DEFAULT_CHARGE_TICKS as f32;
        if Self::is_charged(stack) {
            return true;
        }
        pow >= 1.0 && Self::has_projectile(player)
    }
}
