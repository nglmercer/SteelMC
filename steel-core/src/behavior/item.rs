//! Item behavior trait and registry.

use std::borrow::Cow;
use std::sync::Arc;

use steel_registry::data_components::vanilla_components::{
    BLOCKS_ATTACKS, CONSUMABLE, ITEM_NAME, KINETIC_WEAPON,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::{REGISTRY, RegistryEntry, RegistryExt};
use steel_utils::types::InteractionHand;
use text_components::TextComponent;

use crate::behavior::items::DefaultItemBehavior;
use crate::behavior::{InteractionResult, UseItemContext, UseOnContext};
use crate::entity::damage::DamageSource;
use crate::entity::{Entity, LivingEntity};
use crate::player::{Player, player_inventory::EquipmentSwapResult};
use crate::world::World;

/// Ticks in one second, used to convert consumable seconds into use ticks.
const TICKS_PER_SECOND: f32 = 20.0;

/// Vanilla's "infinite" use duration for blocking and kinetic-weapon items.
const INFINITE_USE_DURATION: i32 = 72000;

/// Trait defining the behavior of an item.
///
/// This trait handles dynamic/functional aspects of items:
/// - Use on blocks (placing, interacting)
/// - Use in air
/// - etc.
pub trait ItemBehavior: Send + Sync {
    /// Returns the Rust type name of the concrete behavior implementation.
    #[cfg(feature = "flint")]
    #[must_use]
    #[expect(clippy::absolute_paths, reason = "easier for features")]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }

    /// Returns vanilla `Item.getName(stack)`.
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        stack
            .get(ITEM_NAME)
            .map_or_else(|| Cow::Owned(TextComponent::new()), Cow::Borrowed)
    }

    /// Called when this item is used on a block.
    fn use_on(&self, _context: &mut UseOnContext) -> InteractionResult {
        InteractionResult::Pass
    }

    /// Called when this item is used (e.g. right click in air).
    ///
    /// Mirrors vanilla `Item.use`: consumable → equippable swap → blocking →
    /// kinetic weapon before delegating to the fallback.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        // Consumable takes precedence — delegated to specialized item behaviors
        // or handled via the CONSUMABLE component elsewhere; fall through to
        // allow those behaviors to call `startUsingItem` themselves.
        let has_consumable = context.inv.with_item(|item| item.has(CONSUMABLE));
        if has_consumable {
            // Vanilla `Consumable.startConsuming` is invoked from `Item.use`;
            // specialized consumable items override this method. For the base
            // implementation we simply start using and let the tick handle it.
            context.player.start_using_item(context.hand);
            return InteractionResult::Consume;
        }

        // Vanilla order: BLOCKS_ATTACKS and KINETIC_WEAPON are checked before
        // equippable swap. See `Item.use` in vanilla 26.2.
        let has_blocks_attacks = context.inv.with_item(|item| item.has(BLOCKS_ATTACKS));
        if has_blocks_attacks {
            context.player.start_using_item(context.hand);
            return InteractionResult::Consume;
        }

        if let Some(kinetic) = context.inv.with_item(|item| item.get(KINETIC_WEAPON).cloned()) {
            context.player.start_using_item(context.hand);
            if let Some(sound) = kinetic.sound().as_ref() {
                // Direct holders cannot be sent as registry sounds yet.
                if let steel_registry::sound_event::SoundEventHolder::Registry(s) = sound {
                    context.world.play_sound_at(
                        s,
                        context.player.sound_source(),
                        context.player.position(),
                        1.0,
                        1.0,
                        None,
                    );
                }
            }
            return InteractionResult::Consume;
        }

        let Some(equippable) = context.inv.with_item(|item| item.get_equippable().cloned()) else {
            return InteractionResult::Pass;
        };

        if !equippable.swappable || !equippable.can_be_equipped_by(context.player.entity_type()) {
            return InteractionResult::Pass;
        }

        let slot = equippable.slot;
        let result = context.inv.with_inventory(|inventory| {
            inventory.try_swap_with_equipment_slot(
                context.hand,
                slot,
                context.player.has_infinite_materials(),
            )
        });

        match result {
            EquipmentSwapResult::Success(overflow) => {
                if !overflow.is_empty() {
                    let _ = context.player.drop_item(overflow, false, false);
                }
                InteractionResult::Success
            }
            EquipmentSwapResult::Fail => InteractionResult::Fail,
        }
    }

    /// Called by vanilla `ItemStack.interactLivingEntity`.
    fn interact_living_entity(
        &self,
        _stack: &mut ItemStack,
        _player: &Player,
        _target: &dyn LivingEntity,
        _hand: InteractionHand,
    ) -> InteractionResult {
        InteractionResult::Pass
    }

    /// Returns vanilla `Item.getItemDamageSource`.
    fn get_item_damage_source(&self, _attacker: &dyn LivingEntity) -> Option<DamageSource> {
        None
    }

    /// Returns item-specific attack damage added by `Item.getAttackDamageBonus`.
    fn get_attack_damage_bonus(
        &self,
        _attacker: &dyn LivingEntity,
        _victim: &dyn Entity,
        _base_damage: f32,
        _damage_source: &DamageSource,
    ) -> f32 {
        0.0
    }

    /// Called by vanilla `Item.hurtEnemy`.
    fn hurt_enemy(
        &self,
        _stack: &mut ItemStack,
        _target: &dyn LivingEntity,
        _attacker: &dyn LivingEntity,
    ) {
    }

    /// Called by vanilla `Item.postHurtEnemy`.
    fn post_hurt_enemy(
        &self,
        _stack: &mut ItemStack,
        _target: &dyn LivingEntity,
        _attacker: &dyn LivingEntity,
    ) {
    }

    /// Returns how much durability this weapon consumes after a successful entity hit.
    fn item_damage_per_attack(&self, stack: &ItemStack) -> Option<i32> {
        stack
            .get_weapon()
            .map(|weapon| weapon.item_damage_per_attack)
    }

    /// Returns vanilla `Item.getUseDuration`: how many ticks a full use of this item takes.
    ///
    /// The default mirrors vanilla's base implementation: consumables use their configured
    /// consume time, blocking and kinetic items use "forever", everything else cannot be
    /// held in use.
    fn use_duration(&self, stack: &ItemStack, _user: &Player) -> i32 {
        if let Some(consumable) = stack.get(CONSUMABLE) {
            // Vanilla: `Math.round(consumeSeconds * 20.0F)`.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "consume durations are a few seconds at most"
            )]
            let ticks = (consumable.consume_seconds() * TICKS_PER_SECOND).round() as i32;
            return ticks;
        }

        if stack.has(BLOCKS_ATTACKS) || stack.has(KINETIC_WEAPON) {
            return INFINITE_USE_DURATION;
        }

        0
    }

    /// Called by vanilla `Item.onUseTick` each tick while this item is held in use, before
    /// the remaining-use counter is decremented.
    fn on_use_tick(
        &self,
        world: &Arc<World>,
        player: &Player,
        stack: &ItemStack,
        _ticks_remaining: i32,
    ) {
        // Vanilla kinetic spear forward movement: applied each tick while the
        // spear is being used. `forward_movement` is defined on the component.
        if let Some(kinetic) = stack.get(KINETIC_WEAPON) {
            let forward = kinetic.forward_movement();
            if forward > 0.0 && player.is_using_item() {
                let look = player.look_angle();
                // Vanilla scales forward_movement per tick and applies as impulse;
                // Steel uses push_impulse for consistent motion.
                let impulse = look.normalize() * f64::from(forward) * 0.15;
                player.push_impulse(impulse);
                // Clamp fall to avoid excessive vertical drift.
                let vel = player.velocity();
                if vel.y < -0.5 {
                    player.set_velocity(glam::DVec3::new(vel.x, -0.5, vel.z));
                }
                let _ = world;
            }
        }
    }

    /// Returns vanilla `Item.useOnRelease`: whether the item finishes when the player
    /// releases the use button (bows, tridents) instead of when the duration runs out.
    fn use_on_release(&self, _stack: &ItemStack) -> bool {
        false
    }

    /// Called by vanilla `Item.releaseUsing` when the player releases the use button.
    fn release_using(
        &self,
        _stack: &ItemStack,
        _world: &Arc<World>,
        _player: &Player,
        _ticks_remaining: i32,
    ) -> bool {
        false
    }

    /// Called by vanilla `Item.finishUsingItem`; returns the stack that replaces the used
    /// item in hand.
    fn finish_using_item(
        &self,
        stack: ItemStack,
        _world: &Arc<World>,
        _player: &Player,
    ) -> ItemStack {
        stack
    }
}

/// Registry for item behaviors.
///
/// Created after the main registry is frozen. Block items get `BlockItemBehavior`,
/// other items get `DefaultItemBehavior`. Custom behaviors can be registered.
pub struct ItemBehaviorRegistry {
    behaviors: Vec<Box<dyn ItemBehavior>>,
}

impl ItemBehaviorRegistry {
    /// Creates a new behavior registry with default behaviors for all items.
    ///
    /// Call `register_item_behaviors()` after this to set up proper behaviors.
    #[must_use]
    pub fn new() -> Self {
        let item_count = REGISTRY.items.len();
        let behaviors = (0..item_count)
            .map(|_| Box::new(DefaultItemBehavior) as Box<dyn ItemBehavior>)
            .collect();

        Self { behaviors }
    }

    /// Sets a custom behavior for an item.
    pub fn set_behavior(&mut self, item: ItemRef, behavior: Box<dyn ItemBehavior>) {
        let id = item.id();
        self.behaviors[id] = behavior;
    }

    /// Gets the behavior for an item.
    #[must_use]
    pub fn get_behavior(&self, item: ItemRef) -> &dyn ItemBehavior {
        let id = item.id();
        self.behaviors[id].as_ref()
    }

    /// Returns vanilla `ItemStack.getHoverName`, including item-specific
    /// `Item.getName(stack)` overrides when no custom name is present.
    #[must_use]
    pub fn hover_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        stack
            .custom_name()
            .unwrap_or_else(|| self.get_behavior(stack.item()).get_name(stack))
    }

    /// Get all behaviors.
    #[cfg(feature = "flint")]
    #[must_use]
    pub fn get_behaviors(&self) -> &[Box<dyn ItemBehavior>] {
        &self.behaviors
    }
}

impl Default for ItemBehaviorRegistry {
    fn default() -> Self {
        Self::new()
    }
}
