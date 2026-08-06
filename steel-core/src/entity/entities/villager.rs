//! Villager entity — vanilla `net.minecraft.world.entity.npc.villager.Villager`.

#![allow(missing_docs, reason = "generated entity boilerplate")]

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::entity_type::{EntityDimensions, EntityTypeRef};
use steel_registry::vanilla_entity_data::VillagerEntityData;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FloatGoal, LookAtPlayerGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::{
    AgeableMob, AgeableMobBase, Entity, EntityBase, EntityBaseLoad, EntityPose, EntitySpawnReason,
    EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::villager::profession::{VillagerProfessionKind, can_level_up, xp_for_level};
use crate::villager::{GossipContainer, MerchantOffers};
use crate::world::World;

const VILLAGER_BABY_DIMENSIONS: EntityDimensions =
    EntityDimensions::new(0.6 * 0.49 / 0.6, 0.98, 0.49);

#[entity_behavior(class = "Villager")]
pub struct VillagerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    entity_data: steel_utils::locks::SyncMutex<VillagerEntityData>,
    // Non-synced runtime state (mirrors Villager.java fields)
    villager_xp: steel_utils::locks::SyncMutex<i32>,
    food_level: steel_utils::locks::SyncMutex<i32>,
    gossip: steel_utils::locks::SyncMutex<GossipContainer>,
    offers: steel_utils::locks::SyncMutex<Option<MerchantOffers>>,
    last_restock_game_time: steel_utils::locks::SyncMutex<i64>,
    number_of_restocks_today: steel_utils::locks::SyncMutex<i32>,
    last_gossip_decay_time: steel_utils::locks::SyncMutex<i64>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `VillagerEntity`.
unsafe impl DowncastType for VillagerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/villager");
}

impl VillagerEntity {
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
        let ageable_base = AgeableMobBase::new();
        {
            let mut goals = mob_base.goal_selector().lock();
            goals.add_goal(0, FloatGoal::new(&mob_base));
            goals.add_goal(2, WaterAvoidingRandomStrollGoal::new(0.5));
            goals.add_goal(8, LookAtPlayerGoal::new(8.0));
            goals.add_goal(8, RandomLookAroundGoal::new());
        }
        let mut entity_data = VillagerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            entity_data: steel_utils::locks::SyncMutex::new(entity_data),
            villager_xp: steel_utils::locks::SyncMutex::new(0),
            food_level: steel_utils::locks::SyncMutex::new(0),
            gossip: steel_utils::locks::SyncMutex::new(GossipContainer::new()),
            offers: steel_utils::locks::SyncMutex::new(None),
            last_restock_game_time: steel_utils::locks::SyncMutex::new(0),
            number_of_restocks_today: steel_utils::locks::SyncMutex::new(0),
            last_gossip_decay_time: steel_utils::locks::SyncMutex::new(0),
        }
    }

    // --- VillagerData accessors (type / profession / level) ---
    pub fn villager_data(&self) -> steel_registry::entity_data::VillagerData {
        *self.entity_data.lock().villager_data.get()
    }
    pub fn set_villager_data(&self, data: steel_registry::entity_data::VillagerData) {
        self.entity_data.lock().villager_data.set(data);
    }
    pub fn profession_kind(&self) -> Option<VillagerProfessionKind> {
        VillagerProfessionKind::from_id(self.villager_data().profession)
    }
    pub fn level(&self) -> i32 {
        self.villager_data().level
    }

    // --- XP / leveling ---
    pub fn villager_xp(&self) -> i32 {
        *self.villager_xp.lock()
    }
    pub fn set_villager_xp(&self, xp: i32) {
        *self.villager_xp.lock() = xp;
    }
    pub fn should_level_up(&self) -> bool {
        let lvl = self.level();
        lvl < 5 && self.villager_xp() >= xp_for_level(lvl + 1)
    }
    pub fn try_level_up(&self) {
        if !self.should_level_up() {
            return;
        }
        let kind = match self.profession_kind() {
            Some(k) if can_level_up(k) => k,
            _ => return,
        };
        let mut data = self.villager_data();
        data.level += 1;
        self.set_villager_data(data);
        // Generate new offers for next level (stub: append)
        let mut offers = self.offers.lock();
        let extra = MerchantOffers::generate_for_profession(kind.id(), data.level);
        if let Some(existing) = offers.as_mut() {
            existing.offers.extend(extra.offers);
        } else {
            *offers = Some(extra);
        }
        let _ = kind;
    }

    // --- Trading ---
    pub fn offers(&self) -> Option<MerchantOffers> {
        self.offers.lock().clone()
    }
    pub fn ensure_offers(&self) -> MerchantOffers {
        let mut guard = self.offers.lock();
        if guard.is_none() {
            let data = self.villager_data();
            *guard = Some(MerchantOffers::generate_for_profession(
                data.profession,
                data.level,
            ));
        }
        guard.clone().unwrap()
    }
    pub fn restock(&self) {
        if let Some(offers) = self.offers.lock().as_mut() {
            offers.update_all_demand();
            offers.reset_all_uses();
        }
        *self.number_of_restocks_today.lock() += 1;
    }
    pub fn notify_trade(&self, slot: usize) {
        if let Some(offers) = self.offers.lock().as_mut() {
            if let Some(o) = offers.offers.get_mut(slot) {
                o.increase_uses();
            }
        }
        // XP reward mirrors Villager.rewardTradeXp
        let xp_gain = 3 + (self.base.id() % 4) as i32;
        *self.villager_xp.lock() += xp_gain;
        if self.should_level_up() {
            self.try_level_up();
        }
    }

    // --- Gossip ---
    pub fn gossip_container(&self) -> GossipContainer {
        self.gossip.lock().clone()
    }
    pub fn reputation_for(&self, player: uuid::Uuid) -> i32 {
        self.gossip.lock().reputation(player)
    }

    // --- unhappy counter (AbstractVillager) ---
    pub fn unhappy_counter(&self) -> i32 {
        *self
            .entity_data
            .lock()
            .abstract_villager
            .unhappy_counter
            .get()
    }
    pub fn set_unhappy_counter(&self, v: i32) {
        self.entity_data
            .lock()
            .abstract_villager_mut()
            .unhappy_counter
            .set(v);
    }
}

impl Entity for VillagerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }
    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }
    fn base_tick(&self) {
        Mob::base_tick_mob(self);
        // Decay gossip every 24000 ticks (simplified: every tick check)
        let game_time = self.base.level().map(|w| w.game_time()).unwrap_or(0) as i64;
        let mut last = self.last_gossip_decay_time.lock();
        if game_time - *last >= 24000 {
            self.gossip.lock().decay();
            *last = game_time;
        }
        // Tick unhappy counter
        let unhappy = self.unhappy_counter();
        if unhappy > 0 {
            self.set_unhappy_counter(unhappy - 1);
        }
    }
    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            VILLAGER_BABY_DIMENSIONS.scale(scale)
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }
    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        let data = self.villager_data();
        let mut vd = NbtCompound::new();
        vd.insert("type", data.villager_type);
        vd.insert("profession", data.profession);
        vd.insert("level", data.level);
        nbt.insert("VillagerData", vd);
        nbt.insert("Xp", self.villager_xp());
        nbt.insert("FoodLevel", *self.food_level.lock() as i8);
        nbt.insert("RestocksToday", *self.number_of_restocks_today.lock());
        nbt.insert("LastRestock", *self.last_restock_game_time.lock());
    }
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        if let Some(vd) = nbt.compound("VillagerData") {
            let ty = vd.int("type").unwrap_or(2);
            let prof = vd.int("profession").unwrap_or(0);
            let lvl = vd.int("level").unwrap_or(1).clamp(1, 5);
            self.set_villager_data(steel_registry::entity_data::VillagerData::new(
                ty, prof, lvl,
            ));
        }
        if let Some(xp) = nbt.int("Xp") {
            *self.villager_xp.lock() = xp;
        }
        if let Some(fl) = nbt.byte("FoodLevel") {
            *self.food_level.lock() = fl as i32;
        }
    }
}

impl LivingEntity for VillagerEntity {
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

impl Mob for VillagerEntity {
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

impl AgeableMob for VillagerEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }
    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }
    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }
    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }
}

impl PathfinderMob for VillagerEntity {}
