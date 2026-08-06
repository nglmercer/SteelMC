//! Vanilla `EntityType.spawn` — creating an entity at a block position and adding
//! it to the world.
//!
//! Vanilla hangs this on `EntityType` itself. `EntityTypeRef` is generated registry
//! data in `steel-registry`, which cannot depend on world or entity state, so the
//! behavior lives here as an extension trait keyed to the same type.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::data_components::vanilla_components::{CUSTOM_DATA, CUSTOM_NAME};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_utils::{BlockLocalAabb, BlockPos, axis::Axis};

use super::mob::wrap_degrees;
use super::{ENTITIES, EntitySpawnReason, SharedEntity, next_entity_id};
use crate::behavior::BlockCollisionContext;
use crate::physics::{CollisionWorld as _, WorldCollisionProvider, collide};
use crate::world::World;

/// Vanilla `EntityType.spawn`.
pub trait EntityTypeSpawnExt {
    /// Returns vanilla `EntityType.spawn(ServerLevel, ItemStack, LivingEntity, BlockPos,
    /// EntitySpawnReason, boolean, boolean)`.
    ///
    /// `try_move_down` sweeps the entity down onto the first surface below the spawn
    /// block; `moved_up` marks that the caller already stepped the spawn position up by
    /// one block, which widens that sweep.
    ///
    /// Returns `None` when no entity factory is registered for the type or the world
    /// rejects the new entity.
    fn spawn_at_block(
        &self,
        world: &Arc<World>,
        stack: Option<&ItemStack>,
        spawn_pos: BlockPos,
        spawn_reason: EntitySpawnReason,
        try_move_down: bool,
        moved_up: bool,
    ) -> Option<SharedEntity>;
}

impl EntityTypeSpawnExt for EntityTypeRef {
    fn spawn_at_block(
        &self,
        world: &Arc<World>,
        stack: Option<&ItemStack>,
        spawn_pos: BlockPos,
        spawn_reason: EntitySpawnReason,
        try_move_down: bool,
        moved_up: bool,
    ) -> Option<SharedEntity> {
        let entity = create_at_block(
            self,
            world,
            stack,
            spawn_pos,
            spawn_reason,
            try_move_down,
            moved_up,
        )?;

        if let Err(error) = world.try_add_entity(Arc::clone(&entity)) {
            log::debug!("failed to spawn {}: {error}", self.key);
            return None;
        }

        if let Some(mob) = entity.as_mob() {
            mob.play_ambient_sound();
        }

        Some(entity)
    }
}

/// Returns vanilla `EntityType.create(ServerLevel, PostSpawnProcessor, BlockPos,
/// EntitySpawnReason, boolean, boolean)`.
///
/// The entity is positioned and finalized but not yet added to the world.
fn create_at_block(
    entity_type: EntityTypeRef,
    world: &Arc<World>,
    stack: Option<&ItemStack>,
    spawn_pos: BlockPos,
    spawn_reason: EntitySpawnReason,
    try_move_down: bool,
    moved_up: bool,
) -> Option<SharedEntity> {
    let entity = ENTITIES.create(
        entity_type,
        next_entity_id(),
        DVec3::ZERO,
        Arc::downgrade(world),
    )?;

    let center_x = f64::from(spawn_pos.x()) + 0.5;
    let center_z = f64::from(spawn_pos.z()) + 0.5;

    let y_offset = if try_move_down {
        // Vanilla positions the entity one block up first so that `getBoundingBox`
        // describes the box the downward sweep starts from.
        entity.base().set_position_local(DVec3::new(
            center_x,
            f64::from(spawn_pos.y()) + 1.0,
            center_z,
        ));
        y_offset(world, spawn_pos, moved_up, &entity)
    } else {
        0.0
    };

    let yaw = wrap_degrees(rand::random::<f32>() * 360.0);
    snap_to(
        &entity,
        DVec3::new(center_x, f64::from(spawn_pos.y()) + y_offset, center_z),
        yaw,
        0.0,
    );

    if let Some(mob) = entity.as_mob() {
        if let Some(living) = entity.as_living_entity() {
            living.set_y_head_rot(yaw);
            living.set_y_body_rot(yaw);
        }
        let _ = mob.finalize_spawn(world, spawn_reason, None);
    }

    // Vanilla `EntityType.appendDefaultStackConfig` applies the stack's implicit
    // components and then `updateCustomEntityTag`. The latter is not implemented:
    // vanilla merges the stack's `ENTITY_DATA` payload over a full `saveWithoutId`
    // dump and reloads the entity in place, and Steel has no in-place entity reload —
    // `EntityRegistry::create_and_load_or_raw` only builds a *new* entity from NBT.
    // Every vanilla spawn egg carries an empty `ENTITY_DATA` payload, so only
    // hand-authored component data is affected.
    if let Some(stack) = stack {
        apply_components_from_item_stack(&entity, stack);
    }

    Some(entity)
}

/// Returns vanilla `EntityType.getYOffset`.
fn y_offset(world: &Arc<World>, spawn_pos: BlockPos, moved_up: bool, entity: &SharedEntity) -> f64 {
    let mut search_box = BlockLocalAabb::FULL_BLOCK.at_block(spawn_pos);
    if moved_up {
        search_box = search_box.expand_towards(DVec3::new(0.0, -1.0, 0.0));
    }

    let collisions = WorldCollisionProvider::new(world)
        .get_collisions_with_context(&search_box, BlockCollisionContext::empty());
    let sweep = if moved_up { -2.0 } else { -1.0 };

    1.0 + collide(Axis::Y, &entity.bounding_box(), &collisions, sweep)
}

/// Applies vanilla `Entity.snapTo` to an entity that is not in the world yet.
fn snap_to(entity: &SharedEntity, position: DVec3, yaw: f32, pitch: f32) {
    entity.base().set_position_local(position);
    entity.set_rotation((yaw, pitch));
    entity.set_old_position_to_current();
}

/// Applies vanilla `Entity.applyComponentsFromItemStack`.
///
/// Vanilla `Entity.applyImplicitComponents` reads only `CUSTOM_NAME` and
/// `CUSTOM_DATA`, and no entity subclass overrides it.
fn apply_components_from_item_stack(entity: &SharedEntity, stack: &ItemStack) {
    if let Some(custom_name) = stack.get(CUSTOM_NAME) {
        entity.set_custom_name(Some(custom_name.clone()));
    }
    if let Some(custom_data) = stack.get(CUSTOM_DATA) {
        entity.set_custom_data(custom_data.copy_tag());
    }
}
