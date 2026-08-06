use std::ptr;

use glam::DVec3;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_attributes;
use steel_registry::vanilla_entities;
use steel_registry::vanilla_game_rules::UNIVERSAL_ANGER;

use super::reduced_tick_delay;
use crate::entity::ai::targeting::TargetingConditions;
use crate::entity::{Entity, LivingEntity, Mob, PathfinderMob, SharedEntity};

const DEFAULT_UNSEEN_MEMORY_TICKS: i32 = 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReachCache {
    Empty,
    CanReach,
    CantReach,
}

pub(super) struct TargetGoalBase {
    must_see: bool,
    must_reach: bool,
    reach_cache: ReachCache,
    reach_cache_time: i32,
    unseen_ticks: i32,
    target_mob: Option<SharedEntity>,
    unseen_memory_ticks: i32,
}

impl TargetGoalBase {
    #[must_use]
    pub(super) const fn new(must_see: bool, must_reach: bool) -> Self {
        Self {
            must_see,
            must_reach,
            reach_cache: ReachCache::Empty,
            reach_cache_time: 0,
            unseen_ticks: 0,
            target_mob: None,
            unseen_memory_ticks: DEFAULT_UNSEEN_MEMORY_TICKS,
        }
    }

    pub(super) const fn set_unseen_memory_ticks(&mut self, unseen_memory_ticks: i32) {
        self.unseen_memory_ticks = unseen_memory_ticks;
    }

    pub(super) fn set_target_mob(&mut self, target_mob: Option<SharedEntity>) {
        self.target_mob = target_mob;
    }

    pub(super) fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(target) = mob.target().or_else(|| self.target_mob.clone()) else {
            return false;
        };
        let Some(target_living) = target.as_living_entity() else {
            return false;
        };

        if !Mob::can_attack(mob, target_living) || mob.is_allied_to(target_living) {
            return false;
        }

        let follow_distance = follow_distance(mob);
        if mob.position().distance_squared(target.position()) > follow_distance * follow_distance {
            return false;
        }

        if self.must_see && !self.update_unseen_ticks(mob, target_living) {
            return false;
        }

        mob.set_target(Some(&target))
    }

    pub(super) const fn start(&mut self) {
        self.reach_cache = ReachCache::Empty;
        self.reach_cache_time = 0;
        self.unseen_ticks = 0;
    }

    pub(super) fn stop(&mut self, mob: &dyn PathfinderMob) {
        mob.set_target(None);
        self.target_mob = None;
    }

    pub(super) fn can_attack(
        &mut self,
        mob: &dyn PathfinderMob,
        target: Option<&dyn LivingEntity>,
        target_conditions: &TargetingConditions,
    ) -> bool {
        let Some(target) = target else {
            return false;
        };
        let Some(world) = mob.level() else {
            return false;
        };

        if !target_conditions.test(world.as_ref(), Some(mob), target) {
            return false;
        }
        if !mob.is_within_home_pos(target.block_position()) {
            return false;
        }

        if self.must_reach && !self.can_reach(mob, target) {
            return false;
        }

        true
    }

    fn update_unseen_ticks(&mut self, mob: &dyn PathfinderMob, target: &dyn LivingEntity) -> bool {
        if mob.has_line_of_sight_cached(target) {
            self.unseen_ticks = 0;
            return true;
        }

        self.unseen_ticks += 1;
        self.unseen_ticks <= reduced_tick_delay(self.unseen_memory_ticks)
    }

    fn can_reach(&mut self, mob: &dyn PathfinderMob, target: &dyn LivingEntity) -> bool {
        self.reach_cache_time -= 1;
        if self.reach_cache_time <= 0 {
            self.reach_cache = ReachCache::Empty;
        }

        if self.reach_cache == ReachCache::Empty {
            self.reach_cache = if self.check_reach(mob, target) {
                ReachCache::CanReach
            } else {
                ReachCache::CantReach
            };
        }

        self.reach_cache == ReachCache::CanReach
    }

    fn check_reach(&mut self, mob: &dyn PathfinderMob, target: &dyn LivingEntity) -> bool {
        self.reach_cache_time = reduced_tick_delay(10 + rand::random_range(0..5));
        mob.can_reach_living_target(target)
    }
}

fn follow_distance(mob: &dyn PathfinderMob) -> f64 {
    mob.attributes()
        .lock()
        .required_value(vanilla_attributes::FOLLOW_RANGE)
}

/// Vanilla `HurtByTargetGoal`.
///
/// Vanilla's `setAlertOthers` variant (which wakes nearby mobs of the same class) is not
/// modeled yet; no mob in the currently ported goal sets uses it.
pub struct HurtByTargetGoal {
    base: TargetGoalBase,
    /// Vanilla `HurtByTargetGoal.timestamp`: the last damage event this goal reacted to.
    timestamp: i32,
}

/// Vanilla `HurtByTargetGoal.HURT_BY_TARGETING`.
const fn hurt_by_targeting() -> TargetingConditions {
    TargetingConditions::for_combat()
        .ignore_line_of_sight()
        .ignore_invisibility_testing()
}

/// Vanilla `HurtByTargetGoal.start` widens the unseen memory to 300 ticks.
const HURT_BY_UNSEEN_MEMORY_TICKS: i32 = 300;

impl HurtByTargetGoal {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            base: TargetGoalBase::new(true, false),
            timestamp: 0,
        }
    }
}

impl super::selector::Goal for HurtByTargetGoal {
    fn controls(&self) -> super::selector::GoalControls {
        super::selector::GoalControls::TARGET
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let timestamp = mob.last_hurt_by_mob_timestamp();
        let Some(attacker) = mob.last_hurt_by_mob() else {
            return false;
        };
        if timestamp == self.timestamp {
            return false;
        }

        // Vanilla defers player retaliation to the universal-anger system when it is on.
        if attacker.as_player().is_some()
            && mob
                .level()
                .is_some_and(|world| world.get_game_rule(&UNIVERSAL_ANGER))
        {
            return false;
        }

        let Some(attacker_living) = attacker.as_living_entity() else {
            return false;
        };
        self.base
            .can_attack(mob, Some(attacker_living), &hurt_by_targeting())
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        let attacker = mob.last_hurt_by_mob();
        if let Some(attacker) = &attacker {
            mob.set_target(Some(attacker));
        }
        self.base.set_target_mob(mob.target());
        self.timestamp = mob.last_hurt_by_mob_timestamp();
        self.base
            .set_unseen_memory_ticks(HURT_BY_UNSEEN_MEMORY_TICKS);
        self.base.start();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.base.stop(mob);
    }
}

/// Vanilla's `Class<T extends LivingEntity>` target filter.
///
/// Vanilla filters target candidates by Java class, which is a class-hierarchy check.
/// Steel names the same intent explicitly so the filter stays inspectable and so plugin
/// entity types can be added to a group without a downcast.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TargetClass {
    /// Vanilla `Player.class` / `ServerPlayer.class`.
    Player,
    /// Vanilla's concrete and abstract entity-class filters, expanded to the entity
    /// types the Java class covers.
    EntityTypes(&'static [EntityTypeRef]),
}

impl TargetClass {
    fn matches(self, candidate: &dyn Entity) -> bool {
        match self {
            Self::Player => candidate.as_player().is_some(),
            Self::EntityTypes(types) => types
                .iter()
                .any(|entity_type| ptr::eq(*entity_type, candidate.entity_type())),
        }
    }
}

/// Vanilla `AbstractVillager.class`, the trading-villager supertype.
static ABSTRACT_VILLAGER_TYPES: &[EntityTypeRef] = &[
    &vanilla_entities::VILLAGER,
    &vanilla_entities::WANDERING_TRADER,
];

/// Vanilla `IronGolem.class`.
static IRON_GOLEM_TYPES: &[EntityTypeRef] = &[&vanilla_entities::IRON_GOLEM];

impl TargetClass {
    /// Vanilla `AbstractVillager.class`.
    pub(crate) const ABSTRACT_VILLAGER: Self = Self::EntityTypes(ABSTRACT_VILLAGER_TYPES);

    /// Vanilla `IronGolem.class`.
    pub(crate) const IRON_GOLEM: Self = Self::EntityTypes(IRON_GOLEM_TYPES);
}

/// Vanilla `NearestAttackableTargetGoal.DEFAULT_RANDOM_INTERVAL`.
const DEFAULT_RANDOM_INTERVAL: i32 = 10;

/// Vanilla `NearestAttackableTargetGoal`.
pub struct NearestAttackableTargetGoal {
    base: TargetGoalBase,
    target_type: TargetClass,
    random_interval: i32,
    target: Option<SharedEntity>,
}

impl NearestAttackableTargetGoal {
    #[must_use]
    pub(crate) fn new(target_type: TargetClass, must_see: bool) -> Self {
        Self::with_reach(target_type, must_see, false)
    }

    #[must_use]
    pub(crate) fn with_reach(target_type: TargetClass, must_see: bool, must_reach: bool) -> Self {
        Self {
            base: TargetGoalBase::new(must_see, must_reach),
            target_type,
            random_interval: reduced_tick_delay(DEFAULT_RANDOM_INTERVAL),
            target: None,
        }
    }

    /// Runs vanilla `NearestAttackableTargetGoal.findTarget`.
    fn find_target(&mut self, mob: &dyn PathfinderMob) {
        self.target = None;
        let Some(world) = mob.level() else {
            return;
        };

        let follow_distance = follow_distance(mob);
        let conditions = TargetingConditions::for_combat().range(follow_distance);
        let search_area =
            mob.bounding_box()
                .inflate_xyz(follow_distance, follow_distance, follow_distance);
        // Vanilla measures candidate distance from the mob's eye height.
        let origin = DVec3::new(mob.position().x, mob.get_eye_y(), mob.position().z);
        let target_type = self.target_type;

        self.target = world.nearest_entity_in_aabb_matching(&search_area, origin, |candidate| {
            target_type.matches(candidate)
                && candidate
                    .as_living_entity()
                    .is_some_and(|living| conditions.test(world.as_ref(), Some(mob), living))
        });
    }
}

impl super::selector::Goal for NearestAttackableTargetGoal {
    fn controls(&self) -> super::selector::GoalControls {
        super::selector::GoalControls::TARGET
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        if self.random_interval > 0 && rand::random_range(0..self.random_interval) != 0 {
            return false;
        }

        self.find_target(mob);
        self.target.is_some()
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.base.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        if let Some(target) = &self.target {
            mob.set_target(Some(target));
        }
        self.base.set_target_mob(self.target.clone());
        self.base.start();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.target = None;
        self.base.stop(mob);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use glam::DVec3;
    use steel_registry::{test_support::init_test_registry, vanilla_entities};

    use super::*;
    use crate::entity::ai::targeting::TargetingConditions;
    use crate::entity::{Mob, entities::PigEntity};

    fn pig(id: i32, position: DVec3) -> Arc<PigEntity> {
        Arc::new(PigEntity::new(
            &vanilla_entities::PIG,
            id,
            position,
            Weak::new(),
        ))
    }

    fn target_living(target: &SharedEntity) -> &dyn LivingEntity {
        let Some(living) = target.as_living_entity() else {
            panic!("test target should be a living entity");
        };
        living
    }

    #[test]
    fn target_goal_base_continues_with_existing_mob_target() {
        init_test_registry();
        let mob = pig(1, DVec3::ZERO);
        let target: SharedEntity = pig(2, DVec3::new(2.0, 0.0, 0.0));
        assert!(mob.set_target(Some(&target)));
        let mut goal = TargetGoalBase::new(false, false);

        goal.start();

        assert!(goal.can_continue_to_use(mob.as_ref()));
        let Some(stored_target) = mob.target() else {
            panic!("target should remain set");
        };
        assert_eq!(stored_target.uuid(), target.uuid());
    }

    #[test]
    fn target_goal_base_restores_stored_target_while_continuing() {
        init_test_registry();
        let mob = pig(1, DVec3::ZERO);
        let target: SharedEntity = pig(2, DVec3::new(2.0, 0.0, 0.0));
        let mut goal = TargetGoalBase::new(false, false);
        goal.set_target_mob(Some(target.clone()));

        assert!(mob.target().is_none());
        assert!(goal.can_continue_to_use(mob.as_ref()));

        let Some(stored_target) = mob.target() else {
            panic!("stored target should be copied onto the mob");
        };
        assert_eq!(stored_target.uuid(), target.uuid());
    }

    #[test]
    fn target_goal_base_forgets_unseen_target_after_memory_ticks() {
        init_test_registry();
        let mob = pig(1, DVec3::ZERO);
        let target: SharedEntity = pig(2, DVec3::new(2.0, 0.0, 0.0));
        assert!(mob.set_target(Some(&target)));
        let mut goal = TargetGoalBase::new(true, false);
        goal.set_unseen_memory_ticks(2);
        goal.start();

        assert!(goal.can_continue_to_use(mob.as_ref()));
        assert!(!goal.can_continue_to_use(mob.as_ref()));
    }

    #[test]
    fn target_goal_base_stop_clears_mob_and_stored_target() {
        init_test_registry();
        let mob = pig(1, DVec3::ZERO);
        let target: SharedEntity = pig(2, DVec3::new(2.0, 0.0, 0.0));
        assert!(mob.set_target(Some(&target)));
        let mut goal = TargetGoalBase::new(false, false);
        goal.set_target_mob(Some(target));

        goal.stop(mob.as_ref());

        assert!(mob.target().is_none());
        assert!(goal.target_mob.is_none());
    }

    #[test]
    fn target_goal_base_can_attack_requires_world() {
        init_test_registry();
        let mob = pig(1, DVec3::ZERO);
        let target: SharedEntity = pig(2, DVec3::new(2.0, 0.0, 0.0));
        let mut goal = TargetGoalBase::new(false, false);
        let target_conditions = TargetingConditions::for_combat().ignore_line_of_sight();

        assert!(!goal.can_attack(
            mob.as_ref(),
            Some(target_living(&target)),
            &target_conditions
        ));
    }

    #[test]
    fn target_goal_base_caches_unreachable_targets() {
        init_test_registry();
        let mob = pig(1, DVec3::ZERO);
        let target: SharedEntity = pig(2, DVec3::new(2.0, 0.0, 0.0));
        let mut goal = TargetGoalBase::new(false, true);

        assert!(!goal.can_reach(mob.as_ref(), target_living(&target)));
        assert_eq!(goal.reach_cache, ReachCache::CantReach);
        let first_reach_cache_time = goal.reach_cache_time;

        assert!(!goal.can_reach(mob.as_ref(), target_living(&target)));
        assert_eq!(goal.reach_cache, ReachCache::CantReach);
        assert_eq!(goal.reach_cache_time, first_reach_cache_time - 1);
    }
}
