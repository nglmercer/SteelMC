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
use steel_utils::{BlockPos, BlockStateId, types::Difficulty};

use crate::{
    behavior::{BLOCK_BEHAVIORS, BlockStateBehaviorExt as _},
    chunk::heightmap::HeightmapType,
    entity::EntitySpawnReason,
    entity::ai::path::PathComputationType,
    entity::ai::walk::WalkPathEvaluator,
    entity::spawn_predicates::SpawnPredicate,
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
static PLACEMENT_BY_TYPE: &[(&str, SpawnPlacementType, HeightmapType, SpawnPredicate)] = {
    use super::spawn_predicates::{
        check_animal_spawn_rules, check_any_light_monster_spawn_rules, check_armadillo_spawn_rules,
        check_axolotl_spawn_rules, check_bat_spawn_rules, check_camel_spawn_rules,
        check_drowned_spawn_rules, check_endermite_spawn_rules, check_fox_spawn_rules,
        check_frog_spawn_rules, check_ghast_spawn_rules, check_glow_squid_spawn_rules,
        check_goat_spawn_rules, check_guardian_spawn_rules, check_magma_cube_spawn_rules,
        check_mob_spawn_rules, check_monster_spawn_rules, check_mushroom_spawn_rules,
        check_nautilus_spawn_rules, check_not_on_nether_wart_block, check_ocelot_spawn_rules,
        check_parrot_spawn_rules, check_patrolling_monster_spawn_rules,
        check_polar_bear_spawn_rules, check_rabbit_spawn_rules, check_skeleton_horse_spawn_rules,
        check_slime_spawn_rules, check_stray_spawn_rules, check_strider_spawn_rules,
        check_sulfur_cube_spawn_rules, check_surface_monsters_spawn_rules,
        check_surface_water_animal_spawn_rules, check_tropical_fish_spawn_rules,
        check_turtle_spawn_rules, check_wolf_spawn_rules, check_zombified_piglin_spawn_rules,
    };
    use HeightmapType::{MotionBlocking, MotionBlockingNoLeaves as Mbnl};
    use SpawnPlacementType::{InLava, InWater, NoRestrictions, OnGround};
    &[
        ("axolotl", InWater, Mbnl, check_axolotl_spawn_rules),
        ("cod", InWater, Mbnl, check_surface_water_animal_spawn_rules),
        (
            "dolphin",
            InWater,
            Mbnl,
            check_surface_water_animal_spawn_rules,
        ),
        ("drowned", InWater, Mbnl, check_drowned_spawn_rules),
        ("guardian", InWater, Mbnl, check_guardian_spawn_rules),
        (
            "pufferfish",
            InWater,
            Mbnl,
            check_surface_water_animal_spawn_rules,
        ),
        (
            "salmon",
            InWater,
            Mbnl,
            check_surface_water_animal_spawn_rules,
        ),
        (
            "squid",
            InWater,
            Mbnl,
            check_surface_water_animal_spawn_rules,
        ),
        (
            "tropical_fish",
            InWater,
            Mbnl,
            check_tropical_fish_spawn_rules,
        ),
        ("armadillo", OnGround, Mbnl, check_armadillo_spawn_rules),
        ("bat", OnGround, Mbnl, check_bat_spawn_rules),
        ("blaze", OnGround, Mbnl, check_any_light_monster_spawn_rules),
        ("bogged", OnGround, Mbnl, check_monster_spawn_rules),
        (
            "breeze",
            OnGround,
            Mbnl,
            check_any_light_monster_spawn_rules,
        ),
        ("camel", OnGround, Mbnl, check_camel_spawn_rules),
        (
            "camel_husk",
            OnGround,
            Mbnl,
            check_surface_monsters_spawn_rules,
        ),
        ("cave_spider", OnGround, Mbnl, check_monster_spawn_rules),
        ("chicken", OnGround, Mbnl, check_animal_spawn_rules),
        ("cow", OnGround, Mbnl, check_animal_spawn_rules),
        ("creeper", OnGround, Mbnl, check_monster_spawn_rules),
        ("donkey", OnGround, Mbnl, check_animal_spawn_rules),
        ("enderman", OnGround, Mbnl, check_monster_spawn_rules),
        ("endermite", OnGround, Mbnl, check_endermite_spawn_rules),
        ("ender_dragon", OnGround, Mbnl, check_mob_spawn_rules),
        ("frog", OnGround, Mbnl, check_frog_spawn_rules),
        ("ghast", OnGround, Mbnl, check_ghast_spawn_rules),
        ("happy_ghast", OnGround, Mbnl, check_animal_spawn_rules),
        ("giant", OnGround, Mbnl, check_monster_spawn_rules),
        ("glow_squid", InWater, Mbnl, check_glow_squid_spawn_rules),
        ("goat", OnGround, Mbnl, check_goat_spawn_rules),
        ("horse", OnGround, Mbnl, check_animal_spawn_rules),
        ("husk", OnGround, Mbnl, check_surface_monsters_spawn_rules),
        ("iron_golem", OnGround, Mbnl, check_mob_spawn_rules),
        ("llama", OnGround, Mbnl, check_animal_spawn_rules),
        ("magma_cube", OnGround, Mbnl, check_magma_cube_spawn_rules),
        ("sulfur_cube", OnGround, Mbnl, check_sulfur_cube_spawn_rules),
        ("mooshroom", OnGround, Mbnl, check_mushroom_spawn_rules),
        ("mule", OnGround, Mbnl, check_animal_spawn_rules),
        ("nautilus", InWater, Mbnl, check_nautilus_spawn_rules),
        ("ocelot", OnGround, MotionBlocking, check_ocelot_spawn_rules),
        ("parrot", OnGround, MotionBlocking, check_parrot_spawn_rules),
        ("pig", OnGround, Mbnl, check_animal_spawn_rules),
        ("hoglin", OnGround, Mbnl, check_not_on_nether_wart_block),
        ("piglin", OnGround, Mbnl, check_not_on_nether_wart_block),
        (
            "pillager",
            OnGround,
            Mbnl,
            check_patrolling_monster_spawn_rules,
        ),
        ("polar_bear", OnGround, Mbnl, check_polar_bear_spawn_rules),
        ("rabbit", OnGround, Mbnl, check_rabbit_spawn_rules),
        ("sheep", OnGround, Mbnl, check_animal_spawn_rules),
        ("silverfish", OnGround, Mbnl, check_endermite_spawn_rules),
        ("skeleton", OnGround, Mbnl, check_monster_spawn_rules),
        (
            "skeleton_horse",
            OnGround,
            Mbnl,
            check_skeleton_horse_spawn_rules,
        ),
        ("slime", OnGround, Mbnl, check_slime_spawn_rules),
        ("snow_golem", OnGround, Mbnl, check_mob_spawn_rules),
        ("spider", OnGround, Mbnl, check_monster_spawn_rules),
        ("stray", OnGround, Mbnl, check_stray_spawn_rules),
        (
            "parched",
            OnGround,
            Mbnl,
            check_surface_monsters_spawn_rules,
        ),
        ("strider", InLava, Mbnl, check_strider_spawn_rules),
        ("turtle", OnGround, Mbnl, check_turtle_spawn_rules),
        ("villager", OnGround, Mbnl, check_mob_spawn_rules),
        ("witch", OnGround, Mbnl, check_monster_spawn_rules),
        ("wither", OnGround, Mbnl, check_monster_spawn_rules),
        ("wither_skeleton", OnGround, Mbnl, check_monster_spawn_rules),
        ("wolf", OnGround, Mbnl, check_wolf_spawn_rules),
        (
            "zoglin",
            OnGround,
            Mbnl,
            check_any_light_monster_spawn_rules,
        ),
        ("creaking", OnGround, Mbnl, check_monster_spawn_rules),
        ("zombie", OnGround, Mbnl, check_monster_spawn_rules),
        ("zombie_horse", OnGround, Mbnl, check_monster_spawn_rules),
        (
            "zombified_piglin",
            OnGround,
            Mbnl,
            check_zombified_piglin_spawn_rules,
        ),
        ("zombie_villager", OnGround, Mbnl, check_monster_spawn_rules),
        ("cat", OnGround, Mbnl, check_animal_spawn_rules),
        ("elder_guardian", InWater, Mbnl, check_guardian_spawn_rules),
        ("evoker", NoRestrictions, Mbnl, check_monster_spawn_rules),
        ("fox", NoRestrictions, Mbnl, check_fox_spawn_rules),
        (
            "illusioner",
            NoRestrictions,
            Mbnl,
            check_monster_spawn_rules,
        ),
        ("panda", NoRestrictions, Mbnl, check_animal_spawn_rules),
        ("phantom", NoRestrictions, Mbnl, check_mob_spawn_rules),
        ("ravager", OnGround, Mbnl, check_monster_spawn_rules),
        ("shulker", NoRestrictions, Mbnl, check_mob_spawn_rules),
        (
            "trader_llama",
            NoRestrictions,
            Mbnl,
            check_animal_spawn_rules,
        ),
        ("vex", NoRestrictions, Mbnl, check_monster_spawn_rules),
        (
            "vindicator",
            NoRestrictions,
            Mbnl,
            check_monster_spawn_rules,
        ),
        ("wandering_trader", OnGround, Mbnl, check_mob_spawn_rules),
        ("warden", NoRestrictions, Mbnl, check_monster_spawn_rules),
    ]
};

type Registration = (
    &'static str,
    SpawnPlacementType,
    HeightmapType,
    SpawnPredicate,
);

fn registration_for(entity_type: EntityTypeRef) -> Option<&'static Registration> {
    if entity_type.key.namespace != "minecraft" {
        return None;
    }
    PLACEMENT_BY_TYPE
        .iter()
        .find(|(path, ..)| *path == entity_type.key.path)
}

/// Returns vanilla `SpawnPlacements.getPlacementType`.
#[must_use]
pub fn placement_type_for(entity_type: EntityTypeRef) -> SpawnPlacementType {
    registration_for(entity_type).map_or(SpawnPlacementType::NoRestrictions, |(_, p, ..)| *p)
}

/// Returns vanilla `SpawnPlacements.getHeightmapType`.
#[must_use]
pub fn heightmap_type_for(entity_type: EntityTypeRef) -> HeightmapType {
    registration_for(entity_type).map_or_else(default_heightmap_type, |(_, _, h, _)| *h)
}

/// Returns vanilla `SpawnPlacements.checkSpawnRules`.
///
/// The peaceful-difficulty gate applies to every type; the registered predicate only runs
/// for types present in the table, matching vanilla's `data == null || data.predicate.test`.
#[must_use]
pub fn check_spawn_rules(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
    rng: &mut dyn rand::Rng,
) -> bool {
    if !entity_type.allowed_in_peaceful && world.difficulty() == Difficulty::Peaceful {
        return false;
    }
    registration_for(entity_type)
        .is_none_or(|(_, _, _, predicate)| predicate(entity_type, world, spawn_reason, pos, rng))
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
        let mut paths: Vec<&str> = PLACEMENT_BY_TYPE.iter().map(|(path, ..)| *path).collect();
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
            .filter(|(_, _, heightmap, _)| *heightmap == HeightmapType::MotionBlocking)
            .map(|(path, ..)| *path)
            .collect();
        assert_eq!(motion_blocking, ["ocelot", "parrot"]);
    }

    #[test]
    fn strider_is_the_only_lava_placement() {
        let in_lava: Vec<&str> = PLACEMENT_BY_TYPE
            .iter()
            .filter(|(_, placement, ..)| *placement == SpawnPlacementType::InLava)
            .map(|(path, ..)| *path)
            .collect();
        assert_eq!(in_lava, ["strider"]);
    }

    #[test]
    fn every_registered_type_resolves_in_the_entity_registry() {
        // A typo in a table key would silently fall back to NO_RESTRICTIONS at runtime
        // rather than failing, so pin the keys against the generated registry.
        use steel_registry::{REGISTRY, RegistryExt as _, test_support::init_test_registry};
        init_test_registry();
        for (path, ..) in PLACEMENT_BY_TYPE {
            let key = steel_utils::Identifier::vanilla_static(path);
            assert!(
                REGISTRY.entity_types.by_key(&key).is_some(),
                "PLACEMENT_BY_TYPE key `{path}` is not a registered entity type"
            );
        }
    }

    #[test]
    fn unregistered_types_fall_back_to_no_restrictions() {
        // Vanilla's `DATA_BY_TYPE` lookup misses for non-mobs, and both getters have a
        // documented fallback rather than throwing.
        use steel_registry::{REGISTRY, RegistryExt as _, test_support::init_test_registry};
        init_test_registry();
        let arrow = REGISTRY
            .entity_types
            .by_key(&steel_utils::Identifier::vanilla_static("arrow"))
            .expect("arrow is a vanilla entity type");
        assert_eq!(
            super::placement_type_for(arrow),
            SpawnPlacementType::NoRestrictions
        );
        assert_eq!(
            super::heightmap_type_for(arrow),
            HeightmapType::MotionBlockingNoLeaves
        );
    }
}
