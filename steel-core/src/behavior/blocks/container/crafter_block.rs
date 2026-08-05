//! Crafter block behavior.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, FrontAndTop};
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, level_events, vanilla_block_entity_types};
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::blocks::container::spawn_dispensed_item;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BlockEntityTicker;
use crate::block_entity::entities::CrafterBlockEntity;
use crate::inventory::container::{add_item, container_at};
use crate::inventory::lock::ContainerRef;
use crate::inventory::menu::kinds::dispenser as crafter_menu;
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess as _, SignalGetter as _, World};

/// Vanilla `CrafterBlock.MAX_CRAFTING_TICKS`.
const MAX_CRAFTING_TICKS: i32 = 6;
/// Vanilla `CrafterBlock.CRAFTING_TICK_DELAY`.
const CRAFTING_TICK_DELAY: i32 = 4;

/// Vanilla `CrafterBlock` behavior.
#[block_behavior]
pub struct CrafterBlock {
    block: BlockRef,
}

impl CrafterBlock {
    /// Creates a new crafter behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn with_block_entity<R>(
        world: &Arc<World>,
        pos: BlockPos,
        f: impl FnOnce(&CrafterBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        block_entity.downcast_ref::<CrafterBlockEntity>().map(f)
    }

    /// The face the crafter pushes its result out of.
    fn front(state: BlockStateId) -> Direction {
        front_of(state.get_value(&BlockStateProperties::ORIENTATION))
    }

    /// Vanilla `CrafterBlock.dispenseFrom`: craft once and push the result out.
    fn craft_once(state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let Some(input) = Self::with_block_entity(world, pos, CrafterBlockEntity::as_craft_input)
        else {
            return;
        };

        let Some(recipe) = REGISTRY.recipes.find_crafting_recipe(&input) else {
            world.level_event(level_events::SOUND_CRAFTER_FAIL, pos, 0, None);
            return;
        };

        let result = recipe.assemble();
        if result.is_empty() {
            world.level_event(level_events::SOUND_CRAFTER_FAIL, pos, 0, None);
            return;
        }

        Self::with_block_entity(world, pos, |crafter| {
            crafter.set_crafting_ticks_remaining(MAX_CRAFTING_TICKS);
        });
        world.set_block(
            pos,
            state.set_value(&BlockStateProperties::CRAFTING, true),
            UpdateFlags::UPDATE_CLIENTS,
        );

        Self::push_result(world, pos, Self::front(state), result);
        Self::with_block_entity(world, pos, CrafterBlockEntity::consume_ingredients);
    }

    /// Vanilla `CrafterBlock.dispenseItem`: into the container ahead, else thrown out.
    fn push_result(world: &Arc<World>, pos: BlockPos, front: Direction, result: ItemStack) {
        let remaining = match container_at(world, pos.relative(front)) {
            Some(target) => add_item(&target, result),
            None => result,
        };

        if !remaining.is_empty() {
            spawn_dispensed_item(world, pos, front, remaining);
        }
    }
}

/// Vanilla `FrontAndTop.front()`.
const fn front_of(orientation: FrontAndTop) -> Direction {
    match orientation {
        FrontAndTop::DownEast
        | FrontAndTop::DownNorth
        | FrontAndTop::DownSouth
        | FrontAndTop::DownWest => Direction::Down,
        FrontAndTop::UpEast | FrontAndTop::UpNorth | FrontAndTop::UpSouth | FrontAndTop::UpWest => {
            Direction::Up
        }
        FrontAndTop::EastUp => Direction::East,
        FrontAndTop::NorthUp => Direction::North,
        FrontAndTop::SouthUp => Direction::South,
        FrontAndTop::WestUp => Direction::West,
    }
}

/// Vanilla `FrontAndTop.fromFrontAndTop`.
const fn orientation_from(front: Direction, top: Direction) -> FrontAndTop {
    match (front, top) {
        (Direction::Down, Direction::East) => FrontAndTop::DownEast,
        (Direction::Down, Direction::North) => FrontAndTop::DownNorth,
        (Direction::Down, Direction::South) => FrontAndTop::DownSouth,
        (Direction::Down, Direction::West) => FrontAndTop::DownWest,
        (Direction::Up, Direction::East) => FrontAndTop::UpEast,
        (Direction::Up, Direction::North) => FrontAndTop::UpNorth,
        (Direction::Up, Direction::South) => FrontAndTop::UpSouth,
        (Direction::Up, Direction::West) => FrontAndTop::UpWest,
        (Direction::East, _) => FrontAndTop::EastUp,
        (Direction::South, _) => FrontAndTop::SouthUp,
        (Direction::West, _) => FrontAndTop::WestUp,
        (Direction::North | Direction::Down | Direction::Up, _) => FrontAndTop::NorthUp,
    }
}

impl BlockBehavior for CrafterBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let front = context.get_nearest_looking_direction().opposite();
        // A crafter placed against the floor or ceiling keeps its top aligned to the player.
        let top = match front {
            Direction::Down => context.horizontal_direction().opposite(),
            Direction::Up => context.horizontal_direction(),
            _ => Direction::Up,
        };

        Some(
            self.block
                .default_state()
                .set_value(
                    &BlockStateProperties::ORIENTATION,
                    orientation_from(front, top),
                )
                .set_value(
                    &BlockStateProperties::TRIGGERED,
                    context.world.has_neighbor_signal(context.place_pos()),
                ),
        )
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let should_trigger = world.has_neighbor_signal(pos);
        let is_triggered = state.get_value(&BlockStateProperties::TRIGGERED);

        if should_trigger && !is_triggered {
            world.schedule_block_tick_default(pos, self.block, CRAFTING_TICK_DELAY);
            world.set_block(
                pos,
                state.set_value(&BlockStateProperties::TRIGGERED, true),
                UpdateFlags::UPDATE_CLIENTS,
            );
            Self::with_block_entity(world, pos, |crafter| crafter.set_triggered(true));
        } else if !should_trigger && is_triggered {
            world.set_block(
                pos,
                state
                    .set_value(&BlockStateProperties::TRIGGERED, false)
                    .set_value(&BlockStateProperties::CRAFTING, false),
                UpdateFlags::UPDATE_CLIENTS,
            );
            Self::with_block_entity(world, pos, |crafter| crafter.set_triggered(false));
        }
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        Self::craft_once(state, world, pos);
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return InteractionResult::Pass;
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_CRAFTER.msg()),
            move |context| crafter_menu(inventory, context.container_id, container_ref),
        );
        InteractionResult::Success
    }

    fn use_item_on(
        &self,
        _state: BlockStateId,
        _world: &Arc<World>,
        _pos: BlockPos,
        _player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        InteractionResult::TryEmptyHandInteraction
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        let crafter = CrafterBlockEntity::new(level, pos, state);
        crafter.set_triggered(state.get_value(&BlockStateProperties::TRIGGERED));
        BlockEntityCreation::Created(Arc::new(crafter))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::CRAFTER,
        )
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
        world
            .get_block_entity(pos)
            .and_then(|entity| {
                entity
                    .downcast_ref::<CrafterBlockEntity>()
                    .map(CrafterBlockEntity::redstone_signal)
            })
            .unwrap_or(0)
    }
}
