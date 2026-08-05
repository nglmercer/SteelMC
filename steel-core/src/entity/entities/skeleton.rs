//! Skeleton entity - vanilla parity.

#![allow(missing_docs, reason = "generated entity boilerplate")]

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::vanilla_entity_data::SkeletonEntityData;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    AvoidEntityGoal, FleeSunGoal, FloatGoal, LookAtPlayerGoal, RandomLookAroundGoal, RestrictSunGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::world::World;

#[entity_behavior(class = "Skeleton")]
pub struct SkeletonEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: steel_utils::locks::SyncMutex<SkeletonEntityData>,
}

unsafe impl DowncastType for SkeletonEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/skeleton");
}

impl SkeletonEntity {
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(EntityBase::new(id, position, entity_type.dimensions, world), entity_type)
    }
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(EntityBase::from_load(load, entity_type.dimensions), entity_type)
    }
    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        {
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(1, FloatGoal::new(&mob_base));
            goals.add_goal(2, RestrictSunGoal::new());
            goals.add_goal(3, FleeSunGoal::new(1.0));
            goals.add_goal(3, AvoidEntityGoal::new(6.0, 1.0, 1.2));
            goals.add_goal(5, WaterAvoidingRandomStrollGoal::new(1.0));
            goals.add_goal(6, LookAtPlayerGoal::new(8.0));
            goals.add_goal(6, RandomLookAroundGoal::new());
        }
        {
            let _targets = mob_base.target_selector().lock();
        }
        let mut entity_data = SkeletonEntityData::new();
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

impl Entity for SkeletonEntity {
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
        self.entity_type.dimensions.scale(LivingEntity::get_scale(self))
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

impl LivingEntity for SkeletonEntity {
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

impl Mob for SkeletonEntity {
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

impl PathfinderMob for SkeletonEntity {}
