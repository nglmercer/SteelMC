//! Falling block entity (sand, gravel, concrete powder, dragon eggs).

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::FallingBlockEntityData;
use steel_registry::vanilla_fluid_tags::FluidTag;
use steel_registry::{REGISTRY, RegistryExt as _, vanilla_blocks, vanilla_entities};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::behavior::BLOCK_BEHAVIORS;
use crate::behavior::blocks::falling_block_is_free;
use crate::block_entity::block_state_nbt;
use crate::entity::callback::RemovalReason;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, SharedEntity, next_entity_id,
};
use crate::fluid::state::fluid_state_to_block;
use crate::physics::MoverType;
use crate::world::{LevelReader as _, World};

/// Vanilla `FallingBlockEntity.getDefaultGravity`.
const FALLING_BLOCK_GRAVITY: f64 = 0.04;
/// Ticks after which a block falling outside the world is dropped.
const OUT_OF_WORLD_TIMEOUT: i32 = 100;
/// Ticks after which any falling block is dropped regardless of position.
const MAX_LIFETIME: i32 = 600;
/// Vanilla's horizontal/vertical velocity damping applied on landing.
const LANDING_DAMPING: DVec3 = DVec3::new(0.7, -0.5, 0.7);

struct FallingBlockState {
    /// The block state being carried.
    block_state: BlockStateId,
    /// Ticks since the entity started falling.
    time: i32,
    /// Whether the block drops as an item when it cannot be placed.
    drop_item: bool,
    /// Whether landing should destroy the block instead of placing it.
    cancel_drop: bool,
}

/// Vanilla `FallingBlockEntity`.
///
/// Anvil fall damage (`hurtEntities`) and carrying a block entity's NBT (`blockData`) are
/// not modelled: no block registered as falling today uses either.
#[entity_behavior(class = "FallingBlockEntity")]
pub struct FallingBlockEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<FallingBlockEntityData>,
    state: SyncMutex<FallingBlockState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `FallingBlockEntity`.
unsafe impl DowncastType for FallingBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/falling_block");
}

impl FallingBlockEntity {
    /// Creates a falling block entity carrying sand, vanilla's default block state.
    ///
    /// Used by the generated factory for spawn and load paths; [`Self::fall`] is the
    /// gameplay entry point.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(FallingBlockEntityData::new()),
            state: SyncMutex::new(FallingBlockState {
                block_state: vanilla_blocks::SAND.default_state(),
                time: 0,
                drop_item: true,
                cancel_drop: false,
            }),
        }
    }

    /// Creates a falling block entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(FallingBlockEntityData::new()),
            state: SyncMutex::new(FallingBlockState {
                block_state: vanilla_blocks::SAND.default_state(),
                time: 0,
                drop_item: true,
                cancel_drop: false,
            }),
        }
    }

    /// Vanilla `FallingBlockEntity.fall`: replaces the block with its fluid and spawns the
    /// falling entity in its place.
    pub fn fall(world: &Arc<World>, pos: BlockPos, state: BlockStateId) -> Option<Arc<Self>> {
        // The carried state never stays waterlogged; the water is left behind in the world.
        let carried = if state
            .try_get_value(&BlockStateProperties::WATERLOGGED)
            .is_some()
        {
            state.set_value(&BlockStateProperties::WATERLOGGED, false)
        } else {
            state
        };

        let position = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
        );

        let entity = Arc::new(Self::new(
            &vanilla_entities::FALLING_BLOCK,
            next_entity_id(),
            position,
            Arc::downgrade(world),
        ));
        entity.state.lock().block_state = carried;
        entity.entity_data.lock().start_pos.set(pos);
        entity.set_old_position_to_current();

        world.set_block(
            pos,
            fluid_state_to_block(state.get_fluid_state()),
            UpdateFlags::UPDATE_ALL,
        );

        let shared: SharedEntity = entity.clone();
        if let Err(error) = world.try_add_entity(shared) {
            log::warn!("Failed to spawn falling block entity: {error}");
            return None;
        }

        Some(entity)
    }

    /// Returns the block state this entity is carrying.
    #[must_use]
    pub fn block_state(&self) -> BlockStateId {
        self.state.lock().block_state
    }

    /// Suppresses the item drop when the block cannot be placed on landing.
    pub fn set_drop_item(&self, drop_item: bool) {
        self.state.lock().drop_item = drop_item;
    }

    /// Makes landing destroy the block instead of placing it.
    pub fn set_cancel_drop(&self, cancel_drop: bool) {
        self.state.lock().cancel_drop = cancel_drop;
    }

    /// Drops the carried block as an item, if dropping is enabled.
    fn drop_as_item(&self, world: &Arc<World>) {
        let (block_state, drop_item) = {
            let state = self.state.lock();
            (state.block_state, state.drop_item)
        };
        if !drop_item {
            return;
        }

        let Some(item) = REGISTRY.items.by_key(&block_state.get_block().key) else {
            return;
        };
        world.drop_item_stack(self.block_position(), ItemStack::new(item));
    }

    /// Handles the tick where the entity comes to rest.
    fn land(&self, world: &Arc<World>, pos: BlockPos) {
        self.set_velocity(self.velocity() * LANDING_DAMPING);

        let current_state = world.get_block_state(pos);
        if current_state.get_block() == &vanilla_blocks::MOVING_PISTON {
            return;
        }

        let (block_state, cancel_drop) = {
            let state = self.state.lock();
            (state.block_state, state.cancel_drop)
        };

        if cancel_drop {
            self.set_removed(RemovalReason::Discarded);
            Self::call_on_broken_after_fall(world, block_state, pos);
            return;
        }

        let behavior = BLOCK_BEHAVIORS.get_behavior(block_state.get_block());
        let would_keep_falling = falling_block_is_free(world.get_block_state(pos.below()));
        let survives =
            behavior.can_survive(block_state, world.as_ref(), pos) && !would_keep_falling;

        if !current_state.is_replaceable() || !survives {
            self.set_removed(RemovalReason::Discarded);
            Self::call_on_broken_after_fall(world, block_state, pos);
            self.drop_as_item(world);
            return;
        }

        // A block that can be waterlogged re-absorbs the water it lands in.
        let placed_state = if block_state
            .try_get_value(&BlockStateProperties::WATERLOGGED)
            .is_some()
            && world
                .get_block_state(pos)
                .get_fluid_state()
                .fluid_id
                .has_tag(&FluidTag::WATER)
        {
            block_state.set_value(&BlockStateProperties::WATERLOGGED, true)
        } else {
            block_state
        };

        if world.set_block(pos, placed_state, UpdateFlags::UPDATE_ALL) {
            self.set_removed(RemovalReason::Discarded);
            if let Some(fallable) = behavior.as_fallable() {
                fallable.on_land(world, pos, placed_state, current_state);
            }
        } else {
            self.set_removed(RemovalReason::Discarded);
            Self::call_on_broken_after_fall(world, block_state, pos);
            self.drop_as_item(world);
        }
    }

    fn call_on_broken_after_fall(world: &Arc<World>, state: BlockStateId, pos: BlockPos) {
        if let Some(fallable) = BLOCK_BEHAVIORS
            .get_behavior(state.get_block())
            .as_fallable()
        {
            fallable.on_broken_after_fall(world, pos, state);
        }
    }
}

impl Entity for FallingBlockEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn get_default_gravity(&self) -> f64 {
        FALLING_BLOCK_GRAVITY
    }

    fn spawn_data(&self) -> i32 {
        i32::from(self.block_state().0)
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn tick(&self) {
        if self.block_state().is_air() {
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        self.set_old_position_to_current();
        let Some(world) = self.level() else {
            return;
        };

        self.state.lock().time += 1;
        self.apply_gravity();
        self.move_entity(MoverType::SelfMovement, self.velocity());
        self.apply_effects_from_blocks();

        if self.is_removed() {
            return;
        }

        let pos = self.block_position();
        if self.on_ground() {
            self.land(&world, pos);
            return;
        }

        let time = self.state.lock().time;
        let out_of_world = pos.y() <= world.min_y() || pos.y() >= world.max_y_exclusive();
        if (time > OUT_OF_WORLD_TIMEOUT && out_of_world) || time > MAX_LIFETIME {
            self.drop_as_item(&world);
            self.set_removed(RemovalReason::Discarded);
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();
        nbt.insert("Time", state.time);
        nbt.insert("DropItem", i8::from(state.drop_item));
        nbt.insert("CancelDrop", i8::from(state.cancel_drop));
        nbt.insert("BlockState", block_state_nbt::save(state.block_state));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        if let Some(time) = nbt.int("Time") {
            state.time = time;
        }
        if let Some(drop_item) = nbt.byte("DropItem") {
            state.drop_item = drop_item != 0;
        }
        if let Some(cancel_drop) = nbt.byte("CancelDrop") {
            state.cancel_drop = cancel_drop != 0;
        }
        if let Some(encoded) = nbt.compound("BlockState")
            && let Some(block_state) = block_state_nbt::load(encoded)
        {
            state.block_state = block_state;
        }
    }
}
