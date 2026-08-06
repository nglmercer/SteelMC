//! Vanilla `SpawnPlacementTypes` and the placement half of `SpawnPlacements`.
//!
//! Vanilla decides *where* a mob may spawn per `EntityType`, not per `MobCategory`:
//! `SpawnPlacements` maps each type to a [`SpawnPlacementType`], a heightmap, and a
//! spawn predicate. This module ports the placement types and the shared
//! `NaturalSpawner.isValidEmptySpawnBlock` helper they lean on.
//!
//! The per-type registration table and the spawn predicates live alongside the entity
//! behaviors; this module only provides the placement primitives they are keyed to.

use std::sync::Arc;

use steel_registry::{
    REGISTRY, TaggedRegistryExt,
    blocks::block_state_ext::BlockStateExt,
    entity_type::EntityTypeRef,
    fluid::{is_lava_fluid, is_water_fluid},
    vanilla_block_tags::BlockTag,
    vanilla_blocks,
};
use steel_utils::{BlockPos, BlockStateId};

use crate::{
    behavior::{BLOCK_BEHAVIORS, BlockStateBehaviorExt as _},
    chunk::heightmap::HeightmapType,
    entity::ai::path::PathComputationType,
    entity::ai::walk::WalkPathEvaluator,
    world::{SignalGetter as _, SignalQueryContext, World},
};

/// Vanilla `SpawnPlacementType` (`SpawnPlacementTypes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpawnPlacementType {
    /// `SpawnPlacementTypes.NO_RESTRICTIONS` — always accepts the position.
    ///
    /// This is also the fallback vanilla `SpawnPlacements.getPlacementType` returns for a
    /// type with no registration.
    #[default]
    NoRestrictions,
    /// `SpawnPlacementTypes.IN_WATER`.
    InWater,
    /// `SpawnPlacementTypes.IN_LAVA`.
    InLava,
    /// `SpawnPlacementTypes.ON_GROUND`.
    OnGround,
}

impl SpawnPlacementType {
    /// Returns vanilla `SpawnPlacementType.isSpawnPositionOk`.
    #[must_use]
    pub fn is_spawn_position_ok(
        self,
        world: &Arc<World>,
        pos: BlockPos,
        entity_type: EntityTypeRef,
    ) -> bool {
        match self {
            Self::NoRestrictions => true,
            // Every restricted variant gates on the world border first.
            Self::InWater => {
                if !world.is_block_within_world_border(pos) {
                    return false;
                }
                let above = pos.above();
                let above_state = world.get_block_state(above);
                is_water_fluid(world.get_block_state(pos).get_fluid_state().fluid_id)
                    && !world.is_redstone_conductor(above_state, above)
            }
            Self::InLava => {
                world.is_block_within_world_border(pos)
                    && is_lava_fluid(world.get_block_state(pos).get_fluid_state().fluid_id)
            }
            Self::OnGround => {
                if !world.is_block_within_world_border(pos) {
                    return false;
                }
                let below = pos.below();
                let below_state = world.get_block_state(below);
                if !BLOCK_BEHAVIORS
                    .get_behavior(below_state.get_block())
                    .is_valid_spawn(below_state, world.as_ref(), below, entity_type)
                {
                    return false;
                }
                is_valid_empty_spawn_block_at(world, pos, entity_type)
                    && is_valid_empty_spawn_block_at(world, pos.above(), entity_type)
            }
        }
    }

    /// Returns vanilla `SpawnPlacementType.adjustSpawnPosition`.
    ///
    /// Only `ON_GROUND` overrides the identity default: it steps one block down when the
    /// block below is land-pathfindable, so a mob spawns standing on the surface rather
    /// than floating one block above it.
    #[must_use]
    pub fn adjust_spawn_position(self, world: &Arc<World>, candidate: BlockPos) -> BlockPos {
        if self != Self::OnGround {
            return candidate;
        }
        let below = candidate.below();
        if world
            .get_block_state(below)
            .is_pathfindable(PathComputationType::Land)
        {
            below
        } else {
            candidate
        }
    }
}

/// Returns vanilla `NaturalSpawner.isValidEmptySpawnBlock`.
///
/// Checked in vanilla's order: a full collision shape, a signal source, any fluid, the
/// `prevent_mob_spawning_inside` tag, then whether the block is dangerous to this type.
#[must_use]
pub fn is_valid_empty_spawn_block_at(
    world: &Arc<World>,
    pos: BlockPos,
    entity_type: EntityTypeRef,
) -> bool {
    let state = world.get_block_state(pos);
    if world.is_collision_shape_full_block_at(pos, state) {
        return false;
    }
    if BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .is_signal_source(state, SignalQueryContext::DEFAULT)
    {
        return false;
    }
    if !state.get_fluid_state().is_empty() {
        return false;
    }
    if REGISTRY
        .blocks
        .is_in_tag(state.get_block(), &BlockTag::PREVENT_MOB_SPAWNING_INSIDE)
    {
        return false;
    }
    !is_block_dangerous(entity_type, state)
}

/// Returns vanilla `EntityType.isBlockDangerous`.
///
/// DIVERGENCE: vanilla short-circuits on `state.is(this.immuneTo)`, the per-type block tag
/// that lets striders stand in lava and so on. `immuneTo` is not present in
/// `build_assets/entities.json`, so that early-out is omitted until `SteelExtractor` emits it.
#[must_use]
fn is_block_dangerous(entity_type: EntityTypeRef, state: BlockStateId) -> bool {
    if !entity_type.fire_immune && WalkPathEvaluator::is_burning_block(state) {
        return true;
    }
    let block = state.get_block();
    block == &vanilla_blocks::WITHER_ROSE
        || block == &vanilla_blocks::SWEET_BERRY_BUSH
        || block == &vanilla_blocks::CACTUS
        || block == &vanilla_blocks::POWDER_SNOW
}

/// Returns the heightmap vanilla `SpawnPlacements.getHeightmapType` falls back to for an
/// unregistered type.
#[must_use]
pub const fn default_heightmap_type() -> HeightmapType {
    HeightmapType::MotionBlockingNoLeaves
}

/// Vanilla `SpawnPlacements.DATA_BY_TYPE`, minus the spawn predicates.
///
/// Transcribed from the `SpawnPlacements` static initializer in registration order. Every
/// vanilla entry uses `MOTION_BLOCKING_NO_LEAVES` except ocelot and parrot, which use
/// `MOTION_BLOCKING`.
///
/// Keys are `minecraft:` paths. Types absent from this table fall back to
/// `NO_RESTRICTIONS` / `MOTION_BLOCKING_NO_LEAVES`, exactly as vanilla's map lookup does.
static PLACEMENT_BY_TYPE: &[(&str, SpawnPlacementType, HeightmapType)] = {
    use HeightmapType::{MotionBlocking, MotionBlockingNoLeaves as Mbnl};
    use SpawnPlacementType::{InLava, InWater, NoRestrictions, OnGround};
    &[
        ("axolotl", InWater, Mbnl),
        ("cod", InWater, Mbnl),
        ("dolphin", InWater, Mbnl),
        ("drowned", InWater, Mbnl),
        ("guardian", InWater, Mbnl),
        ("pufferfish", InWater, Mbnl),
        ("salmon", InWater, Mbnl),
        ("squid", InWater, Mbnl),
        ("tropical_fish", InWater, Mbnl),
        ("armadillo", OnGround, Mbnl),
        ("bat", OnGround, Mbnl),
        ("blaze", OnGround, Mbnl),
        ("bogged", OnGround, Mbnl),
        ("breeze", OnGround, Mbnl),
        ("camel", OnGround, Mbnl),
        ("camel_husk", OnGround, Mbnl),
        ("cave_spider", OnGround, Mbnl),
        ("chicken", OnGround, Mbnl),
        ("cow", OnGround, Mbnl),
        ("creeper", OnGround, Mbnl),
        ("donkey", OnGround, Mbnl),
        ("enderman", OnGround, Mbnl),
        ("endermite", OnGround, Mbnl),
        ("ender_dragon", OnGround, Mbnl),
        ("frog", OnGround, Mbnl),
        ("ghast", OnGround, Mbnl),
        ("happy_ghast", OnGround, Mbnl),
        ("giant", OnGround, Mbnl),
        ("glow_squid", InWater, Mbnl),
        ("goat", OnGround, Mbnl),
        ("horse", OnGround, Mbnl),
        ("husk", OnGround, Mbnl),
        ("iron_golem", OnGround, Mbnl),
        ("llama", OnGround, Mbnl),
        ("magma_cube", OnGround, Mbnl),
        ("sulfur_cube", OnGround, Mbnl),
        ("mooshroom", OnGround, Mbnl),
        ("mule", OnGround, Mbnl),
        ("nautilus", InWater, Mbnl),
        ("ocelot", OnGround, MotionBlocking),
        ("parrot", OnGround, MotionBlocking),
        ("pig", OnGround, Mbnl),
        ("hoglin", OnGround, Mbnl),
        ("piglin", OnGround, Mbnl),
        ("pillager", OnGround, Mbnl),
        ("polar_bear", OnGround, Mbnl),
        ("rabbit", OnGround, Mbnl),
        ("sheep", OnGround, Mbnl),
        ("silverfish", OnGround, Mbnl),
        ("skeleton", OnGround, Mbnl),
        ("skeleton_horse", OnGround, Mbnl),
        ("slime", OnGround, Mbnl),
        ("snow_golem", OnGround, Mbnl),
        ("spider", OnGround, Mbnl),
        ("stray", OnGround, Mbnl),
        ("parched", OnGround, Mbnl),
        ("strider", InLava, Mbnl),
        ("turtle", OnGround, Mbnl),
        ("villager", OnGround, Mbnl),
        ("witch", OnGround, Mbnl),
        ("wither", OnGround, Mbnl),
        ("wither_skeleton", OnGround, Mbnl),
        ("wolf", OnGround, Mbnl),
        ("zoglin", OnGround, Mbnl),
        ("creaking", OnGround, Mbnl),
        ("zombie", OnGround, Mbnl),
        ("zombie_horse", OnGround, Mbnl),
        ("zombified_piglin", OnGround, Mbnl),
        ("zombie_villager", OnGround, Mbnl),
        ("cat", OnGround, Mbnl),
        ("elder_guardian", InWater, Mbnl),
        ("evoker", NoRestrictions, Mbnl),
        ("fox", NoRestrictions, Mbnl),
        ("illusioner", NoRestrictions, Mbnl),
        ("panda", NoRestrictions, Mbnl),
        ("phantom", NoRestrictions, Mbnl),
        ("ravager", OnGround, Mbnl),
        ("shulker", NoRestrictions, Mbnl),
        ("trader_llama", NoRestrictions, Mbnl),
        ("vex", NoRestrictions, Mbnl),
        ("vindicator", NoRestrictions, Mbnl),
        ("wandering_trader", OnGround, Mbnl),
        ("warden", NoRestrictions, Mbnl),
    ]
};

fn registration_for(
    entity_type: EntityTypeRef,
) -> Option<&'static (&'static str, SpawnPlacementType, HeightmapType)> {
    if entity_type.key.namespace != "minecraft" {
        return None;
    }
    PLACEMENT_BY_TYPE
        .iter()
        .find(|(path, _, _)| *path == entity_type.key.path)
}

/// Returns vanilla `SpawnPlacements.getPlacementType`.
#[must_use]
pub fn placement_type_for(entity_type: EntityTypeRef) -> SpawnPlacementType {
    registration_for(entity_type).map_or(SpawnPlacementType::NoRestrictions, |(_, p, _)| *p)
}

/// Returns vanilla `SpawnPlacements.getHeightmapType`.
#[must_use]
pub fn heightmap_type_for(entity_type: EntityTypeRef) -> HeightmapType {
    registration_for(entity_type).map_or_else(default_heightmap_type, |(_, _, h)| *h)
}

/// Returns vanilla `SpawnPlacements.isSpawnPositionOk`.
#[must_use]
pub fn is_spawn_position_ok(world: &Arc<World>, pos: BlockPos, entity_type: EntityTypeRef) -> bool {
    placement_type_for(entity_type).is_spawn_position_ok(world, pos, entity_type)
}

#[cfg(test)]
mod tests {
    use super::{HeightmapType, PLACEMENT_BY_TYPE, SpawnPlacementType};

    #[test]
    fn registration_table_has_no_duplicate_entity_types() {
        // Vanilla `SpawnPlacements.register` throws on a duplicate registration, so a
        // repeated key here means the transcription drifted from the static initializer.
        let mut paths: Vec<&str> = PLACEMENT_BY_TYPE.iter().map(|(path, _, _)| *path).collect();
        paths.sort_unstable();
        let before = paths.len();
        paths.dedup();
        assert_eq!(
            paths.len(),
            before,
            "duplicate entity type in PLACEMENT_BY_TYPE"
        );
    }

    #[test]
    fn only_ocelot_and_parrot_use_the_motion_blocking_heightmap() {
        let motion_blocking: Vec<&str> = PLACEMENT_BY_TYPE
            .iter()
            .filter(|(_, _, heightmap)| *heightmap == HeightmapType::MotionBlocking)
            .map(|(path, _, _)| *path)
            .collect();
        assert_eq!(motion_blocking, ["ocelot", "parrot"]);
    }

    #[test]
    fn strider_is_the_only_lava_placement() {
        let in_lava: Vec<&str> = PLACEMENT_BY_TYPE
            .iter()
            .filter(|(_, placement, _)| *placement == SpawnPlacementType::InLava)
            .map(|(path, _, _)| *path)
            .collect();
        assert_eq!(in_lava, ["strider"]);
    }
}
