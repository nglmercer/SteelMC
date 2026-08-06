use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::entity::projectile::Projectile;
use crate::world::{ClipHitResult, LevelReader, World};

use super::{BlockRef, default_surviving_state};
use crate::behavior::blocks::vegetation::chorus_plant_block::ChorusPlantBlock;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::level_events;
use steel_utils::types::UpdateFlags;

/// Vanilla `ChorusFlowerBlock` dead age (`AGE` 5).
const DEAD_AGE: u8 = 5;

const HORIZONTAL_DIRECTIONS: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];

/// Vanilla `ChorusFlowerBlock` survival behavior.
#[block_behavior]
pub struct ChorusFlowerBlock {
    block: BlockRef,
    #[json_arg(vanilla_blocks)]
    plant: BlockRef,
}

impl ChorusFlowerBlock {
    /// Creates a new chorus flower block behavior.
    #[must_use]
    pub const fn new(block: BlockRef, plant: BlockRef) -> Self {
        Self { block, plant }
    }

    fn projectile_can_break(projectile: &dyn Projectile, world: &World, pos: BlockPos) -> bool {
        projectile.projectile_may_interact(world, pos) && projectile.may_break(world)
    }

    /// Vanilla `ChorusFlowerBlock.allNeighborsEmpty`.
    fn all_neighbors_empty(world: &Arc<World>, pos: BlockPos, ignore: Option<Direction>) -> bool {
        HORIZONTAL_DIRECTIONS.iter().all(|direction| {
            Some(*direction) == ignore || world.get_block_state(pos.relative(*direction)).is_air()
        })
    }

    /// Vanilla `ChorusFlowerBlock.placeGrownFlower`.
    fn place_grown_flower(&self, world: &Arc<World>, pos: BlockPos, age: u8) {
        world.set_block(
            pos,
            self.block
                .default_state()
                .set_value(&BlockStateProperties::AGE_5, age),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.level_event(level_events::SOUND_CHORUS_GROW, pos, 0, None);
    }

    /// Vanilla `ChorusFlowerBlock.placeDeadFlower`.
    fn place_dead_flower(&self, world: &Arc<World>, pos: BlockPos) {
        world.set_block(
            pos,
            self.block
                .default_state()
                .set_value(&BlockStateProperties::AGE_5, DEAD_AGE),
            UpdateFlags::UPDATE_CLIENTS,
        );
        world.level_event(level_events::SOUND_CHORUS_DEATH, pos, 0, None);
    }

    /// Replaces this flower with a connected plant segment.
    fn convert_to_plant(&self, world: &Arc<World>, pos: BlockPos) {
        let plant_state = ChorusPlantBlock::state_with_connections(
            world.as_ref(),
            pos,
            self.plant.default_state(),
        );
        world.set_block(pos, plant_state, UpdateFlags::UPDATE_CLIENTS);
    }
}

impl BlockBehavior for ChorusFlowerBlock {
    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let below_state = world.get_block_state(pos.below());
        if below_state.get_block() == self.plant
            || below_state
                .get_block()
                .has_tag(&BlockTag::SUPPORTS_CHORUS_FLOWER)
        {
            return true;
        }

        if !below_state.is_air() {
            return false;
        }

        let mut has_single_plant_neighbor = false;
        for direction in HORIZONTAL_DIRECTIONS {
            let neighbor_state = world.get_block_state(pos.relative(direction));
            if neighbor_state.get_block() == self.plant {
                if has_single_plant_neighbor {
                    return false;
                }
                has_single_plant_neighbor = true;
            } else if !neighbor_state.is_air() {
                return false;
            }
        }

        has_single_plant_neighbor
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        // Vanilla `ChorusFlowerBlock.randomTick`.
        let above = pos.above();
        if !world.get_block_state(above).is_air() || above.y() > world.max_build_height() {
            return;
        }

        let age: u8 = state.get_value(&BlockStateProperties::AGE_5);
        if age >= DEAD_AGE {
            return;
        }

        // Decide whether the stalk may extend straight up, and how tall it already is.
        let below_state = world.get_block_state(pos.below());
        let mut grow_upwards = false;
        let mut pillar_on_support_block = false;

        if below_state
            .get_block()
            .has_tag(&BlockTag::SUPPORTS_CHORUS_FLOWER)
        {
            grow_upwards = true;
        } else if below_state.get_block() == self.plant {
            let mut height = 1;
            for _ in 0..4 {
                let test = world.get_block_state(pos.offset(0, -(height + 1), 0));
                if test.get_block() != self.plant {
                    if test.get_block().has_tag(&BlockTag::SUPPORTS_CHORUS_FLOWER) {
                        pillar_on_support_block = true;
                    }
                    break;
                }
                height += 1;
            }

            // Taller pillars are progressively less likely to keep growing upward.
            let limit = if pillar_on_support_block { 5 } else { 4 };
            if height < 2 || height <= rand::random_range(0..limit) {
                grow_upwards = true;
            }
        } else if below_state.is_air() {
            grow_upwards = true;
        }

        if grow_upwards
            && Self::all_neighbors_empty(world, above, None)
            && world.get_block_state(pos.offset(0, 2, 0)).is_air()
        {
            self.convert_to_plant(world, pos);
            self.place_grown_flower(world, above, age);
            return;
        }

        if age >= DEAD_AGE - 1 {
            self.place_dead_flower(world, pos);
            return;
        }

        // Otherwise try to branch sideways.
        let mut branch_attempts = rand::random_range(0..4);
        if pillar_on_support_block {
            branch_attempts += 1;
        }

        let mut created_branch = false;
        for _ in 0..branch_attempts {
            let direction =
                HORIZONTAL_DIRECTIONS[rand::random_range(0..HORIZONTAL_DIRECTIONS.len())];
            let target = pos.relative(direction);
            if world.get_block_state(target).is_air()
                && world.get_block_state(target.below()).is_air()
                && Self::all_neighbors_empty(world, target, Some(direction.opposite()))
            {
                self.place_grown_flower(world, target, age + 1);
                created_branch = true;
            }
        }

        if created_branch {
            self.convert_to_plant(world, pos);
        } else {
            self.place_dead_flower(world, pos);
        }
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        default_surviving_state(self.block, self, context)
    }

    fn on_projectile_hit(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        hit: &ClipHitResult,
        projectile: &dyn Projectile,
    ) {
        if Self::projectile_can_break(projectile, world, hit.block_pos) {
            world.destroy_block_by_entity(hit.block_pos, true, projectile.as_entity_event_source());
        }
    }
}
