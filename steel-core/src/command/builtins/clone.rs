//! Copies a region of blocks, mirroring vanilla `CloneCommands`.

use std::{collections::VecDeque, sync::Arc};

use simdnbt::owned::NbtCompound;
use steel_registry::vanilla_game_rules::MAX_BLOCK_MODIFICATIONS;
use steel_registry::{blocks::block_state_ext::BlockStateExt as _, vanilla_blocks};
use steel_utils::{
    BlockPos, BlockStateId, BoundingBox, Identifier, nbt::compare_nbt_compounds, translations,
    types::UpdateFlags,
};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        BlockPredicate, CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime,
        argument, literal,
    },
    registration::CommandRegistration,
};
use super::position::{
    block_region_volume, ensure_region_chunks_loaded, loaded_block_position_in,
    missing_position_argument,
};
use crate::world::World;

/// The bits vanilla writes as the literal `816` in this command: take the state as given,
/// suppress drops and block-entity side effects, and skip the placement callback.
const STRICT_BITS: UpdateFlags = UpdateFlags::UPDATE_KNOWN_SHAPE
    .union(UpdateFlags::UPDATE_SUPPRESS_DROPS)
    .union(UpdateFlags::UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS)
    .union(UpdateFlags::UPDATE_SKIP_ON_PLACE);

/// How the source region is treated once it has been copied.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Copy, and reject a destination that overlaps the source.
    Normal,
    /// Copy, allowing the destination to overlap the source.
    Force,
    /// Copy, then clear the source region to air.
    Move,
}

impl Mode {
    /// `force` and `move` stage through barrier blocks, so they tolerate an overlap.
    const fn can_overlap(self) -> bool {
        matches!(self, Self::Force | Self::Move)
    }
}

/// Which source blocks are copied.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    /// Every block, including air.
    All,
    /// Everything but air, as vanilla's `masked` does.
    NotAir,
    /// Only blocks matching the `filter` block-predicate argument.
    Predicate,
}

/// One source block staged for writing at its destination.
struct CloneBlockInfo {
    pos: BlockPos,
    state: BlockStateId,
    block_entity_nbt: Option<NbtCompound>,
    previous_state_at_destination: BlockStateId,
}

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("clone"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("clone")
        .then(begin_end_and_destination())
        .then(
            literal("from").then(
                argument("sourceDimension", SteelArgumentType::world())
                    .then(begin_end_and_destination()),
            ),
        )
}

fn begin_end_and_destination() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    argument("begin", SteelArgumentType::block_pos()).then(
        argument("end", SteelArgumentType::block_pos())
            .then(destination_and_strict())
            .then(
                literal("to").then(
                    argument("targetDimension", SteelArgumentType::world())
                        .then(destination_and_strict()),
                ),
            ),
    )
}

/// Vanilla hangs `strict` off the `destination` argument, repeating every mode beneath it.
fn destination_and_strict() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    with_filters(
        argument("destination", SteelArgumentType::block_pos()),
        false,
    )
    .then(with_filters(literal("strict"), true))
}

/// Adds vanilla's `replace`, `masked` and `filtered` branches, each carrying the clone modes.
fn with_filters(
    builder: CommandNodeBuilder<CommandSource, SteelCommandRuntime>,
    strict: bool,
) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    with_modes(builder, Filter::All, strict)
        .then(with_modes(literal("replace"), Filter::All, strict))
        .then(with_modes(literal("masked"), Filter::NotAir, strict))
        .then(literal("filtered").then(with_modes(
            argument("filter", SteelArgumentType::block_predicate()),
            Filter::Predicate,
            strict,
        )))
}

/// Makes `builder` a plain clone and adds the `force`, `move` and `normal` literals under it.
fn with_modes(
    builder: CommandNodeBuilder<CommandSource, SteelCommandRuntime>,
    filter: Filter,
    strict: bool,
) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let mut builder = builder.executes(move |context| clone(context, Mode::Normal, filter, strict));
    for (name, mode) in [
        ("force", Mode::Force),
        ("move", Mode::Move),
        ("normal", Mode::Normal),
    ] {
        builder = builder
            .then(literal(name).executes(move |context| clone(context, mode, filter, strict)));
    }
    builder
}

/// Resolves an optional dimension argument, falling back to the source's own world.
fn dimension(
    context: &SteelCommandContext<CommandSource>,
    name: &str,
) -> Result<Arc<World>, CommandSyntaxError> {
    context.world_argument(name).map_or_else(
        || Ok(Arc::clone(context.source().world())),
        |argument| argument.resolve(context.source()),
    )
}

fn clone(
    context: &SteelCommandContext<CommandSource>,
    mode: Mode,
    filter: Filter,
    strict: bool,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let from_world = dimension(context, "sourceDimension")?;
    let to_world = dimension(context, "targetDimension")?;

    let begin = loaded_block_position_in(context, &from_world, "begin")?;
    let end = loaded_block_position_in(context, &from_world, "end")?;
    let from = BoundingBox::from_corners(begin, end);
    let destination_pos = loaded_block_position_in(context, &to_world, "destination")?;
    // Vanilla offsets by `from.getLength()`, which is the span minus one on each axis.
    let destination_end = BlockPos::new(
        destination_pos.x() + from.max_x() - from.min_x(),
        destination_pos.y() + from.max_y() - from.min_y(),
        destination_pos.z() + from.max_z() - from.min_z(),
    );
    let destination = BoundingBox::from_corners(destination_pos, destination_end);

    if !mode.can_overlap() && Arc::ptr_eq(&from_world, &to_world) && destination.intersects(from) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_CLONE_OVERLAP,
        )));
    }

    let area = block_region_volume(&from);
    let limit = source.world().get_game_rule(&MAX_BLOCK_MODIFICATIONS);
    if area > i64::from(limit) {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_CLONE_TOOBIG
                .message([limit.to_string(), area.to_string()])
                .component(),
        ));
    }

    ensure_region_chunks_loaded(&from_world, &from)?;
    ensure_region_chunks_loaded(&to_world, &destination)?;

    let predicate = match filter {
        Filter::Predicate => Some(
            context
                .block_predicate("filter")
                .ok_or_else(|| missing_position_argument("filter"))?,
        ),
        Filter::All | Filter::NotAir => None,
    };

    let staged = stage_region(
        &from_world,
        &to_world,
        from,
        destination_pos,
        filter,
        predicate,
    );

    let count = apply_staged(&from_world, &to_world, staged, mode, strict);

    // DEFERRED (Phase 4-8): Copy scheduled block ticks from the source region, as vanilla's
    // `getBlockTicks().copyAreaFrom` does. Steel's tick scheduler has no area-copy operation
    // yet, so a clone of a block with a pending tick lands without that tick.

    if count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_CLONE_FAILED,
        )));
    }

    let message = translations::COMMANDS_CLONE_SUCCESS
        .message([count.to_string()])
        .component();
    source.send_success(&message, true);
    Ok(count)
}

/// Writes the staged blocks into the destination, and clears the source for `move`.
///
/// Returns how many destination blocks actually changed.
fn apply_staged(
    from_world: &Arc<World>,
    to_world: &Arc<World>,
    staged: StagedRegion,
    mode: Mode,
    strict: bool,
) -> i32 {
    let StagedRegion {
        clear_positions, ..
    } = &staged;
    let default_flags = if strict {
        UpdateFlags::UPDATE_CLIENTS.union(STRICT_BITS)
    } else {
        UpdateFlags::UPDATE_CLIENTS
    };
    let barrier_flags = default_flags.union(STRICT_BITS);
    let barrier = vanilla_blocks::BARRIER.default_state();

    if mode == Mode::Move {
        // Two passes: barriers first so that nothing in the source region reacts to a
        // neighbour that is about to be cleared, then air.
        for pos in clear_positions {
            from_world.set_block(*pos, barrier, barrier_flags);
        }
        let clear_flags = if strict {
            default_flags
        } else {
            UpdateFlags::UPDATE_NEIGHBORS.union(UpdateFlags::UPDATE_CLIENTS)
        };
        let air = vanilla_blocks::AIR.default_state();
        for pos in clear_positions {
            from_world.set_block(*pos, air, clear_flags);
        }
    }

    let blocks = staged.ordered_blocks();
    for info in blocks.iter().rev() {
        to_world.set_block(info.pos, barrier, barrier_flags);
    }

    let mut count = 0_i32;
    for info in &blocks {
        if to_world.set_block(info.pos, info.state, default_flags) {
            count += 1;
        }
    }

    for info in &blocks {
        let Some(nbt) = info.block_entity_nbt.as_ref() else {
            continue;
        };
        if let Some(block_entity) = to_world.get_block_entity(info.pos)
            && block_entity.load_custom_only(nbt)
        {
            block_entity.set_changed();
        }
        // Vanilla re-places the state afterwards so the block entity settles against it.
        to_world.set_block(info.pos, info.state, default_flags);
    }

    if !strict {
        for info in blocks.iter().rev() {
            to_world.update_neighbours_on_block_set(info.pos, info.previous_state_at_destination);
        }
    }

    count
}

/// The three vanilla staging buckets, kept apart so they can be written in dependency order.
struct StagedRegion {
    solid: Vec<CloneBlockInfo>,
    block_entities: Vec<CloneBlockInfo>,
    other: Vec<CloneBlockInfo>,
    clear_positions: VecDeque<BlockPos>,
}

impl StagedRegion {
    /// Solid blocks first, then block entities, then everything that needs support.
    fn ordered_blocks(self) -> Vec<CloneBlockInfo> {
        let mut blocks = self.solid;
        blocks.extend(self.block_entities);
        blocks.extend(self.other);
        blocks
    }
}

fn stage_region(
    from_world: &Arc<World>,
    to_world: &Arc<World>,
    from: BoundingBox,
    destination_pos: BlockPos,
    filter: Filter,
    predicate: Option<&BlockPredicate>,
) -> StagedRegion {
    let offset = BlockPos::new(
        destination_pos.x() - from.min_x(),
        destination_pos.y() - from.min_y(),
        destination_pos.z() - from.min_z(),
    );
    let mut staged = StagedRegion {
        solid: Vec::new(),
        block_entities: Vec::new(),
        other: Vec::new(),
        clear_positions: VecDeque::new(),
    };

    // Vanilla's loop order: x fastest, z slowest.
    for z in from.min_z()..=from.max_z() {
        for y in from.min_y()..=from.max_y() {
            for x in from.min_x()..=from.max_x() {
                let source_pos = BlockPos::new(x, y, z);
                let state = from_world.get_block_state(source_pos);
                if !block_matches(from_world, source_pos, state, filter, predicate) {
                    continue;
                }

                let pos = BlockPos::new(x + offset.x(), y + offset.y(), z + offset.z());
                let previous_state_at_destination = to_world.get_block_state(pos);
                let block_entity = from_world.get_block_entity(source_pos);

                if let Some(block_entity) = block_entity {
                    staged.block_entities.push(CloneBlockInfo {
                        pos,
                        state,
                        block_entity_nbt: Some(block_entity.save_custom_only()),
                        previous_state_at_destination,
                    });
                    staged.clear_positions.push_back(source_pos);
                } else if state.is_solid_render()
                    || from_world.is_collision_shape_full_block_at(source_pos, state)
                {
                    staged.solid.push(CloneBlockInfo {
                        pos,
                        state,
                        block_entity_nbt: None,
                        previous_state_at_destination,
                    });
                    staged.clear_positions.push_back(source_pos);
                } else {
                    // Blocks that need support are cleared first and placed last.
                    staged.other.push(CloneBlockInfo {
                        pos,
                        state,
                        block_entity_nbt: None,
                        previous_state_at_destination,
                    });
                    staged.clear_positions.push_front(source_pos);
                }
            }
        }
    }

    staged
}

fn block_matches(
    world: &Arc<World>,
    pos: BlockPos,
    state: BlockStateId,
    filter: Filter,
    predicate: Option<&BlockPredicate>,
) -> bool {
    match filter {
        Filter::All => true,
        Filter::NotAir => !state.is_air(),
        Filter::Predicate => predicate.is_some_and(|predicate| {
            if !predicate.matches_state(state) {
                return false;
            }
            let Some(expected) = predicate.nbt() else {
                return true;
            };
            world.get_block_entity(pos).is_some_and(|block_entity| {
                compare_nbt_compounds(expected, &block_entity.save_with_full_metadata(), true)
            })
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use super::{CloneBlockInfo, StagedRegion};
    use crate::command::{
        brigadier::{CommandDispatcher, NodeId},
        execution::{CommandSource, SteelArgumentType, SteelCommandRuntime},
    };
    use std::collections::VecDeque;
    use steel_registry::test_support::init_test_registry;
    use steel_registry::vanilla_blocks;
    use steel_utils::{BlockPos, BlockStateId};

    type Dispatcher = CommandDispatcher<CommandSource, SteelCommandRuntime>;

    fn child(dispatcher: &Dispatcher, parent: NodeId, name: &str) -> NodeId {
        let Some(children) = dispatcher.children(parent) else {
            panic!("parent node should exist");
        };
        let Some(child) = children.iter().copied().find(|child| {
            dispatcher
                .node(*child)
                .is_some_and(|node| node.name() == name)
        }) else {
            panic!("child {name} should exist");
        };
        child
    }

    fn assert_executable(dispatcher: &Dispatcher, node: NodeId, what: &str) {
        let Some(node) = dispatcher.node(node) else {
            panic!("{what} should exist");
        };
        assert!(node.is_executable(), "{what} should be executable");
    }

    /// Every filter branch carries the three clone modes, and `strict` repeats the whole set.
    fn assert_filters_and_modes(dispatcher: &Dispatcher, parent: NodeId, what: &str) {
        assert_executable(dispatcher, parent, what);
        for mode in ["force", "move", "normal"] {
            assert_executable(dispatcher, child(dispatcher, parent, mode), mode);
        }
        for filter in ["replace", "masked"] {
            let node = child(dispatcher, parent, filter);
            assert_executable(dispatcher, node, filter);
            for mode in ["force", "move", "normal"] {
                assert_executable(dispatcher, child(dispatcher, node, mode), mode);
            }
        }
        let filtered = child(dispatcher, parent, "filtered");
        let filter = child(dispatcher, filtered, "filter");
        assert_eq!(
            dispatcher
                .node(filter)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::block_predicate())
        );
        assert_executable(dispatcher, filter, "clone filtered filter");
    }

    /// `strict` hangs off `destination` and repeats every filter and mode beneath it.
    #[test]
    fn clone_graph_repeats_filters_and_modes_under_destination_and_strict() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "clone");
        let begin = child(&dispatcher, root, "begin");
        let end = child(&dispatcher, begin, "end");
        let destination = child(&dispatcher, end, "destination");

        assert_filters_and_modes(&dispatcher, destination, "clone destination");
        assert_filters_and_modes(
            &dispatcher,
            child(&dispatcher, destination, "strict"),
            "clone strict",
        );
    }

    /// Both dimension branches reach the same destination subtree.
    #[test]
    fn clone_graph_exposes_source_and_target_dimension_branches() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "clone");

        let source_dimension = child(
            &dispatcher,
            child(&dispatcher, root, "from"),
            "sourceDimension",
        );
        assert_eq!(
            dispatcher
                .node(source_dimension)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::world())
        );

        for begin_parent in [root, source_dimension] {
            let end = child(
                &dispatcher,
                child(&dispatcher, begin_parent, "begin"),
                "end",
            );
            assert_executable(
                &dispatcher,
                child(&dispatcher, end, "destination"),
                "destination",
            );
            let target_dimension = child(
                &dispatcher,
                child(&dispatcher, end, "to"),
                "targetDimension",
            );
            assert_eq!(
                dispatcher
                    .node(target_dimension)
                    .and_then(|node| node.argument_type()),
                Some(&SteelArgumentType::world())
            );
            assert_executable(
                &dispatcher,
                child(&dispatcher, target_dimension, "destination"),
                "cross-dimension destination",
            );
        }
    }

    fn info(x: i32, state: BlockStateId) -> CloneBlockInfo {
        CloneBlockInfo {
            pos: BlockPos::new(x, 0, 0),
            state,
            block_entity_nbt: None,
            previous_state_at_destination: state,
        }
    }

    /// Solid blocks are written first so that the blocks needing support land last.
    #[test]
    fn staged_blocks_are_written_solid_then_block_entities_then_supported() {
        init_test_registry();
        let state = vanilla_blocks::STONE.default_state();
        let staged = StagedRegion {
            solid: vec![info(0, state)],
            block_entities: vec![info(1, state)],
            other: vec![info(2, state)],
            clear_positions: VecDeque::new(),
        };

        let order = staged
            .ordered_blocks()
            .iter()
            .map(|info| info.pos.x())
            .collect::<Vec<_>>();
        assert_eq!(order, [0, 1, 2]);
    }
}
