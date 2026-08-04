//! Shared resolution of block-position arguments.

use steel_utils::{BlockPos, BoundingBox, ChunkPos, SectionPos, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::CommandSyntaxError,
    execution::{CommandSource, SteelCommandContext},
};
use crate::world::World;

/// Resolves a block-position argument that must name a loaded, in-world block.
///
/// Mirrors vanilla `BlockPosArgument.getLoadedBlockPos`, including the order of its two
/// checks: an unloaded position is reported as unloaded even when it is also out of bounds.
pub(super) fn loaded_block_position(
    context: &SteelCommandContext<CommandSource>,
    name: &str,
) -> Result<BlockPos, CommandSyntaxError> {
    loaded_block_position_in(context, &context.source().world().clone(), name)
}

/// Resolves a block-position argument against an explicitly chosen world.
///
/// Relative and local coordinates still resolve against the command source, as vanilla's
/// three-argument `getLoadedBlockPos` overload does; only the loaded and bounds checks use
/// `world`. `/clone … from <dimension>` needs that split.
pub(super) fn loaded_block_position_in(
    context: &SteelCommandContext<CommandSource>,
    world: &World,
    name: &str,
) -> Result<BlockPos, CommandSyntaxError> {
    let position = context
        .coordinates(name)
        .ok_or_else(|| missing_position_argument(name))?
        .block_pos(context.source());
    if !world.is_full_chunk_loaded_at(position) {
        return Err(unloaded_position());
    }
    if !world.is_in_world_bounds(position) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::ARGUMENT_POS_OUTOFWORLD,
        )));
    }
    Ok(position)
}

pub(super) fn missing_position_argument(name: &str) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(format!(
        "Parsed value for {name} is missing from the command context"
    ))
}

/// The error vanilla reports for a position whose chunk is not loaded.
pub(super) fn unloaded_position() -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(&translations::ARGUMENT_POS_UNLOADED))
}

/// Returns the inclusive block count of `region`, saturating rather than overflowing.
pub(super) fn block_region_volume(region: &BoundingBox) -> i64 {
    let x_span = i64::from(region.max_x()) - i64::from(region.min_x()) + 1;
    let y_span = i64::from(region.max_y()) - i64::from(region.min_y()) + 1;
    let z_span = i64::from(region.max_z()) - i64::from(region.min_z()) + 1;
    x_span.saturating_mul(y_span).saturating_mul(z_span)
}

/// Rejects a region that reaches into a chunk which is not loaded.
///
/// Stands in for vanilla `Level.hasChunksAt`. Steel's synchronous command runner reports
/// unloaded region chunks instead of loading them.
pub(super) fn ensure_region_chunks_loaded(
    world: &World,
    region: &BoundingBox,
) -> Result<(), CommandSyntaxError> {
    if region.max_y() < world.get_min_y() || region.min_y() > world.get_max_y() {
        return Ok(());
    }
    let min_chunk_x = SectionPos::block_to_section_coord(region.min_x());
    let max_chunk_x = SectionPos::block_to_section_coord(region.max_x());
    let min_chunk_z = SectionPos::block_to_section_coord(region.min_z());
    let max_chunk_z = SectionPos::block_to_section_coord(region.max_z());
    for chunk_z in min_chunk_z..=max_chunk_z {
        for chunk_x in min_chunk_x..=max_chunk_x {
            if !ChunkPos::is_valid(chunk_x, chunk_z) {
                continue;
            }
            let pos = BlockPos::new(chunk_x * 16, world.get_min_y(), chunk_z * 16);
            if !world.is_full_chunk_loaded_at(pos) {
                return Err(unloaded_position());
            }
        }
    }
    Ok(())
}
