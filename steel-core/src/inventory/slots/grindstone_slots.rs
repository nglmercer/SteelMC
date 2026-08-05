//! Result slot handler for the grindstone.

use std::sync::Arc;

use glam::DVec3;
use rand::RngExt as _;
use steel_registry::{
    REGISTRY, RegistryExt as _, TaggedRegistryExt as _, item_stack::ItemStack, level_events,
    vanilla_enchantment_tags::EnchantmentTag,
};
use steel_utils::{BlockPos, Identifier, locks::Shared};

use crate::{
    entity::entities::ExperienceOrbEntity,
    inventory::{
        container::{ResultContainer, SimpleContainer},
        lock::{ContainerId, ContainerLockGuard, ContainerRef},
        slots::ResultHandler,
    },
    player::Player,
    world::World,
};

/// Returns whether `enchantment` is a curse, which a grindstone cannot strip.
#[must_use]
pub fn is_curse(enchantment: &Identifier) -> bool {
    REGISTRY
        .enchantments
        .by_key(enchantment)
        .is_some_and(|entry| {
            REGISTRY
                .enchantments
                .is_in_tag(entry, &EnchantmentTag::CURSE)
        })
}

/// Result slot handler for a grindstone.
///
/// Taking the result awards experience for the enchantments that were ground off and
/// empties both input slots.
#[derive(Clone)]
pub struct GrindstoneResultHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    block_pos: BlockPos,
    world: Arc<World>,
}

impl GrindstoneResultHandler {
    /// Creates a new handler.
    #[must_use]
    pub const fn new(
        input_container: Shared<SimpleContainer>,
        result_container: Shared<ResultContainer>,
        block_pos: BlockPos,
        world: Arc<World>,
    ) -> Self {
        Self {
            input_container,
            result_container,
            block_pos,
            world,
        }
    }

    /// Vanilla `GrindstoneMenu.getExperienceFromItem`: the summed minimum enchanting cost
    /// of every non-curse enchantment on `item`.
    fn experience_from_item(item: &ItemStack) -> i32 {
        let Some(enchantments) = item.get_enchantments_for_crafting() else {
            return 0;
        };

        enchantments
            .iter()
            .filter(|(key, _)| !is_curse(key))
            .filter_map(|(key, level)| {
                let entry = REGISTRY.enchantments.by_key(key)?;
                let level = i32::try_from(*level).unwrap_or(i32::MAX);
                Some(
                    entry
                        .min_cost
                        .per_level_above_first
                        .saturating_mul(level - 1)
                        .saturating_add(entry.min_cost.base),
                )
            })
            .sum()
    }

    /// Vanilla `GrindstoneMenu.getExperienceAmount`: half the total, plus a random bonus.
    fn experience_amount(first: &ItemStack, second: &ItemStack) -> i32 {
        let total =
            Self::experience_from_item(first).saturating_add(Self::experience_from_item(second));
        if total <= 0 {
            return 0;
        }

        // Vanilla rounds the half up, then adds `random(half)` on top.
        let half = (total + 1) / 2;
        half + rand::rng().random_range(0..half)
    }
}

impl ResultHandler for GrindstoneResultHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, _guard: &mut ContainerLockGuard) {}

    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) -> Option<ItemStack> {
        let input_id = ContainerId::from_arc(&self.input_container);
        let input = guard.get_mut(input_id).expect("input container not locked");

        let experience = {
            let [first, second] = input.items() else {
                panic!("input_container in grindstone menu does not fit expected shape")
            };
            Self::experience_amount(first, second)
        };

        input.set_item(0, ItemStack::empty());
        input.set_item(1, ItemStack::empty());
        input.set_changed();

        if experience > 0 {
            ExperienceOrbEntity::award(
                &self.world,
                DVec3::new(
                    f64::from(self.block_pos.x()) + 0.5,
                    f64::from(self.block_pos.y()) + 0.5,
                    f64::from(self.block_pos.z()) + 0.5,
                ),
                experience,
            );
        }
        self.world
            .level_event(level_events::SOUND_GRINDSTONE_USED, self.block_pos, 0, None);

        guard
            .get_mut(ContainerId::from_arc(&self.result_container))
            .expect("container not locked")
            .set_changed();
        None
    }

    fn is_result_valid(&self, _guard: &ContainerLockGuard, _player: &Player) -> bool {
        true
    }
}
