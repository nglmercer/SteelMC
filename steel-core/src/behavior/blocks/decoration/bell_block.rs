//! Bell block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BellAttachType, BlockStateProperties};
use steel_registry::blocks::shapes::SupportType;
use steel_registry::vanilla_block_entity_types;
use steel_registry::vanilla_blocks;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _, axis::Axis};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BlockEntityTicker;
use crate::block_entity::entities::BellBlockEntity;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, SignalGetter as _, World};

/// Above this height on a side face, vanilla treats the click as hitting the post
/// rather than the bell itself.
const BELL_TOP: f64 = 0.812_4;

/// Vanilla `BellBlock` behavior.
///
/// Ringing a bell in vanilla also reveals nearby raiders; that needs the raid system.
#[block_behavior]
pub struct BellBlock {
    block: BlockRef,
}

impl BellBlock {
    /// Creates a new bell behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `BellBlock.getConnectedDirection`: the side the bell hangs from.
    fn connected_direction(state: BlockStateId) -> Direction {
        match state.get_value(&BlockStateProperties::BELL_ATTACHMENT) {
            BellAttachType::Floor => Direction::Up,
            BellAttachType::Ceiling => Direction::Down,
            BellAttachType::SingleWall | BellAttachType::DoubleWall => state
                .get_value(&BlockStateProperties::HORIZONTAL_FACING)
                .opposite(),
        }
    }

    fn can_survive_at(state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        let attach_direction = Self::connected_direction(state).opposite();
        let support_pos = pos.relative(attach_direction);
        let support_state = world.get_block_state(support_pos);

        if attach_direction == Direction::Up {
            return world.is_face_sturdy_for(
                support_state,
                support_pos,
                Direction::Down,
                SupportType::Center,
            );
        }

        world.is_face_sturdy(support_state, support_pos, attach_direction.opposite())
    }

    fn is_face_sturdy_at(world: &dyn LevelReader, pos: BlockPos, direction: Direction) -> bool {
        world.is_face_sturdy(world.get_block_state(pos), pos, direction)
    }

    /// Vanilla `BellBlock.isProperHit`: only the bell body rings, not the support post.
    fn is_proper_hit(state: BlockStateId, hit_result: &BlockHitResult) -> bool {
        let clicked = hit_result.direction;
        let click_y = hit_result.location.y - f64::from(hit_result.block_pos.y());
        if clicked.get_axis() == Axis::Y || click_y > BELL_TOP {
            return false;
        }

        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        match state.get_value(&BlockStateProperties::BELL_ATTACHMENT) {
            BellAttachType::Floor => facing.get_axis() == clicked.get_axis(),
            BellAttachType::SingleWall | BellAttachType::DoubleWall => {
                facing.get_axis() != clicked.get_axis()
            }
            BellAttachType::Ceiling => true,
        }
    }

    /// Vanilla `BellBlock.attemptToRing`.
    fn attempt_to_ring(
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        direction: Option<Direction>,
    ) -> bool {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return false;
        };
        let Some(bell) = block_entity.downcast_ref::<BellBlockEntity>() else {
            return false;
        };

        let direction =
            direction.unwrap_or_else(|| state.get_value(&BlockStateProperties::HORIZONTAL_FACING));
        bell.on_hit(direction);
        true
    }
}

impl BlockBehavior for BellBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let clicked_face = context.clicked_face();
        let pos = context.place_pos();
        let world = context.world;

        if clicked_face.get_axis() == Axis::Y {
            let attachment = if clicked_face == Direction::Down {
                BellAttachType::Ceiling
            } else {
                BellAttachType::Floor
            };
            let state = self
                .block
                .default_state()
                .set_value(&BlockStateProperties::BELL_ATTACHMENT, attachment)
                .set_value(
                    &BlockStateProperties::HORIZONTAL_FACING,
                    context.horizontal_direction(),
                );
            return Self::can_survive_at(state, world, pos).then_some(state);
        }

        // A bell wedged between two walls hangs from both of them.
        let axis = clicked_face.get_axis();
        let double_attached = match axis {
            Axis::X => {
                Self::is_face_sturdy_at(world, pos.west(), Direction::East)
                    && Self::is_face_sturdy_at(world, pos.east(), Direction::West)
            }
            Axis::Z => {
                Self::is_face_sturdy_at(world, pos.north(), Direction::South)
                    && Self::is_face_sturdy_at(world, pos.south(), Direction::North)
            }
            Axis::Y => false,
        };

        let wall_state = self
            .block
            .default_state()
            .set_value(
                &BlockStateProperties::HORIZONTAL_FACING,
                clicked_face.opposite(),
            )
            .set_value(
                &BlockStateProperties::BELL_ATTACHMENT,
                if double_attached {
                    BellAttachType::DoubleWall
                } else {
                    BellAttachType::SingleWall
                },
            );
        if Self::can_survive_at(wall_state, world, pos) {
            return Some(wall_state);
        }

        // Fall back to standing on the floor, or hanging from the ceiling.
        let can_attach_below = Self::is_face_sturdy_at(world, pos.below(), Direction::Up);
        let fallback = wall_state.set_value(
            &BlockStateProperties::BELL_ATTACHMENT,
            if can_attach_below {
                BellAttachType::Floor
            } else {
                BellAttachType::Ceiling
            },
        );
        Self::can_survive_at(fallback, world, pos).then_some(fallback)
    }

    fn can_survive(&self, state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::can_survive_at(state, world, pos)
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
        let attachment = state.get_value(&BlockStateProperties::BELL_ATTACHMENT);
        let connected = Self::connected_direction(state).opposite();

        if connected == direction
            && !Self::can_survive_at(state, world, pos)
            && attachment != BellAttachType::DoubleWall
        {
            return vanilla_blocks::AIR.default_state();
        }

        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        if direction.get_axis() == facing.get_axis() {
            if attachment == BellAttachType::DoubleWall
                && !world.is_face_sturdy(neighbor_state, neighbor_pos, direction)
            {
                return state
                    .set_value(
                        &BlockStateProperties::BELL_ATTACHMENT,
                        BellAttachType::SingleWall,
                    )
                    .set_value(
                        &BlockStateProperties::HORIZONTAL_FACING,
                        direction.opposite(),
                    );
            }

            if attachment == BellAttachType::SingleWall
                && connected.opposite() == direction
                && world.is_face_sturdy(neighbor_state, neighbor_pos, facing)
            {
                return state.set_value(
                    &BlockStateProperties::BELL_ATTACHMENT,
                    BellAttachType::DoubleWall,
                );
            }
        }

        state
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let signal = world.has_neighbor_signal(pos);
        if signal == state.get_value(&BlockStateProperties::POWERED) {
            return;
        }

        if signal {
            Self::attempt_to_ring(state, world, pos, None);
        }

        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::POWERED, signal),
            UpdateFlags::UPDATE_ALL,
        );
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !Self::is_proper_hit(state, hit_result) {
            return InteractionResult::Pass;
        }

        Self::attempt_to_ring(state, world, pos, Some(hit_result.direction));
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(BellBlockEntity::new(level, pos, state)))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::BELL,
        )
    }
}
