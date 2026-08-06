//! ZombieVillager entity - vanilla `{java_class}`.

#![allow(missing_docs, reason = "generated entity boilerplate")]

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::vanilla_entity_data::ZombieVillagerEntityData;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    HurtByTargetGoal, LookAtPlayerGoal, NearestAttackableTargetGoal, RandomLookAroundGoal,
    TargetClass, WaterAvoidingRandomStrollGoal, ZombieAttackGoal,
};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::world::World;

#[entity_behavior(class = "ZombieVillager")]
pub struct ZombieVillagerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: steel_utils::locks::SyncMutex<ZombieVillagerEntityData>,
}

unsafe impl DowncastType for ZombieVillagerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/zombie_villager");
}

impl ZombieVillagerEntity {
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }
    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        // Vanilla `Zombie.registerGoals` / `addBehaviourGoals`. Zombie has no `FloatGoal`:
        // vanilla zombies sink and walk along the bottom.
        //
        // Not ported yet: `ZombieAttackTurtleEggGoal` (4), `SpearUseGoal` (2) and
        // `MoveThroughVillageGoal` (6).
        {
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(3, ZombieAttackGoal::new(1.0, false));
            goals.add_goal(7, WaterAvoidingRandomStrollGoal::new(1.0));
            goals.add_goal(8, LookAtPlayerGoal::new(8.0));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }
        // Vanilla's turtle target goal (5) needs `Turtle.BABY_ON_LAND_SELECTOR`, and
        // `HurtByTargetGoal.setAlertOthers` is not modeled.
        {
            let mut targets = mob_base.target_selector().lock();
            targets.add_goal(1, HurtByTargetGoal::new());
            targets.add_goal(
                2,
                NearestAttackableTargetGoal::new(TargetClass::Player, true),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new(TargetClass::ABSTRACT_VILLAGER, false),
            );
            targets.add_goal(
                3,
                NearestAttackableTargetGoal::new(TargetClass::IRON_GOLEM, true),
            );
        }
        let mut random = steel_utils::random::legacy_random::LegacyRandom::from_seed(0);
        let mut entity_data = ZombieVillagerEntityData::new(&mut random);
        living_base.initialize_synced_data(&mut entity_data);
        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: steel_utils::locks::SyncMutex::new(entity_data),
        }
    }
}

impl Entity for ZombieVillagerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }
    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }
    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        self.entity_type
            .dimensions
            .scale(LivingEntity::get_scale(self))
    }
    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for ZombieVillagerEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }
    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }
    fn set_health(&self, health: f32) {
        let max = self.get_max_health();
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(health.clamp(0.0, max));
    }
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }
    fn ai_step(&self) -> Option<crate::entity::MoveResult> {
        self.default_ai_step()
    }
}

impl Mob for ZombieVillagerEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }
    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }
    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }
    fn finalize_spawn(
        &self,
        world: &std::sync::Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        self.finalize_spawn_mob_base(world, spawn_reason, group_data)
    }
    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }
    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for ZombieVillagerEntity {}
