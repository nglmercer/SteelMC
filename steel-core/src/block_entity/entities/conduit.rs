//! Conduit block entity.

use std::sync::{Arc, Weak};

use crate::entity::{Entity as _, LivingEntity as _, MobEffectInstance};
use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_registry::{
    sound_events, vanilla_block_entity_types, vanilla_blocks, vanilla_mob_effects,
};
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Vanilla `ConduitBlockEntity` re-evaluates its frame every 40 ticks.
const SHAPE_UPDATE_INTERVAL: i64 = 40;
/// Minimum frame blocks for a conduit to activate.
const MIN_FRAME_BLOCKS: usize = 16;
/// Frame blocks per 16 blocks of effect range.
const BLOCKS_PER_RANGE_STEP: usize = 7;
/// Effect range granted per range step.
const RANGE_STEP: i32 = 16;
/// Duration in ticks of the conduit power effect vanilla applies.
const CONDUIT_POWER_DURATION: i32 = 260;

/// Vanilla `ConduitBlockEntity`.
///
/// Vanilla's active conduit also hunts and damages a nearby hostile mob; that needs
/// hostile-mob targeting, so only the player effect is applied here.
pub struct ConduitBlockEntity {
    base: BlockEntityBase,
    /// Frame block positions found by the last shape update.
    frame_blocks: SyncMutex<Vec<BlockPos>>,
    is_active: SyncMutex<bool>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ConduitBlockEntity`.
unsafe impl DowncastType for ConduitBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/conduit");
}

impl ConduitBlockEntity {
    /// Creates an inactive conduit block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::CONDUIT, world, pos, state),
            frame_blocks: SyncMutex::new(Vec::new()),
            is_active: SyncMutex::new(false),
        }
    }

    /// Returns whether the conduit is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        *self.is_active.lock()
    }

    /// Vanilla `ConduitBlockEntity.VALID_BLOCKS`, mirrored from its hardcoded array.
    fn is_frame_block(state: BlockStateId) -> bool {
        let block = state.get_block();
        block == &vanilla_blocks::PRISMARINE
            || block == &vanilla_blocks::PRISMARINE_BRICKS
            || block == &vanilla_blocks::SEA_LANTERN
            || block == &vanilla_blocks::DARK_PRISMARINE
    }

    fn is_water_at(world: &Arc<World>, pos: BlockPos) -> bool {
        world
            .get_block_state(pos)
            .get_fluid_state()
            .fluid_id
            .has_tag(&FluidTag::WATER)
    }

    /// Vanilla `ConduitBlockEntity.updateShape`: recomputes the frame and activity.
    fn update_shape(&self, world: &Arc<World>, pos: BlockPos) -> bool {
        let mut frame = self.frame_blocks.lock();
        frame.clear();

        // The conduit must sit in a 3x3x3 pocket of water.
        for ox in -1..=1 {
            for oy in -1..=1 {
                for oz in -1..=1 {
                    if !Self::is_water_at(world, pos.offset(ox, oy, oz)) {
                        return false;
                    }
                }
            }
        }

        // The frame is the twelve edge-centre columns of the surrounding 5x5x5.
        for ox in -2..=2_i32 {
            for oy in -2..=2_i32 {
                for oz in -2..=2_i32 {
                    let (ax, ay, az) = (ox.abs(), oy.abs(), oz.abs());
                    let on_shell = ax > 1 || ay > 1 || az > 1;
                    let on_frame_line = (ox == 0 && (ay == 2 || az == 2))
                        || (oy == 0 && (ax == 2 || az == 2))
                        || (oz == 0 && (ax == 2 || ay == 2));
                    if !on_shell || !on_frame_line {
                        continue;
                    }

                    let test_pos = pos.offset(ox, oy, oz);
                    if Self::is_frame_block(world.get_block_state(test_pos)) {
                        frame.push(test_pos);
                    }
                }
            }
        }

        frame.len() >= MIN_FRAME_BLOCKS
    }

    /// Vanilla `ConduitBlockEntity.applyEffects`.
    fn apply_effects(&self, world: &Arc<World>, pos: BlockPos) {
        let frame_size = self.frame_blocks.lock().len();
        let range = i32::try_from(frame_size / BLOCKS_PER_RANGE_STEP).unwrap_or(0) * RANGE_STEP;
        if range <= 0 {
            return;
        }

        let effect = MobEffectInstance::with_duration(
            vanilla_mob_effects::CONDUIT_POWER,
            CONDUIT_POWER_DURATION,
            0,
        );
        let range_squared = i64::from(range) * i64::from(range);

        world.players.iter_players(|_uuid, player| {
            if player.is_in_water() {
                let player_pos = player.block_position();
                let delta = (
                    i64::from(player_pos.x() - pos.x()),
                    i64::from(player_pos.y() - pos.y()),
                    i64::from(player_pos.z() - pos.z()),
                );
                let distance_squared = delta.0 * delta.0 + delta.1 * delta.1 + delta.2 * delta.2;
                if distance_squared <= range_squared {
                    player.add_mob_effect(effect.clone());
                }
            }
            true
        });
    }
}

impl BlockEntity for ConduitBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `ConduitBlockEntity.serverTick`.
    fn tick(&self, world: &Arc<World>) {
        let game_time = world.level_data.read().game_time();
        if game_time % SHAPE_UPDATE_INTERVAL != 0 {
            return;
        }

        let pos = self.get_block_pos();
        let active = self.update_shape(world, pos);
        let was_active = {
            let mut is_active = self.is_active.lock();
            let previous = *is_active;
            *is_active = active;
            previous
        };

        if active != was_active {
            let sound = if active {
                &sound_events::BLOCK_CONDUIT_ACTIVATE
            } else {
                &sound_events::BLOCK_CONDUIT_DEACTIVATE
            };
            world.play_block_sound(sound, pos, 1.0, 1.0, None);
        }

        if active {
            self.apply_effects(world, pos);
        }
    }

    fn load_additional(&self, _nbt: &BorrowedNbtCompound<'_>) {
        // Vanilla only persists its hunting target, which Steel does not model yet.
    }

    fn save_additional(&self, _nbt: &mut NbtCompound) {}
}
