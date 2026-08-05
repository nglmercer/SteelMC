//! Beacon block entity.

use std::sync::{Arc, Weak};

use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::NbtCompound;
use std::str::FromStr as _;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::mob_effect::MobEffectRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{REGISTRY, RegistryExt as _, sound_events, vanilla_block_entity_types};
use steel_utils::{
    BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier, locks::SyncMutex,
};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::entity::{Entity as _, LivingEntity as _, MobEffectInstance};
use crate::world::World;

/// Vanilla re-checks the beacon's base every 80 ticks.
const BASE_UPDATE_INTERVAL: i64 = 80;
/// Vanilla `BeaconBlockEntity.MAX_LEVELS`.
const MAX_LEVELS: i32 = 4;

struct BeaconState {
    /// Pyramid tiers under the beacon, 0 when inactive.
    levels: i32,
    primary_power: Option<MobEffectRef>,
    secondary_power: Option<MobEffectRef>,
}

/// Vanilla `BeaconBlockEntity`.
///
/// Selecting the powers needs vanilla's beacon menu, which Steel does not have yet, so
/// the powers can currently only arrive from saved data.
pub struct BeaconBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<BeaconState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BeaconBlockEntity`.
unsafe impl DowncastType for BeaconBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/beacon");
}

impl BeaconBlockEntity {
    /// Creates an inactive beacon block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::BEACON, world, pos, state),
            state: SyncMutex::new(BeaconState {
                levels: 0,
                primary_power: None,
                secondary_power: None,
            }),
        }
    }

    /// Returns the number of complete pyramid tiers under the beacon.
    #[must_use]
    pub fn levels(&self) -> i32 {
        self.state.lock().levels
    }

    /// Vanilla `BeaconBlockEntity.updateBase`: counts complete pyramid tiers.
    fn update_base(world: &Arc<World>, pos: BlockPos) -> i32 {
        let mut levels = 0;

        for step in 1..=MAX_LEVELS {
            let layer_y = pos.y() - step;
            if layer_y < world.min_y() {
                break;
            }

            let mut complete = true;
            'layer: for x in (pos.x() - step)..=(pos.x() + step) {
                for z in (pos.z() - step)..=(pos.z() + step) {
                    let base_state = world.get_block_state(BlockPos::new(x, layer_y, z));
                    if !base_state.get_block().has_tag(&BlockTag::BEACON_BASE_BLOCKS) {
                        complete = false;
                        break 'layer;
                    }
                }
            }

            if !complete {
                break;
            }
            levels = step;
        }

        levels
    }

    /// Vanilla `BeaconBlockEntity.applyEffects`.
    fn apply_effects(&self, world: &Arc<World>, pos: BlockPos) {
        let (levels, primary, secondary) = {
            let state = self.state.lock();
            (state.levels, state.primary_power, state.secondary_power)
        };

        let Some(primary) = primary else {
            return;
        };

        let range = i64::from(levels) * 10 + 10;
        let range_squared = range * range;
        // A level-four beacon upgrades its primary power when both slots match.
        let amplifier = i32::from(levels >= MAX_LEVELS && primary == secondary);
        let duration = (9 + levels * 2) * 20;

        let primary_effect = MobEffectInstance::with_duration(primary, duration, amplifier);
        let secondary_effect = (levels >= MAX_LEVELS && primary != secondary)
            .then(|| secondary.map(|effect| MobEffectInstance::with_duration(effect, duration, 0)))
            .flatten();

        world.players.iter_players(|_uuid, player| {
            let player_pos = player.block_position();
            let dx = i64::from(player_pos.x() - pos.x());
            let dz = i64::from(player_pos.z() - pos.z());
            if dx * dx + dz * dz <= range_squared {
                player.add_mob_effect(primary_effect.clone());
                if let Some(secondary_effect) = &secondary_effect {
                    player.add_mob_effect(secondary_effect.clone());
                }
            }
            true
        });
    }

    fn read_effect(nbt: &NbtCompoundView<'_, '_>, key: &str) -> Option<MobEffectRef> {
        let name = nbt.string(key)?;
        let identifier = Identifier::from_str(name.to_str().as_ref()).ok()?;
        REGISTRY.mob_effects.by_key(&identifier)
    }
}

impl BlockEntity for BeaconBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `BeaconBlockEntity.tick`.
    fn tick(&self, world: &Arc<World>) {
        let game_time = world.level_data.read().game_time();
        if game_time % BASE_UPDATE_INTERVAL != 0 {
            return;
        }

        let pos = self.get_block_pos();
        let levels = Self::update_base(world, pos);
        let was_active = {
            let mut state = self.state.lock();
            let previous = state.levels > 0;
            state.levels = levels;
            previous
        };

        let is_active = levels > 0;
        if is_active != was_active {
            let sound = if is_active {
                &sound_events::BLOCK_BEACON_ACTIVATE
            } else {
                &sound_events::BLOCK_BEACON_DEACTIVATE
            };
            world.play_block_sound(sound, pos, 1.0, 1.0, None);
        }

        if is_active {
            self.apply_effects(world, pos);
            world.play_block_sound(&sound_events::BLOCK_BEACON_AMBIENT, pos, 1.0, 1.0, None);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();
        state.primary_power = Self::read_effect(&nbt, "primary_effect");
        state.secondary_power = Self::read_effect(&nbt, "secondary_effect");
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        if let Some(primary) = state.primary_power {
            nbt.insert("primary_effect", primary.key.to_string());
        }
        if let Some(secondary) = state.secondary_power {
            nbt.insert("secondary_effect", secondary.key.to_string());
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        let mut nbt = NbtCompound::new();
        self.save_additional(&mut nbt);
        Some(nbt)
    }
}
