//! Vanilla-shaped goal selector and movement goals.

mod avoid_entity;
mod breath_air;
mod breed_goal;
mod climb_on_top_of_powder_snow;
mod door_interact;
mod flee_sun;
mod float_goal;
mod follow_mob;
mod follow_parent;
mod interact;
mod leap_at_target;
mod look_at_player;
mod melee_attack;
mod move_to_block;
mod move_towards_restriction;
mod move_towards_target;
mod open_door;
mod panic_goal;
mod random_look_around;
mod random_pos;
mod random_stroll;

mod random_swimming;
mod restrict_sun;
pub(crate) mod selector;
mod target_goal;
mod tempt_goal;
mod try_find_water;
mod water_avoiding_random_stroll;

pub(crate) use avoid_entity::AvoidEntityGoal;
pub(crate) use selector::Goal;
pub(crate) use breed_goal::BreedGoal;
pub(crate) use breath_air::BreathAirGoal;
pub(crate) use climb_on_top_of_powder_snow::ClimbOnTopOfPowderSnowGoal;
pub(crate) use flee_sun::FleeSunGoal;
pub(crate) use float_goal::FloatGoal;
pub(crate) use restrict_sun::RestrictSunGoal;
pub(crate) use follow_mob::FollowMobGoal;
pub(crate) use follow_parent::FollowParentGoal;
pub(crate) use leap_at_target::LeapAtTargetGoal;
pub(crate) use look_at_player::LookAtPlayerGoal;
pub(crate) use melee_attack::MeleeAttackGoal;
pub(crate) use move_to_block::MoveToBlockGoal;
pub(crate) use move_towards_restriction::MoveTowardsRestrictionGoal;
pub(crate) use move_towards_target::MoveTowardsTargetGoal;
pub(crate) use open_door::OpenDoorGoal;
pub(crate) use panic_goal::PanicGoal;
pub(crate) use random_look_around::RandomLookAroundGoal;
pub(crate) use random_stroll::RandomStrollGoal;
pub(crate) use random_swimming::RandomSwimmingGoal;
pub(crate) use selector::{GoalControl, GoalSelector};
pub(crate) use target_goal::{HurtByTargetGoal, NearestAttackableTargetGoal};
pub(crate) use tempt_goal::TemptGoal;
pub(crate) use try_find_water::TryFindWaterGoal;
pub(crate) use water_avoiding_random_stroll::WaterAvoidingRandomStrollGoal;

pub(super) const fn reduced_tick_delay(ticks: i32) -> i32 {
    (ticks + 1) / 2
}
