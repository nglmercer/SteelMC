//! WanderingTrader entity — vanilla `WanderingTrader` (extends `AbstractVillager`).

#![allow(missing_docs, reason = "generated entity boilerplate")]

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::vanilla_entity_data::WanderingTraderEntityData;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{FloatGoal, LookAtPlayerGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal};
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::villager::MerchantOffers;
use crate::world::World;

#[entity_behavior(class = "WanderingTrader")]
pub struct WanderingTraderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: steel_utils::locks::SyncMutex<WanderingTraderEntityData>,
    offers: steel_utils::locks::SyncMutex<Option<MerchantOffers>>,
    despawn_delay: steel_utils::locks::SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `WanderingTraderEntity`.
unsafe impl DowncastType for WanderingTraderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/wandering_trader");
}

impl WanderingTraderEntity {
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
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(2, WaterAvoidingRandomStrollGoal::new(0.5));
            goals.add_goal(8, LookAtPlayerGoal::new(8.0));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }
        let mut entity_data = WanderingTraderEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: steel_utils::locks::SyncMutex::new(entity_data),
            offers: steel_utils::locks::SyncMutex::new(None),
            despawn_delay: steel_utils::locks::SyncMutex::new(48000),
        }
    }

    pub fn despawn_delay(&self) -> i32 {
        *self.despawn_delay.lock()
    }
    pub fn set_despawn_delay(&self, v: i32) {
        *self.despawn_delay.lock() = v;
    }

    pub fn ensure_offers(&self) -> MerchantOffers {
        let mut guard = self.offers.lock();
        if guard.is_none() {
            // Wandering trader has 5 generic + 1 rare offer in vanilla; stub with 5
            let mut offers = MerchantOffers::new();
            for i in 0..5 {
                let sell_item: steel_registry::item_stack::ItemStack = match i % 3 {
                    0 => steel_registry::item_stack::ItemStack::with_count(&steel_registry::vanilla_items::NAUTILUS_SHELL, 1),
                    1 => steel_registry::item_stack::ItemStack::with_count(&steel_registry::vanilla_items::POTION, 1),
                    _ => steel_registry::item_stack::ItemStack::with_count(&steel_registry::vanilla_items::GLOWSTONE, 1),
                };
                let buy = steel_registry::item_stack::ItemStack::with_count(&steel_registry::vanilla_items::EMERALD, 1 + i as i32);
                offers.push(crate::villager::MerchantOffer::new(buy, None, sell_item, 12, 1, 0.05));
            }
            *guard = Some(offers);
        }
        guard.clone().unwrap()
    }

    pub fn offers(&self) -> Option<MerchantOffers> {
        self.offers.lock().clone()
    }
}

impl Entity for WanderingTraderEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }
    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }
    fn base_tick(&self) {
        Mob::base_tick_mob(self);
        let mut delay = self.despawn_delay.lock();
        if *delay > 0 {
            *delay -= 1;
            if *delay == 0 {
                self.base.set_removed(crate::entity::RemovalReason::Discarded);
            }
        }
    }
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        self.entity_type.dimensions.scale(LivingEntity::get_scale(self))
    }
    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("DespawnDelay", self.despawn_delay());
    }
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(v) = nbt.int("DespawnDelay") {
            self.set_despawn_delay(v);
        }
    }
}

impl LivingEntity for WanderingTraderEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }
    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }
    fn set_health(&self, health: f32) {
        let max = self.get_max_health();
        self.entity_data.lock().living_entity_mut().health.set(health.clamp(0.0, max));
    }
    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }
    fn ai_step(&self) -> Option<crate::entity::MoveResult> {
        self.default_ai_step()
    }
}

impl Mob for WanderingTraderEntity {
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

impl PathfinderMob for WanderingTraderEntity {}
