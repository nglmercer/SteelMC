//! Pumpkin block behavior.

use std::sync::Arc;

use glam::DVec3;
use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::item_stack::ItemStack;
use steel_registry::loot_table::LootContext;
use steel_registry::{
    sound_events, vanilla_blocks, vanilla_entities, vanilla_game_events, vanilla_items,
    vanilla_loot_tables,
};
use steel_utils::random::RandomSource;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction, axis::Axis};

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::entity::entities::ItemEntity;
use crate::entity::next_entity_id;
use crate::entity::{Entity as _, entity_loot_ref};
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

/// Vanilla offsets the carved seeds this far along the carved face.
const SEED_SPAWN_OFFSET: f64 = 0.65;

/// Vanilla `PumpkinBlock` behavior: shears carve it into a carved pumpkin.
#[block_behavior]
pub struct PumpkinBlock {
    block: BlockRef,
}

impl PumpkinBlock {
    /// Creates a new pumpkin behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla spawns the seeds just outside the carved face with a small push.
    fn spawn_seeds(world: &Arc<World>, pos: BlockPos, direction: Direction, seeds: ItemStack) {
        let step = direction.offset_vec();
        let spawn = DVec3::new(
            f64::from(pos.x()) + 0.5 + f64::from(step.x) * SEED_SPAWN_OFFSET,
            f64::from(pos.y()) + 0.1,
            f64::from(pos.z()) + 0.5 + f64::from(step.z) * SEED_SPAWN_OFFSET,
        );
        let mut rng = rand::rng();
        let velocity = DVec3::new(
            0.05 * f64::from(step.x) + rng.random::<f64>() * 0.02,
            0.05,
            0.05 * f64::from(step.z) + rng.random::<f64>() * 0.02,
        );

        let entity = Arc::new(ItemEntity::with_item_and_velocity(
            &vanilla_entities::ITEM,
            next_entity_id(),
            spawn,
            seeds,
            velocity,
            Arc::downgrade(world),
        ));
        if let Err(error) = world.try_add_entity(entity) {
            log::warn!("Failed to spawn carved pumpkin seeds: {error}");
        }
    }
}

impl BlockBehavior for PumpkinBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hand: InteractionHand,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !inv.with_item(|held| held.is(&vanilla_items::SHEARS)) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        // Carving a top or bottom face orients the face towards the player instead.
        let clicked = hit_result.direction;
        let direction = if clicked.get_axis() == Axis::Y {
            let (rotation, _) = player.rotation();
            Direction::from_yaw(rotation).opposite()
        } else {
            clicked
        };

        let seeds = inv.with_item(|tool| {
            let mut rng = RandomSource::create_thread_safe();
            let mut context = LootContext::new(&mut rng)
                .with_block_state(state)
                .with_tool(tool)
                .with_interacting_entity(entity_loot_ref(player));
            vanilla_loot_tables::CARVE_PUMPKIN.get_random_items(&mut context)
        });
        for seed_stack in seeds {
            Self::spawn_seeds(world, pos, direction, seed_stack);
        }

        world.play_sound(
            &sound_events::BLOCK_PUMPKIN_CARVE,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        );
        world.set_block(
            pos,
            vanilla_blocks::CARVED_PUMPKIN
                .default_state()
                .set_value(&BlockStateProperties::HORIZONTAL_FACING, direction),
            UpdateFlags::UPDATE_ALL | UpdateFlags::UPDATE_IMMEDIATE,
        );

        let has_infinite_materials = player.has_infinite_materials();
        inv.with_inventory(|inventory| {
            inventory.hurt_item_in_hand(hand, 1, has_infinite_materials);
        });
        world.game_event(
            &vanilla_game_events::SHEAR,
            pos,
            &GameEventContext::new(Some(player), None),
        );
        InteractionResult::Success
    }
}
