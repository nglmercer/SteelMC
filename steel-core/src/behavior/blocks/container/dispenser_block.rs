//! Dispenser and dropper behaviors.

use std::sync::{Arc, Weak};

use glam::DVec3;
use rand::RngExt as _;
use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::item_stack::ItemStack;
use steel_registry::level_events;
use steel_registry::vanilla_block_entity_types;
use steel_registry::vanilla_entities;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId, Direction, Downcast as _, axis::Axis, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::DispenserBlockEntity;
use crate::entity::entities::ItemEntity;
use crate::entity::next_entity_id;
use crate::inventory::container::{
    add_item, calculate_redstone_signal_from_container, container_at,
};
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::dispenser as dispenser_menu;
use crate::player::Player;
use crate::world::{
    LevelReader, ScheduledTickAccess as _, SignalGetter as _, World, triangle_random,
};

/// Vanilla `DispenserBlock.TRIGGER_DURATION`.
const TRIGGER_DURATION: i32 = 4;
/// Vanilla `DefaultDispenseItemBehavior.DEFAULT_ACCURACY`.
const DEFAULT_ACCURACY: f64 = 6.0;
/// Vanilla's per-accuracy-unit velocity spread.
const ACCURACY_SPREAD: f64 = 0.017_227_5;
/// How far in front of the block face vanilla spawns the dispensed item.
const DISPENSE_OFFSET: f64 = 0.7;

/// Vanilla `DispenserBlock.getDispensePosition`.
fn dispense_position(pos: BlockPos, facing: Direction) -> DVec3 {
    let center = DVec3::new(
        f64::from(pos.x()) + 0.5,
        f64::from(pos.y()) + 0.5,
        f64::from(pos.z()) + 0.5,
    );
    let step = facing.offset_vec();
    center
        + DVec3::new(
            DISPENSE_OFFSET * f64::from(step.x),
            DISPENSE_OFFSET * f64::from(step.y),
            DISPENSE_OFFSET * f64::from(step.z),
        )
}

/// Vanilla `DefaultDispenseItemBehavior.spawnItem`: throws one item out of the face.
pub(crate) fn spawn_dispensed_item(
    world: &Arc<World>,
    pos: BlockPos,
    facing: Direction,
    stack: ItemStack,
) {
    let mut spawn = dispense_position(pos, facing);
    // Vanilla drops the spawn point slightly so the item clears the block face.
    spawn.y -= if facing.get_axis() == Axis::Y {
        0.125
    } else {
        0.156_25
    };

    let step = facing.offset_vec();
    let power = rand::rng().random::<f64>().mul_add(0.1, 0.2);
    let spread = ACCURACY_SPREAD * DEFAULT_ACCURACY;
    let velocity = DVec3::new(
        triangle_random(f64::from(step.x) * power, spread),
        triangle_random(0.2, spread),
        triangle_random(f64::from(step.z) * power, spread),
    );

    let entity = Arc::new(ItemEntity::with_item_and_velocity(
        &vanilla_entities::ITEM,
        next_entity_id(),
        spawn,
        stack,
        velocity,
        Arc::downgrade(world),
    ));
    if let Err(error) = world.try_add_entity(entity) {
        log::warn!("Failed to spawn dispensed item: {error}");
    }
}

/// Shared vanilla `DispenserBlock` logic; the dropper only differs in how it dispenses.
struct DispenserBehavior {
    block: BlockRef,
    block_entity_type: BlockEntityTypeRef,
    /// Whether this block pushes into a neighboring container like a dropper.
    is_dropper: bool,
}

impl DispenserBehavior {
    const fn new(block: BlockRef, block_entity_type: BlockEntityTypeRef, is_dropper: bool) -> Self {
        Self {
            block,
            block_entity_type,
            is_dropper,
        }
    }

    fn with_block_entity<R>(
        world: &Arc<World>,
        pos: BlockPos,
        f: impl FnOnce(&DispenserBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        block_entity.downcast_ref::<DispenserBlockEntity>().map(f)
    }

    /// Vanilla `DispenserBlock.dispenseFrom`.
    fn dispense_from(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let Some(slot) =
            Self::with_block_entity(world, pos, DispenserBlockEntity::random_occupied_slot)
                .flatten()
        else {
            world.level_event(level_events::SOUND_DISPENSER_FAIL, pos, 0, None);
            return;
        };

        let Some(stack) = Self::with_block_entity(world, pos, |dispenser| dispenser.item(slot))
        else {
            return;
        };
        if stack.is_empty() {
            return;
        }

        let facing = state.get_value(&BlockStateProperties::FACING);
        let remaining = if self.is_dropper {
            Self::drop_one(world, pos, facing, stack)
        } else {
            Self::dispense_one(world, pos, facing, stack)
        };

        Self::with_block_entity(world, pos, |dispenser| dispenser.set_item(slot, remaining));

        world.level_event(level_events::SOUND_DISPENSER_DISPENSE, pos, 0, None);
        world.level_event(
            level_events::PARTICLES_SHOOT_SMOKE,
            pos,
            facing as i32,
            None,
        );
    }

    /// Vanilla `DefaultDispenseItemBehavior.execute`: eject a single item.
    fn dispense_one(
        world: &Arc<World>,
        pos: BlockPos,
        facing: Direction,
        mut stack: ItemStack,
    ) -> ItemStack {
        let thrown = stack.split(1);
        spawn_dispensed_item(world, pos, facing, thrown);
        stack
    }

    /// Vanilla `DropperBlock.dispenseFrom`: push into the container in front, else eject.
    fn drop_one(
        world: &Arc<World>,
        pos: BlockPos,
        facing: Direction,
        mut stack: ItemStack,
    ) -> ItemStack {
        let Some(target) = container_at(world, pos.relative(facing)) else {
            return Self::dispense_one(world, pos, facing, stack);
        };

        let mut single = stack.clone();
        single.set_count(1);
        if add_item(&target, single).is_empty() {
            stack.set_count(stack.count() - 1);
        }
        stack
    }

    fn placement_state(&self, context: &BlockPlaceContext<'_>) -> BlockStateId {
        self.block.default_state().set_value(
            &BlockStateProperties::FACING,
            context.get_nearest_looking_direction().opposite(),
        )
    }

    /// Vanilla `DispenserBlock.neighborChanged`: arm on a rising redstone edge.
    fn neighbor_changed(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let should_trigger =
            world.has_neighbor_signal(pos) || world.has_neighbor_signal(pos.above());
        let is_triggered = state.get_value(&BlockStateProperties::TRIGGERED);

        if should_trigger && !is_triggered {
            world.schedule_block_tick_default(pos, self.block, TRIGGER_DURATION);
            world.set_block(
                pos,
                state.set_value(&BlockStateProperties::TRIGGERED, true),
                UpdateFlags::UPDATE_CLIENTS,
            );
        } else if !should_trigger && is_triggered {
            world.set_block(
                pos,
                state.set_value(&BlockStateProperties::TRIGGERED, false),
                UpdateFlags::UPDATE_CLIENTS,
            );
        }
    }

    fn open(world: &Arc<World>, pos: BlockPos, player: &Player) -> InteractionResult {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return InteractionResult::Pass;
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_DISPENSER.msg()),
            move |context| dispenser_menu(inventory, context.container_id, container_ref),
        );
        InteractionResult::Success
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
        BlockEntityCreation::Created(Arc::new(DispenserBlockEntity::new(
            self.block_entity_type,
            level,
            pos,
            state,
        )))
    }
}

/// Generates the `BlockBehavior` impl shared by the dispenser and the dropper.
macro_rules! dispenser_block_behavior {
    ($ty:ident) => {
        impl BlockBehavior for $ty {
            fn get_state_for_placement(
                &self,
                context: &BlockPlaceContext<'_>,
            ) -> Option<BlockStateId> {
                Some(self.dispenser.placement_state(context))
            }

            fn handle_neighbor_changed(
                &self,
                state: BlockStateId,
                world: &Arc<World>,
                pos: BlockPos,
                _source_block: BlockRef,
                _moved_by_piston: bool,
            ) {
                self.dispenser.neighbor_changed(state, world, pos);
            }

            fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
                self.dispenser.dispense_from(state, world, pos);
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
                DispenserBehavior::open(world, pos, player)
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
                self.dispenser.new_block_entity(level, pos, state)
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
                DispenserBehavior::analog_output(world, pos)
            }
        }
    };
}

/// Vanilla `DispenserBlock` behavior.
///
/// Vanilla dispatches through a per-item `DISPENSER_REGISTRY` so that arrows are shot,
/// buckets are emptied and so on. Steel only implements vanilla's
/// `DefaultDispenseItemBehavior` fallback so far, which ejects the item as an entity;
/// item-specific behaviors register on top of it exactly as they do in vanilla.
#[block_behavior]
pub struct DispenserBlock {
    dispenser: DispenserBehavior,
}

impl DispenserBlock {
    /// Creates a new dispenser behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            dispenser: DispenserBehavior::new(block, &vanilla_block_entity_types::DISPENSER, false),
        }
    }
}

dispenser_block_behavior!(DispenserBlock);

/// Vanilla `DropperBlock` behavior.
#[block_behavior]
pub struct DropperBlock {
    dispenser: DispenserBehavior,
}

impl DropperBlock {
    /// Creates a new dropper behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            dispenser: DispenserBehavior::new(block, &vanilla_block_entity_types::DROPPER, true),
        }
    }
}

dispenser_block_behavior!(DropperBlock);
