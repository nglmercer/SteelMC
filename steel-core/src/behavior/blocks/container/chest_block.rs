//! Chest, trapped chest and copper chest behaviors.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, ChestType, Direction};
use steel_registry::vanilla_block_entity_types;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_utils::{BlockPos, BlockStateId, axis::Axis, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::blocks::{WeatherState, WeatheringCopper};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::ChestBlockEntity;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::{chest, double_chest};
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Rows shown for a single chest.
const SINGLE_CHEST_ROWS: usize = 3;

/// Which blocks a chest is allowed to pair with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChestPairing {
    /// Pairs only with the exact same block (plain and trapped chests).
    SameBlock,
    /// Pairs with any block in the `copper_chests` tag, regardless of oxidation.
    CopperChest,
}

/// Shared vanilla `ChestBlock` logic, parameterised by which chests may connect.
struct ChestBehavior {
    block: BlockRef,
    kind: ChestPairing,
    block_entity_type: BlockEntityTypeRef,
}

impl ChestBehavior {
    const fn new(
        block: BlockRef,
        kind: ChestPairing,
        block_entity_type: BlockEntityTypeRef,
    ) -> Self {
        Self {
            block,
            kind,
            block_entity_type,
        }
    }

    /// Vanilla `ChestBlock.chestCanConnectTo`.
    fn can_connect_to(&self, state: BlockStateId) -> bool {
        match self.kind {
            ChestPairing::SameBlock => state.get_block() == self.block,
            ChestPairing::CopperChest => {
                state.get_block().has_tag(&BlockTag::COPPER_CHESTS)
                    && state
                        .try_get_value(&BlockStateProperties::CHEST_TYPE)
                        .is_some()
            }
        }
    }

    /// Vanilla `ChestBlock.getConnectedDirection`: which side the paired half sits on.
    fn connected_direction(state: BlockStateId) -> Direction {
        let facing = state.get_value(&BlockStateProperties::HORIZONTAL_FACING);
        if state.get_value(&BlockStateProperties::CHEST_TYPE) == ChestType::Left {
            facing.rotate_y_clockwise()
        } else {
            facing.rotate_y_counter_clockwise()
        }
    }

    /// Vanilla `ChestBlock.candidatePartnerFacing`.
    fn candidate_partner_facing(
        &self,
        world: &dyn LevelReader,
        pos: BlockPos,
        neighbor_direction: Direction,
    ) -> Option<Direction> {
        let state = world.get_block_state(pos.relative(neighbor_direction));
        if self.can_connect_to(state)
            && state.get_value(&BlockStateProperties::CHEST_TYPE) == ChestType::Single
        {
            Some(state.get_value(&BlockStateProperties::HORIZONTAL_FACING))
        } else {
            None
        }
    }

    /// Vanilla `ChestBlock.getChestType`.
    fn chest_type(&self, world: &dyn LevelReader, pos: BlockPos, facing: Direction) -> ChestType {
        if self.candidate_partner_facing(world, pos, facing.rotate_y_clockwise()) == Some(facing) {
            ChestType::Left
        } else if self.candidate_partner_facing(world, pos, facing.rotate_y_counter_clockwise())
            == Some(facing)
        {
            ChestType::Right
        } else {
            ChestType::Single
        }
    }

    fn placement_state(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        let pos = context.place_pos();
        let mut chest_type = ChestType::Single;
        let mut facing = context.horizontal_direction().opposite();

        let secondary_use = context.is_secondary_use_active();
        let clicked_face = context.clicked_face();

        // Sneak-placing against a chest's side forces a pair on that side.
        if clicked_face.get_axis() != Axis::Y && secondary_use {
            let neighbor_facing =
                self.candidate_partner_facing(context.world, pos, clicked_face.opposite());
            if let Some(neighbor_facing) = neighbor_facing
                && neighbor_facing.get_axis() != clicked_face.get_axis()
            {
                facing = neighbor_facing;
                chest_type = if facing.rotate_y_counter_clockwise() == clicked_face.opposite() {
                    ChestType::Right
                } else {
                    ChestType::Left
                };
            }
        }

        if chest_type == ChestType::Single && !secondary_use {
            chest_type = self.chest_type(context.world, pos, facing);
        }

        self.block
            .default_state()
            .set_value(&BlockStateProperties::HORIZONTAL_FACING, facing)
            .set_value(&BlockStateProperties::CHEST_TYPE, chest_type)
            .set_value(
                &BlockStateProperties::WATERLOGGED,
                context.is_water_source(),
            )
    }

    fn shape_update(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);

        if self.can_connect_to(neighbor_state) && direction.get_axis() != Axis::Y {
            let neighbor_type = neighbor_state.get_value(&BlockStateProperties::CHEST_TYPE);
            if state.get_value(&BlockStateProperties::CHEST_TYPE) == ChestType::Single
                && neighbor_type != ChestType::Single
                && state.get_value(&BlockStateProperties::HORIZONTAL_FACING)
                    == neighbor_state.get_value(&BlockStateProperties::HORIZONTAL_FACING)
                && Self::connected_direction(neighbor_state) == direction.opposite()
            {
                let paired_type = match neighbor_type {
                    ChestType::Left => ChestType::Right,
                    ChestType::Right => ChestType::Left,
                    ChestType::Single => ChestType::Single,
                };
                return state.set_value(&BlockStateProperties::CHEST_TYPE, paired_type);
            }
        } else if Self::connected_direction(state) == direction {
            return state.set_value(&BlockStateProperties::CHEST_TYPE, ChestType::Single);
        }

        state
    }

    /// Opens the single or double chest menu, mirroring vanilla's `MENU_PROVIDER_COMBINER`.
    fn open(state: BlockStateId, world: &Arc<World>, pos: BlockPos, player: &Player) {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return;
        };

        let chest_type = state.get_value(&BlockStateProperties::CHEST_TYPE);
        if chest_type == ChestType::Single {
            let inventory = player.inventory.clone();
            player.open_menu(
                TextComponent::translated(translations::CONTAINER_CHEST.msg()),
                move |context| {
                    chest(
                        inventory,
                        context.container_id,
                        container_ref,
                        SINGLE_CHEST_ROWS,
                    )
                },
            );
            return;
        }

        let partner_pos = pos.relative(Self::connected_direction(state));
        let Some(partner_ref) = world
            .get_block_entity(partner_pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return;
        };

        // Vanilla's `CompoundContainer` puts the RIGHT half first.
        let (top, bottom) = if chest_type == ChestType::Right {
            (container_ref, partner_ref)
        } else {
            (partner_ref, container_ref)
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_CHEST_DOUBLE.msg()),
            move |context| double_chest(inventory, context.container_id, top, bottom),
        );
    }

    fn analog_output(world: &dyn LevelReader, pos: BlockPos) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, |container| {
                calculate_redstone_signal_from_container(container)
            })
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::Created(Arc::new(ChestBlockEntity::new(
            self.block_entity_type,
            level,
            pos,
            state,
        )))
    }
}

/// Generates the `BlockBehavior` impl shared by every chest variant.
macro_rules! chest_block_behavior {
    ($ty:ident) => {
        impl BlockBehavior for $ty {
            fn get_state_for_placement(
                &self,
                context: &BlockPlaceContext<'_>,
            ) -> Option<BlockStateId> {
                Some(self.chest.placement_state(context))
            }

            fn update_shape(
                &self,
                state: BlockStateId,
                world: &dyn ScheduledTickAccess,
                pos: BlockPos,
                direction: Direction,
                _neighbor_pos: BlockPos,
                neighbor_state: BlockStateId,
            ) -> BlockStateId {
                self.chest
                    .shape_update(state, world, pos, direction, neighbor_state)
            }

            fn use_without_item(
                &self,
                state: BlockStateId,
                world: &Arc<World>,
                pos: BlockPos,
                player: &Player,
                _hit_result: &BlockHitResult,
                _inv: &mut InventoryAccess,
            ) -> InteractionResult {
                ChestBehavior::open(state, world, pos, player);
                InteractionResult::Success
            }

            fn new_block_entity(
                &self,
                level: Weak<World>,
                pos: BlockPos,
                state: BlockStateId,
            ) -> BlockEntityCreation {
                self.chest.new_block_entity(level, pos, state)
            }

            fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
                true
            }

            fn get_analog_output_signal(
                &self,
                _state: BlockStateId,
                world: &dyn LevelReader,
                pos: BlockPos,
                _direction: Direction,
            ) -> i32 {
                ChestBehavior::analog_output(world, pos)
            }
        }
    };
}

/// Vanilla `ChestBlock` behavior.
///
/// Vanilla's open/close sounds and lid animation go through
/// `ContainerOpenersCounter`, which Steel does not have yet (the barrel has the same gap),
/// so the sound events are carried but not played.
#[block_behavior]
pub struct ChestBlock {
    chest: ChestBehavior,
}

impl ChestBlock {
    /// Creates a new chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::SameBlock,
                &vanilla_block_entity_types::CHEST,
            ),
        }
    }
}

chest_block_behavior!(ChestBlock);

/// Vanilla `TrappedChestBlock` behavior.
///
/// The redstone signal vanilla derives from its open-player count needs
/// `ContainerOpenersCounter`; only the container behavior is implemented here.
#[block_behavior]
pub struct TrappedChestBlock {
    chest: ChestBehavior,
}

impl TrappedChestBlock {
    /// Creates a new trapped chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::SameBlock,
                &vanilla_block_entity_types::TRAPPED_CHEST,
            ),
        }
    }
}

chest_block_behavior!(TrappedChestBlock);

/// Vanilla `CopperChestBlock` behavior (the waxed variants).
///
/// Vanilla additionally rewrites a newly placed copper chest to the least oxidized of the
/// two connected halves; that mapping needs the copper-block weathering chain and is not
/// applied here.
#[block_behavior]
pub struct CopperChestBlock {
    chest: ChestBehavior,
}

impl CopperChestBlock {
    /// Creates a new copper chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::CopperChest,
                &vanilla_block_entity_types::CHEST,
            ),
        }
    }
}

chest_block_behavior!(CopperChestBlock);

/// Vanilla `WeatheringCopperChestBlock` behavior (the unwaxed variants).
#[block_behavior]
pub struct WeatheringCopperChestBlock {
    chest: ChestBehavior,
    #[json_arg(r#enum = "WeatherState", json = "weather_state")]
    weathering: WeatheringCopper,
}

impl WeatheringCopperChestBlock {
    /// Creates a new weathering copper chest behavior.
    #[must_use]
    pub const fn new(block: BlockRef, weather_state: WeatherState) -> Self {
        Self {
            chest: ChestBehavior::new(
                block,
                ChestPairing::CopperChest,
                &vanilla_block_entity_types::CHEST,
            ),
            weathering: WeatheringCopper::new(weather_state),
        }
    }
}

impl BlockBehavior for WeatheringCopperChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.chest.placement_state(context))
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        self.chest
            .shape_update(state, world, pos, direction, neighbor_state)
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        ChestBehavior::open(state, world, pos, player);
        InteractionResult::Success
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.weathering.change_over_time(state, world, pos);
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        self.chest.new_block_entity(level, pos, state)
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        ChestBehavior::analog_output(world, pos)
    }
}
