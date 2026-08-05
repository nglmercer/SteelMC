//! Blocks that drop experience when broken (ores other than redstone).

use std::sync::Arc;

use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::enchantment_effect::EnchantmentEffectComponent;
use steel_registry::item_stack::ItemStack;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::{BlockBehavior, BlockPlaceContext};
use crate::world::World;

/// Vanilla `DropExperienceBlock` behavior.
///
/// Vanilla's experience is an `IntProvider`; the extracted block list only ever uses a
/// constant value or a uniform inclusive range, so both forms are carried here and
/// exactly one of them is populated per block.
#[block_behavior]
pub struct DropExperienceBlock {
    block: BlockRef,
    #[json_arg(value, json = "xp_range_value", optional_missing)]
    constant_experience: Option<i32>,
    #[json_arg(value, json = "xp_range_min_inclusive", optional_missing)]
    min_experience: Option<i32>,
    #[json_arg(value, json = "xp_range_max_inclusive", optional_missing)]
    max_experience: Option<i32>,
}

impl DropExperienceBlock {
    /// Creates a new experience-dropping block behavior.
    #[must_use]
    pub const fn new(
        block: BlockRef,
        constant_experience: Option<i32>,
        min_experience: Option<i32>,
        max_experience: Option<i32>,
    ) -> Self {
        Self {
            block,
            constant_experience,
            min_experience,
            max_experience,
        }
    }

    /// Vanilla `IntProvider.sample` for this block's experience range.
    fn sample_experience(&self) -> i32 {
        match (
            self.constant_experience,
            self.min_experience,
            self.max_experience,
        ) {
            (Some(value), _, _) => value,
            (None, Some(min), Some(max)) => rand::rng().random_range(min..=max),
            // Unreachable for extracted vanilla data, which always supplies one form.
            (None, _, _) => 0,
        }
    }
}

impl BlockBehavior for DropExperienceBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        drop_experience: bool,
    ) {
        if !drop_experience {
            return;
        }

        let experience = tool.apply_unconditional_enchantment_value_effects(
            EnchantmentEffectComponent::BlockExperience,
            self.sample_experience() as f32,
        ) as i32;

        if experience > 0 {
            world.pop_experience(pos, experience);
        }
    }
}
