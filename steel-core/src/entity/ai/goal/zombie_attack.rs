//! Vanilla `ZombieAttackGoal`.

use super::melee_attack::MeleeAttackGoal;
use super::selector::{Goal, GoalControls};
use crate::entity::PathfinderMob;

/// Vanilla raises the zombie's arms once it has been chasing for this many ticks.
const RAISE_ARM_DELAY_TICKS: i32 = 5;

/// Vanilla `ZombieAttackGoal`.
///
/// A melee attack that also drives the zombie's raised-arm pose: the arms come up once
/// the chase has lasted a moment and the next swing is less than half an attack interval
/// away.
pub(crate) struct ZombieAttackGoal {
    melee: MeleeAttackGoal,
    raise_arm_ticks: i32,
}

impl ZombieAttackGoal {
    #[must_use]
    pub(crate) const fn new(speed_modifier: f64, following_target_even_if_not_seen: bool) -> Self {
        Self {
            melee: MeleeAttackGoal::new(speed_modifier, following_target_even_if_not_seen),
            raise_arm_ticks: 0,
        }
    }
}

impl Goal for ZombieAttackGoal {
    fn controls(&self) -> GoalControls {
        self.melee.controls()
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.melee.can_use(mob)
    }

    fn can_continue_to_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        self.melee.can_continue_to_use(mob)
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        self.melee.start(mob);
        self.raise_arm_ticks = 0;
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        self.melee.stop(mob);
        mob.set_aggressive(false);
    }

    fn requires_update_every_tick(&self) -> bool {
        self.melee.requires_update_every_tick()
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        self.melee.tick(mob);
        self.raise_arm_ticks += 1;

        let arms_up = self.raise_arm_ticks >= RAISE_ARM_DELAY_TICKS
            && self.melee.get_ticks_until_next_attack() < MeleeAttackGoal::attack_interval() / 2;
        mob.set_aggressive(arms_up);
    }
}
