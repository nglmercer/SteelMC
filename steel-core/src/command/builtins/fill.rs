//! Fills a region with a block, mirroring vanilla `FillCommand`.

use std::sync::Arc;

use steel_registry::vanilla_game_rules::MAX_BLOCK_MODIFICATIONS;
use steel_registry::{blocks::block_state_ext::BlockStateExt as _, vanilla_blocks};
use steel_utils::{
    BlockPos, BoundingBox, Identifier, nbt::compare_nbt_compounds, translations, types::UpdateFlags,
};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        BlockInput, BlockPredicate, CommandSource, SteelArgumentType, SteelCommandContext,
        SteelCommandRuntime, argument, literal,
    },
    registration::CommandRegistration,
};
use super::position::{loaded_block_position, missing_position_argument};
use crate::world::World;

/// Vanilla places with `2 | 256`: notify clients, but skip block-entity side effects.
const DEFAULT_FLAGS: UpdateFlags =
    UpdateFlags::UPDATE_CLIENTS.union(UpdateFlags::UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS);

/// Vanilla's `strict` mode places with `2 | 816`.
const STRICT_FLAGS: UpdateFlags = DEFAULT_FLAGS
    .union(UpdateFlags::UPDATE_KNOWN_SHAPE)
    .union(UpdateFlags::UPDATE_SUPPRESS_DROPS)
    .union(UpdateFlags::UPDATE_SKIP_ON_PLACE);

/// How each position in the region is treated.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Fill the whole region.
    Replace,
    /// Fill only the region's faces, leaving the interior untouched.
    Outline,
    /// Fill the faces and clear the interior to air.
    Hollow,
    /// Break the old block, dropping its items, before filling.
    Destroy,
}

/// Which positions in the region are eligible to be filled.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    /// Every position.
    All,
    /// Only air, as vanilla's `keep` does.
    OnlyAir,
    /// Only positions matching the `filter` block-predicate argument.
    Predicate,
}

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("fill"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    // Vanilla's `wrapWithMode` hangs the same four mode literals off both the `block` argument
    // and the `filter` argument, so the modes are available with and without a filter.
    let block = with_modes(
        argument("block", SteelArgumentType::block_state()),
        Filter::All,
    )
    .then(
        literal("replace")
            .executes(|context| fill(context, Mode::Replace, Filter::All, false))
            .then(with_modes(
                argument("filter", SteelArgumentType::block_predicate()),
                Filter::Predicate,
            )),
    )
    .then(literal("keep").executes(|context| fill(context, Mode::Replace, Filter::OnlyAir, false)));

    literal("fill").then(
        argument("from", SteelArgumentType::block_pos())
            .then(argument("to", SteelArgumentType::block_pos()).then(block)),
    )
}

/// Adds vanilla's `outline`, `hollow`, `destroy` and `strict` literals under `builder`, and
/// makes `builder` itself a plain replace.
fn with_modes(
    builder: CommandNodeBuilder<CommandSource, SteelCommandRuntime>,
    filter: Filter,
) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let mut builder = builder.executes(move |context| fill(context, Mode::Replace, filter, false));
    for (name, mode) in [
        ("outline", Mode::Outline),
        ("hollow", Mode::Hollow),
        ("destroy", Mode::Destroy),
    ] {
        builder =
            builder.then(literal(name).executes(move |context| fill(context, mode, filter, false)));
    }
    builder
        .then(literal("strict").executes(move |context| fill(context, Mode::Replace, filter, true)))
}

fn fill(
    context: &SteelCommandContext<CommandSource>,
    mode: Mode,
    filter: Filter,
    strict: bool,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let region = BoundingBox::from_corners(
        loaded_block_position(context, "from")?,
        loaded_block_position(context, "to")?,
    );

    let area = i64::from(region.width()) * i64::from(region.height()) * i64::from(region.depth());
    let limit = world.get_game_rule(&MAX_BLOCK_MODIFICATIONS);
    if area > i64::from(limit) {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_FILL_TOOBIG
                .message([limit.to_string(), area.to_string()])
                .component(),
        ));
    }

    let Some(block) = context.block_state("block") else {
        return Err(missing_position_argument("block"));
    };
    let predicate = match filter {
        Filter::Predicate => Some(
            context
                .block_predicate("filter")
                .ok_or_else(|| missing_position_argument("filter"))?,
        ),
        Filter::All | Filter::OnlyAir => None,
    };
    let hollow_core = BlockInput::of_state(vanilla_blocks::AIR.default_state());

    // Neighbor updates are deferred until the whole region is placed, so that a block does not
    // react to a neighbor the fill is about to overwrite.
    let mut deferred_updates = Vec::new();
    let mut count = 0_i32;

    // Vanilla's `BlockPos.betweenClosed` advances x fastest and z slowest; placement side
    // effects depend on that order.
    for z in region.min_z()..=region.max_z() {
        for y in region.min_y()..=region.max_y() {
            for x in region.min_x()..=region.max_x() {
                let pos = BlockPos::new(x, y, z);
                if !position_matches(world, pos, filter, predicate) {
                    continue;
                }

                let old_state = world.get_block_state(pos);
                let affected = mode == Mode::Destroy && world.destroy_block(pos, true);

                let placed = match block_for(mode, region, pos, block, &hollow_core) {
                    None => false,
                    Some(input) => input.place(
                        world,
                        pos,
                        if strict { STRICT_FLAGS } else { DEFAULT_FLAGS },
                    ),
                };

                if placed {
                    if !strict {
                        deferred_updates.push((pos, old_state));
                    }
                    count += 1;
                } else if affected {
                    count += 1;
                }
            }
        }
    }

    for (pos, old_state) in deferred_updates {
        world.update_neighbours_on_block_set(pos, old_state);
    }

    if count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_FILL_FAILED,
        )));
    }

    let message = translations::COMMANDS_FILL_SUCCESS
        .message([count.to_string()])
        .component();
    source.send_success(&message, true);
    Ok(count)
}

/// Returns the block to place at `pos`, or `None` when this mode leaves the position alone.
fn block_for<'a>(
    mode: Mode,
    region: BoundingBox,
    pos: BlockPos,
    block: &'a BlockInput,
    hollow_core: &'a BlockInput,
) -> Option<&'a BlockInput> {
    if mode == Mode::Replace || mode == Mode::Destroy || is_on_region_face(region, pos) {
        return Some(block);
    }
    match mode {
        Mode::Hollow => Some(hollow_core),
        _ => None,
    }
}

/// Whether `pos` lies on any face of `region`, which is what `outline` and `hollow` fill.
const fn is_on_region_face(region: BoundingBox, pos: BlockPos) -> bool {
    pos.x() == region.min_x()
        || pos.x() == region.max_x()
        || pos.y() == region.min_y()
        || pos.y() == region.max_y()
        || pos.z() == region.min_z()
        || pos.z() == region.max_z()
}

fn position_matches(
    world: &Arc<World>,
    pos: BlockPos,
    filter: Filter,
    predicate: Option<&BlockPredicate>,
) -> bool {
    match filter {
        Filter::All => true,
        Filter::OnlyAir => world.get_block_state(pos).is_air(),
        Filter::Predicate => predicate.is_some_and(|predicate| {
            if !predicate.matches_state(world.get_block_state(pos)) {
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
    use super::{Mode, block_for, is_on_region_face};
    use crate::command::{
        brigadier::{CommandDispatcher, NodeId},
        execution::{BlockInput, CommandSource, SteelArgumentType, SteelCommandRuntime},
    };
    use steel_registry::test_support::init_test_registry;
    use steel_registry::vanilla_blocks;
    use steel_utils::{BlockPos, BoundingBox};

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

    /// Vanilla hangs the mode literals off both `block` and `filter`, and `keep` takes none.
    #[test]
    fn fill_graph_repeats_the_modes_under_block_and_under_filter() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "fill");
        let from = child(&dispatcher, root, "from");
        let to = child(&dispatcher, from, "to");
        let block = child(&dispatcher, to, "block");
        assert_eq!(
            dispatcher.node(block).and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::block_state())
        );
        assert_executable(&dispatcher, block, "fill block");

        for mode in ["outline", "hollow", "destroy", "strict"] {
            assert_executable(&dispatcher, child(&dispatcher, block, mode), mode);
        }

        let replace = child(&dispatcher, block, "replace");
        assert_executable(&dispatcher, replace, "fill replace");
        let filter = child(&dispatcher, replace, "filter");
        assert_eq!(
            dispatcher
                .node(filter)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::block_predicate())
        );
        assert_executable(&dispatcher, filter, "fill filter");
        for mode in ["outline", "hollow", "destroy", "strict"] {
            assert_executable(&dispatcher, child(&dispatcher, filter, mode), mode);
        }

        let keep = child(&dispatcher, block, "keep");
        assert_executable(&dispatcher, keep, "fill keep");
        assert!(
            dispatcher.children(keep).is_none_or(<[NodeId]>::is_empty),
            "keep takes no mode literals"
        );
    }

    /// `outline` skips the interior entirely while `hollow` clears it to air; both fill faces.
    #[test]
    fn outline_and_hollow_differ_only_in_the_region_interior() {
        init_test_registry();
        let region = BoundingBox::from_corners(BlockPos::new(0, 0, 0), BlockPos::new(4, 4, 4));
        let block = BlockInput::of_state(vanilla_blocks::STONE.default_state());
        let air = BlockInput::of_state(vanilla_blocks::AIR.default_state());

        let face = BlockPos::new(0, 2, 2);
        let interior = BlockPos::new(2, 2, 2);
        assert!(is_on_region_face(region, face));
        assert!(!is_on_region_face(region, interior));

        for mode in [Mode::Outline, Mode::Hollow] {
            assert_eq!(
                block_for(mode, region, face, &block, &air),
                Some(&block),
                "faces are always filled with the target block"
            );
        }
        assert_eq!(
            block_for(Mode::Outline, region, interior, &block, &air),
            None
        );
        assert_eq!(
            block_for(Mode::Hollow, region, interior, &block, &air),
            Some(&air)
        );
        assert_eq!(
            block_for(Mode::Replace, region, interior, &block, &air),
            Some(&block)
        );
    }
}
