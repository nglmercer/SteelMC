//! Vault block entity: the trial chamber reward vault and its state machine.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, VaultState};
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::{LootContext, LootTableRef};
use steel_registry::sound_event::SoundEvent;
use steel_registry::{
    level_events, sound_events, vanilla_block_entity_types, vanilla_items, vanilla_loot_tables,
};
use steel_utils::locks::SyncMutex;
use steel_utils::random::RandomSource;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::{Entity as _, entity_loot_ref};
use crate::player::Player;
use crate::world::World;

/// Vanilla `VaultConfig.DEFAULT.activationRange`.
const ACTIVATION_RANGE: f64 = 4.0;
/// Vanilla `VaultConfig.DEFAULT.deactivationRange`.
const DEACTIVATION_RANGE: f64 = 4.5;
/// Vanilla `VaultServerData.MAX_REWARD_PLAYERS`.
const MAX_REWARD_PLAYERS: usize = 128;
/// Vanilla `VaultBlockEntity.Server.UNLOCKING_DELAY_TICKS`.
const UNLOCKING_DELAY_TICKS: i64 = 14;
/// Vanilla `VaultBlockEntity.Server.DISPLAY_CYCLE_TICK_RATE`.
const DISPLAY_CYCLE_TICK_RATE: i64 = 20;
/// Vanilla `VaultBlockEntity.Server.INSERT_FAIL_SOUND_BUFFER_TICKS`.
const INSERT_FAIL_SOUND_BUFFER_TICKS: i64 = 15;
/// Vanilla `VaultState.UPDATE_CONNECTED_PLAYERS_TICK_RATE`, also its ejection delays.
const STATE_UPDATE_TICK_RATE: i64 = 20;

/// Vanilla `VaultConfig`, restricted to the fields Steel can act on.
struct VaultConfig {
    /// Loot table the reward comes from.
    loot_table: LootTableRef,
    /// The key a player must insert.
    key_item: ItemStack,
}

impl VaultConfig {
    /// The vanilla default config: a trial key opening the trial chamber reward table.
    fn default_for(ominous: bool) -> Self {
        Self {
            loot_table: if ominous {
                &vanilla_loot_tables::CHESTS_TRIAL_CHAMBERS_REWARD_OMINOUS
            } else {
                &vanilla_loot_tables::CHESTS_TRIAL_CHAMBERS_REWARD
            },
            key_item: ItemStack::new(if ominous {
                &vanilla_items::OMINOUS_TRIAL_KEY
            } else {
                &vanilla_items::TRIAL_KEY
            }),
        }
    }
}

/// Vanilla `VaultServerData` and `VaultSharedData` merged into the vault's own state.
#[derive(Default)]
struct VaultData {
    /// Players who have already been rewarded by this vault.
    rewarded_players: Vec<Uuid>,
    /// Game time the state machine resumes at.
    state_updating_resumes_at: i64,
    /// Rewards still to be thrown out, ejected from the back.
    items_to_eject: Vec<ItemStack>,
    /// How many rewards this unlock started with, for the ejection sound ramp.
    total_ejections_needed: usize,
    /// Game time of the last rejected key, so the fail sound cannot be spammed.
    last_insert_fail: i64,
    /// The reward preview shown above the vault.
    display_item: ItemStack,
    /// Players currently keeping the vault awake.
    connected_players: Vec<Uuid>,
}

impl VaultData {
    fn has_rewarded(&self, player: &Player) -> bool {
        self.rewarded_players.contains(&player.gameprofile.id)
    }

    /// Vanilla `VaultServerData.addToRewardedPlayers`, dropping the oldest past the cap.
    fn add_rewarded(&mut self, player: &Player) {
        if self.has_rewarded(player) {
            return;
        }
        self.rewarded_players.push(player.gameprofile.id);
        if self.rewarded_players.len() > MAX_REWARD_PLAYERS {
            self.rewarded_players.remove(0);
        }
    }

    /// Vanilla `VaultServerData.ejectionProgress`.
    fn ejection_progress(&self) -> f32 {
        if self.total_ejections_needed <= 1 {
            return 1.0;
        }
        #[expect(
            clippy::cast_precision_loss,
            reason = "mirrors vanilla's float ejection ramp"
        )]
        let remaining = self.items_to_eject.len() as f32;
        #[expect(
            clippy::cast_precision_loss,
            reason = "mirrors vanilla's float ejection ramp"
        )]
        let total = self.total_ejections_needed as f32;
        // Vanilla: 1 - inverseLerp(remaining, 1, total).
        1.0 - (remaining - 1.0) / (total - 1.0)
    }

    fn next_item_to_eject(&self) -> ItemStack {
        self.items_to_eject
            .last()
            .cloned()
            .unwrap_or_else(ItemStack::empty)
    }
}

/// Vanilla `VaultBlockEntity`.
///
/// Vanilla's client-side data (the spinning display item and its particles) has no Steel
/// counterpart, so only the server half is modelled.
pub struct VaultBlockEntity {
    base: BlockEntityBase,
    data: SyncMutex<VaultData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `VaultBlockEntity`.
unsafe impl DowncastType for VaultBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/vault");
}

impl VaultBlockEntity {
    /// Creates an inactive vault block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::VAULT, world, pos, state),
            data: SyncMutex::new(VaultData::default()),
        }
    }

    /// Returns the reward preview the vault is currently showing.
    #[must_use]
    pub fn display_item(&self) -> ItemStack {
        self.data.lock().display_item.clone()
    }

    fn config(state: BlockStateId) -> VaultConfig {
        VaultConfig::default_for(state.get_value(&BlockStateProperties::OMINOUS))
    }

    fn vault_state(state: BlockStateId) -> VaultState {
        state.get_value(&BlockStateProperties::VAULT_STATE)
    }

    /// Vanilla `VaultBlockEntity.Server.canEjectReward`.
    fn can_eject_reward(config: &VaultConfig, state: VaultState) -> bool {
        !config.key_item.is_empty() && state != VaultState::Inactive
    }

    /// Vanilla `PlayerDetector.INCLUDING_CREATIVE_PLAYERS` over one range.
    fn detect_players(&self, world: &Arc<World>, range: f64, data: &VaultData) -> Vec<Uuid> {
        let pos = self.base.pos();
        let mut detected = Vec::new();
        world.players.iter_players(|uuid, player| {
            if player.is_spectator() || data.rewarded_players.contains(uuid) {
                return true;
            }
            // Vanilla compares the player's block position against the vault's.
            let player_pos = player.block_position();
            let dx = f64::from(player_pos.x() - pos.x());
            let dy = f64::from(player_pos.y() - pos.y());
            let dz = f64::from(player_pos.z() - pos.z());
            if dx.mul_add(dx, dy.mul_add(dy, dz * dz)) < range * range {
                detected.push(*uuid);
            }
            true
        });
        detected
    }

    /// Vanilla `VaultSharedData.updateConnectedPlayersWithinRange`.
    fn update_connected_players(&self, world: &Arc<World>, range: f64, data: &mut VaultData) {
        let detected = self.detect_players(world, range, data);
        if detected != data.connected_players {
            data.connected_players = detected;
        }
    }

    /// Vanilla `VaultBlockEntity.Server.getRandomDisplayItemFromLootTable`.
    fn random_display_item(pos: BlockPos, config: &VaultConfig) -> ItemStack {
        let mut rng = RandomSource::create_thread_safe();
        let mut context = LootContext::new(&mut rng).with_origin(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + 0.5,
            f64::from(pos.z()) + 0.5,
        );
        let items = config.loot_table.get_random_items(&mut context);
        if items.is_empty() {
            return ItemStack::empty();
        }
        let index = rand::random_range(0..items.len());
        items
            .into_iter()
            .nth(index)
            .unwrap_or_else(ItemStack::empty)
    }

    /// Vanilla `VaultBlockEntity.Server.cycleDisplayItemFromLootTable`.
    fn cycle_display_item(
        pos: BlockPos,
        state: VaultState,
        config: &VaultConfig,
        data: &mut VaultData,
    ) {
        data.display_item = if Self::can_eject_reward(config, state) {
            Self::random_display_item(pos, config)
        } else {
            ItemStack::empty()
        };
    }

    /// Vanilla `VaultBlockEntity.Server.tryInsertKey`.
    pub fn try_insert_key(
        &self,
        world: &Arc<World>,
        block_state: BlockStateId,
        player: &Player,
        key: &mut ItemStack,
    ) {
        let config = Self::config(block_state);
        let state = Self::vault_state(block_state);
        if !Self::can_eject_reward(&config, state) {
            return;
        }

        let pos = self.base.pos();
        let mut data = self.data.lock();

        if !ItemStack::is_same_item_same_components(key, &config.key_item)
            || key.count() < config.key_item.count()
        {
            self.play_insert_fail(
                world,
                &mut data,
                &sound_events::BLOCK_VAULT_INSERT_ITEM_FAIL,
            );
            return;
        }
        if data.has_rewarded(player) {
            self.play_insert_fail(
                world,
                &mut data,
                &sound_events::BLOCK_VAULT_REJECT_REWARDED_PLAYER,
            );
            return;
        }

        let rewards = Self::resolve_items_to_eject(pos, &config, player, key);
        if rewards.is_empty() {
            return;
        }

        key.shrink(config.key_item.count());
        data.items_to_eject = rewards;
        data.total_ejections_needed = data.items_to_eject.len();
        data.display_item = data.next_item_to_eject();
        data.state_updating_resumes_at = world.game_time() + UNLOCKING_DELAY_TICKS;
        data.add_rewarded(player);

        Self::set_vault_state(world, pos, block_state, VaultState::Unlocking, &mut data);
        self.update_connected_players(world, DEACTIVATION_RANGE, &mut data);
    }

    /// Vanilla `VaultBlockEntity.Server.resolveItemsToEject`.
    fn resolve_items_to_eject(
        pos: BlockPos,
        config: &VaultConfig,
        player: &Player,
        key: &ItemStack,
    ) -> Vec<ItemStack> {
        let mut rng = RandomSource::create_thread_safe();
        let mut context = LootContext::new(&mut rng)
            .with_origin(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 0.5,
                f64::from(pos.z()) + 0.5,
            )
            .with_tool(key)
            .with_this_entity(entity_loot_ref(player));
        config.loot_table.get_random_items(&mut context)
    }

    /// Vanilla `VaultBlockEntity.Server.playInsertFailSound`, rate limited like vanilla.
    fn play_insert_fail(
        &self,
        world: &Arc<World>,
        data: &mut VaultData,
        sound: &'static SoundEvent,
    ) {
        let now = world.game_time();
        if now < data.last_insert_fail + INSERT_FAIL_SOUND_BUFFER_TICKS {
            return;
        }
        data.last_insert_fail = now;
        world.play_sound(sound, SoundSource::Blocks, self.base.pos(), 1.0, 1.0, None);
    }

    /// Vanilla `VaultBlockEntity.Server.setVaultState`, including the transition effects.
    fn set_vault_state(
        world: &Arc<World>,
        pos: BlockPos,
        block_state: BlockStateId,
        next: VaultState,
        data: &mut VaultData,
    ) {
        let current = Self::vault_state(block_state);
        if current == next {
            return;
        }

        let new_state = block_state.set_value(&BlockStateProperties::VAULT_STATE, next.clone());
        world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);

        let ominous = block_state.get_value(&BlockStateProperties::OMINOUS);
        let config = Self::config(block_state);

        // Vanilla `VaultState.onExit`.
        if current == VaultState::Ejecting {
            world.play_sound(
                &sound_events::BLOCK_VAULT_CLOSE_SHUTTER,
                SoundSource::Blocks,
                pos,
                1.0,
                1.0,
                None,
            );
        }

        // Vanilla `VaultState.onEnter`.
        match next {
            VaultState::Inactive => {
                data.display_item = ItemStack::empty();
                world.level_event(
                    level_events::ANIMATION_VAULT_DEACTIVATE,
                    pos,
                    i32::from(ominous),
                    None,
                );
            }
            VaultState::Active => {
                if data.display_item.is_empty() {
                    Self::cycle_display_item(pos, next, &config, data);
                }
                world.level_event(
                    level_events::ANIMATION_VAULT_ACTIVATE,
                    pos,
                    i32::from(ominous),
                    None,
                );
            }
            VaultState::Unlocking => world.play_sound(
                &sound_events::BLOCK_VAULT_INSERT_ITEM,
                SoundSource::Blocks,
                pos,
                1.0,
                1.0,
                None,
            ),
            VaultState::Ejecting => world.play_sound(
                &sound_events::BLOCK_VAULT_OPEN_SHUTTER,
                SoundSource::Blocks,
                pos,
                1.0,
                1.0,
                None,
            ),
        }
    }

    /// Vanilla `VaultState.ejectResultItem`.
    fn eject_result_item(world: &Arc<World>, pos: BlockPos, item: ItemStack, progress: f32) {
        if !item.is_empty() {
            let spawn = DVec3::new(
                f64::from(pos.x()) + 0.5,
                f64::from(pos.y()) + 1.2,
                f64::from(pos.z()) + 0.5,
            );
            if let Some(entity) = world.spawn_item(spawn, item) {
                entity.set_default_pickup_delay();
            }
        }
        world.level_event(level_events::ANIMATION_VAULT_EJECT_ITEM, pos, 0, None);
        world.play_sound(
            &sound_events::BLOCK_VAULT_EJECT_ITEM,
            SoundSource::Blocks,
            pos,
            1.0,
            0.4f32.mul_add(progress, 0.8),
            None,
        );
    }

    /// Vanilla `VaultState.tickAndGetNext`.
    fn tick_and_get_next(
        &self,
        world: &Arc<World>,
        block_state: BlockStateId,
        data: &mut VaultData,
    ) -> VaultState {
        let pos = self.base.pos();
        match Self::vault_state(block_state) {
            VaultState::Inactive => {
                self.update_state_for_connected_players(world, ACTIVATION_RANGE, data)
            }
            VaultState::Active => {
                self.update_state_for_connected_players(world, DEACTIVATION_RANGE, data)
            }
            VaultState::Unlocking => {
                data.state_updating_resumes_at = world.game_time() + STATE_UPDATE_TICK_RATE;
                VaultState::Ejecting
            }
            VaultState::Ejecting => {
                if data.items_to_eject.is_empty() {
                    data.total_ejections_needed = 0;
                    return self.update_state_for_connected_players(
                        world,
                        DEACTIVATION_RANGE,
                        data,
                    );
                }

                let progress = data.ejection_progress();
                let item = data.items_to_eject.pop().unwrap_or_else(ItemStack::empty);
                Self::eject_result_item(world, pos, item, progress);
                data.display_item = data.next_item_to_eject();
                data.state_updating_resumes_at = world.game_time() + STATE_UPDATE_TICK_RATE;
                VaultState::Ejecting
            }
        }
    }

    /// Vanilla `VaultState.updateStateForConnectedPlayers`.
    fn update_state_for_connected_players(
        &self,
        world: &Arc<World>,
        range: f64,
        data: &mut VaultData,
    ) -> VaultState {
        self.update_connected_players(world, range, data);
        data.state_updating_resumes_at = world.game_time() + STATE_UPDATE_TICK_RATE;
        if data.connected_players.is_empty() {
            VaultState::Inactive
        } else {
            VaultState::Active
        }
    }
}

impl BlockEntity for VaultBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `VaultBlockEntity.Server.tick`.
    fn tick(&self, world: &Arc<World>) {
        let pos = self.base.pos();
        let block_state = world.get_block_state(pos);
        let vault_state = Self::vault_state(block_state);
        let config = Self::config(block_state);
        let game_time = world.game_time();

        let mut data = self.data.lock();

        // Vanilla `shouldCycleDisplayItem`: only while waiting for a key.
        if vault_state != VaultState::Unlocking
            && vault_state != VaultState::Ejecting
            && game_time % DISPLAY_CYCLE_TICK_RATE == 0
        {
            Self::cycle_display_item(pos, vault_state, &config, &mut data);
        }

        if game_time < data.state_updating_resumes_at {
            return;
        }

        let next = self.tick_and_get_next(world, block_state, &mut data);
        Self::set_vault_state(world, pos, block_state, next, &mut data);
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut data = self.data.lock();

        if let Some(server_data) = nbt.compound("server_data") {
            if let Some(resumes_at) = server_data.long("state_updating_resumes_at") {
                data.state_updating_resumes_at = resumes_at;
            }
            if let Some(items) = server_data.list("items_to_eject") {
                let mut stacks = Vec::new();
                if let Some(compounds) = items.compounds() {
                    for item in compounds {
                        if let Some(stack) = ItemStack::from_borrowed_compound(&item) {
                            stacks.push(stack);
                        }
                    }
                }
                data.items_to_eject = stacks;
            }
            if let Some(players) = server_data.list("rewarded_players") {
                data.rewarded_players = players
                    .strings()
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|uuid| Uuid::parse_str(&uuid.to_string()).ok())
                    .collect();
            }
            data.total_ejections_needed = data.items_to_eject.len();
        }

        if let Some(shared) = nbt.compound("shared_data")
            && let Some(display) = shared.compound("display_item")
        {
            data.display_item =
                ItemStack::from_borrowed_compound(&display).unwrap_or_else(ItemStack::empty);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let data = self.data.lock();

        let mut server_data = NbtCompound::new();
        server_data.insert("state_updating_resumes_at", data.state_updating_resumes_at);
        server_data.insert(
            "items_to_eject",
            NbtTag::List(NbtList::Compound(
                data.items_to_eject
                    .iter()
                    .filter_map(|item| match item.to_nbt_tag_ref() {
                        NbtTag::Compound(compound) => Some(compound),
                        _ => None,
                    })
                    .collect(),
            )),
        );
        server_data.insert(
            "rewarded_players",
            NbtTag::List(NbtList::String(
                data.rewarded_players
                    .iter()
                    .map(|uuid| uuid.to_string().into())
                    .collect(),
            )),
        );
        nbt.insert("server_data", NbtTag::Compound(server_data));

        let mut shared_data = NbtCompound::new();
        if !data.display_item.is_empty()
            && let NbtTag::Compound(display) = data.display_item.to_nbt_tag_ref()
        {
            shared_data.insert("display_item", NbtTag::Compound(display));
        }
        nbt.insert("shared_data", NbtTag::Compound(shared_data));
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::item_stack::ItemStack;
    use steel_registry::test_support::init_test_registry;
    use steel_registry::vanilla_items;

    use super::{VaultBlockEntity, VaultConfig, VaultData, VaultState};

    #[test]
    fn an_inactive_vault_never_ejects_a_reward() {
        init_test_registry();
        let config = VaultConfig::default_for(false);

        assert!(!VaultBlockEntity::can_eject_reward(
            &config,
            VaultState::Inactive
        ));
        assert!(VaultBlockEntity::can_eject_reward(
            &config,
            VaultState::Active
        ));
    }

    #[test]
    fn an_ominous_vault_wants_an_ominous_key() {
        init_test_registry();

        assert!(
            VaultConfig::default_for(false)
                .key_item
                .is(&vanilla_items::TRIAL_KEY)
        );
        assert!(
            VaultConfig::default_for(true)
                .key_item
                .is(&vanilla_items::OMINOUS_TRIAL_KEY)
        );
    }

    #[test]
    fn the_rewarded_player_list_drops_its_oldest_entry_past_the_cap() {
        let mut data = VaultData::default();
        for index in 0..200_u128 {
            data.rewarded_players.push(uuid::Uuid::from_u128(index));
            if data.rewarded_players.len() > super::MAX_REWARD_PLAYERS {
                data.rewarded_players.remove(0);
            }
        }

        assert_eq!(data.rewarded_players.len(), super::MAX_REWARD_PLAYERS);
        // The first entries aged out, the last ones survived.
        assert!(!data.rewarded_players.contains(&uuid::Uuid::from_u128(0)));
        assert!(data.rewarded_players.contains(&uuid::Uuid::from_u128(199)));
    }

    #[test]
    fn the_ejection_sound_ramps_from_the_first_reward_to_the_last() {
        let mut data = VaultData {
            total_ejections_needed: 4,
            items_to_eject: vec![ItemStack::empty(); 4],
            ..VaultData::default()
        };
        assert!((data.ejection_progress() - 0.0).abs() < f32::EPSILON);

        data.items_to_eject.truncate(1);
        assert!((data.ejection_progress() - 1.0).abs() < f32::EPSILON);

        // A single-reward unlock is always at full progress.
        data.total_ejections_needed = 1;
        assert!((data.ejection_progress() - 1.0).abs() < f32::EPSILON);
    }
}
