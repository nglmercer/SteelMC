//! Snow-aware ground blocks: podzol, grass block and mycelium.

use std::sync::Arc;

use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::chunk::light::get_light_block_into;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Whether the block renders its snowy side texture.
const SNOWY: BoolProperty = BlockStateProperties::SNOWY;

/// Light dampening at or above which spreading grass dies out.
const LETHAL_LIGHT_DAMPENING: u8 = 15;
/// Minimum brightness above a block for grass or mycelium to spread.
const MIN_SPREAD_BRIGHTNESS: u8 = 9;
/// Vanilla spread attempts per random tick.
const SPREAD_ATTEMPTS: u32 = 4;

/// Vanilla `SnowyBlock` behavior.
#[block_behavior]
pub struct SnowyBlock {
    block: BlockRef,
}

impl SnowyBlock {
    /// Creates a new snowy block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `SnowyBlock.isSnowySetting`.
    #[must_use]
    pub fn is_snowy_setting(above_state: BlockStateId) -> bool {
        above_state.get_block().has_tag(&BlockTag::SNOW)
    }

    fn placement_state(block: BlockRef, context: &BlockPlaceContext<'_>) -> BlockStateId {
        let above_state = context.world.get_block_state(context.place_pos().above());
        block
            .default_state()
            .set_value(&SNOWY, Self::is_snowy_setting(above_state))
    }

    fn shape_update(
        state: BlockStateId,
        direction: Direction,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if direction == Direction::Up {
            state.set_value(&SNOWY, Self::is_snowy_setting(neighbor_state))
        } else {
            state
        }
    }
}

impl BlockBehavior for SnowyBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(Self::placement_state(self.block, context))
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        _world: &dyn ScheduledTickAccess,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        Self::shape_update(state, direction, neighbor_state)
    }
}

/// Vanilla `SpreadingSnowyBlock`: the shared grass block / mycelium spreading rules.
///
/// Not registered on its own; vanilla's class is abstract.
pub(super) struct SpreadingSnowyBlock {
    block: BlockRef,
    /// The block this reverts to when it can no longer stay alive.
    base_block: BlockRef,
}

impl SpreadingSnowyBlock {
    /// Creates a spreading snowy block behavior reverting to `base_block`.
    #[must_use]
    pub const fn new(block: BlockRef, base_block: BlockRef) -> Self {
        Self { block, base_block }
    }

    /// Vanilla `SpreadingSnowyBlock.canBeGrass` (`canStayAlive`).
    fn can_stay_alive(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let above_state = world.get_block_state(pos.above());

        if above_state.get_block() == &vanilla_blocks::SNOW
            && above_state.get_value(&BlockStateProperties::LAYERS) == 1
        {
            return true;
        }

        if above_state.get_fluid_state().is_full() {
            return false;
        }

        get_light_block_into(
            state,
            above_state,
            Direction::Up,
            above_state.get_light_dampening(),
        ) < LETHAL_LIGHT_DAMPENING
    }

    /// Vanilla `SpreadingSnowyBlock.canPropagate`.
    fn can_propagate(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::can_stay_alive(state, world, pos)
            && !world
                .get_block_state(pos.above())
                .get_fluid_state()
                .fluid_id
                .has_tag(&FluidTag::WATER)
    }

    /// Vanilla `SpreadingSnowyBlock.randomTick`.
    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !Self::can_stay_alive(state, world.as_ref(), pos) {
            world.set_block(
                pos,
                self.base_block.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
            return;
        }

        if world.max_local_raw_brightness(pos.above(), 0) < MIN_SPREAD_BRIGHTNESS {
            return;
        }

        let spread_state = self.block.default_state();
        let mut rng = rand::rng();

        for _ in 0..SPREAD_ATTEMPTS {
            let target = pos.offset(
                rng.random_range(0..3) - 1,
                rng.random_range(0..5) - 3,
                rng.random_range(0..3) - 1,
            );

            if world.get_block_state(target).get_block() != self.base_block
                || !Self::can_propagate(spread_state, world.as_ref(), target)
            {
                continue;
            }

            let above_state = world.get_block_state(target.above());
            world.set_block(
                target,
                spread_state.set_value(&SNOWY, SnowyBlock::is_snowy_setting(above_state)),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }
}

/// Vanilla `GrassBlock` behavior.
///
/// Bonemealing a grass block places vegetation features at runtime, which Steel cannot
/// do yet (no live `PlacedFeature` placement outside chunk generation), so this behavior
/// covers spreading and the snowy state only.
#[block_behavior]
pub struct GrassBlock {
    snowy: SnowyBlock,
    spreading: SpreadingSnowyBlock,
}

impl GrassBlock {
    /// Creates a new grass block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            snowy: SnowyBlock::new(block),
            spreading: SpreadingSnowyBlock::new(block, &vanilla_blocks::DIRT),
        }
    }
}

impl BlockBehavior for GrassBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.snowy.get_state_for_placement(context)
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.snowy
            .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.spreading.random_tick(state, world, pos);
    }
}

/// Vanilla `MyceliumBlock` behavior.
#[block_behavior]
pub struct MyceliumBlock {
    snowy: SnowyBlock,
    spreading: SpreadingSnowyBlock,
}

impl MyceliumBlock {
    /// Creates a new mycelium block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            snowy: SnowyBlock::new(block),
            spreading: SpreadingSnowyBlock::new(block, &vanilla_blocks::DIRT),
        }
    }
}

impl BlockBehavior for MyceliumBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.snowy.get_state_for_placement(context)
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.snowy
            .update_shape(state, world, pos, direction, neighbor_pos, neighbor_state)
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.spreading.random_tick(state, world, pos);
    }
}
