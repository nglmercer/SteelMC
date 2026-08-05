//! Moving items between containers.
//!
//! Backs vanilla's `HopperBlockEntity.getContainerAt` / `addItem`, which droppers and
//! hoppers both use to push items into a neighboring container.

use std::sync::Arc;

use steel_registry::item_stack::ItemStack;
use steel_utils::BlockPos;

use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::world::World;

/// Vanilla `HopperBlockEntity.getContainerAt`: the container occupying `pos`, if any.
///
/// Vanilla also finds container minecarts and the composter's worldly container here;
/// Steel only resolves block-entity containers so far.
#[must_use]
pub fn container_at(world: &Arc<World>, pos: BlockPos) -> Option<ContainerRef> {
    world
        .get_block_entity(pos)
        .and_then(ContainerRef::from_block_entity)
}

/// Vanilla `HopperBlockEntity.addItem`: inserts as much of `stack` as fits.
///
/// Returns whatever could not be inserted.
#[must_use]
pub fn add_item(target: &ContainerRef, mut stack: ItemStack) -> ItemStack {
    if stack.is_empty() {
        return stack;
    }

    let mut guard = ContainerLockGuard::lock_all(&[target]);
    let Some(container) = guard.get_mut(target.container_id()) else {
        return stack;
    };

    // Vanilla tops up matching stacks before filling an empty slot.
    for slot in 0..container.get_container_size() {
        if stack.is_empty() {
            break;
        }

        let existing = container.get_item(slot).clone();
        if existing.is_empty() || !ItemStack::is_same_item_same_components(&existing, &stack) {
            continue;
        }

        let limit = container.get_max_stack_size_for_item(&existing);
        let room = limit - existing.count();
        if room <= 0 {
            continue;
        }

        let moved = room.min(stack.count());
        let mut merged = existing;
        merged.set_count(merged.count() + moved);
        container.set_item(slot, merged);
        stack.set_count(stack.count() - moved);
    }

    if !stack.is_empty() {
        for slot in 0..container.get_container_size() {
            if !container.get_item(slot).is_empty() {
                continue;
            }

            let limit = container.get_max_stack_size_for_item(&stack);
            let moved = limit.min(stack.count());
            let mut placed = stack.clone();
            placed.set_count(moved);
            container.set_item(slot, placed);
            stack.set_count(stack.count() - moved);
            if stack.is_empty() {
                break;
            }
        }
    }

    if stack.count() <= 0 {
        return ItemStack::empty();
    }
    stack
}
