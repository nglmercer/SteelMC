//! Shared resolution of block-position arguments.

use steel_utils::{BlockPos, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::CommandSyntaxError,
    execution::{CommandSource, SteelCommandContext},
};

/// Resolves a block-position argument that must name a loaded, in-world block.
///
/// Mirrors vanilla `BlockPosArgument.getLoadedBlockPos`, including the order of its two
/// checks: an unloaded position is reported as unloaded even when it is also out of bounds.
pub(super) fn loaded_block_position(
    context: &SteelCommandContext<CommandSource>,
    name: &str,
) -> Result<BlockPos, CommandSyntaxError> {
    let position = context
        .coordinates(name)
        .ok_or_else(|| missing_position_argument(name))?
        .block_pos(context.source());
    let world = context.source().world();
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
