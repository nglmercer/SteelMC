//! Sets a single block, mirroring vanilla `SetBlockCommand`.

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_utils::{Identifier, translations, types::UpdateFlags};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::position::{loaded_block_position, missing_position_argument};

/// Vanilla places with `2 | 256`: notify clients, but skip block-entity side effects.
const DEFAULT_FLAGS: UpdateFlags =
    UpdateFlags::UPDATE_CLIENTS.union(UpdateFlags::UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS);

/// Vanilla's `strict` mode places with `2 | 816`, additionally taking the state exactly as
/// written, suppressing drops, and skipping the placement callback.
const STRICT_FLAGS: UpdateFlags = DEFAULT_FLAGS
    .union(UpdateFlags::UPDATE_KNOWN_SHAPE)
    .union(UpdateFlags::UPDATE_SUPPRESS_DROPS)
    .union(UpdateFlags::UPDATE_SKIP_ON_PLACE);

/// Whether an existing block is removed, and how, before the new one is placed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Overwrite whatever is there.
    Replace,
    /// Break the old block first, dropping its items.
    Destroy,
    /// Only place into air.
    Keep,
    /// Replace, but place the written state verbatim with no side effects.
    Strict,
}

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("setblock"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let mut block = argument("block", SteelArgumentType::block_state())
        .executes(|context| set_block(context, Mode::Replace));
    for (name, mode) in [
        ("destroy", Mode::Destroy),
        ("keep", Mode::Keep),
        ("replace", Mode::Replace),
        ("strict", Mode::Strict),
    ] {
        block = block.then(literal(name).executes(move |context| set_block(context, mode)));
    }
    literal("setblock").then(argument("pos", SteelArgumentType::block_pos()).then(block))
}

fn set_block(
    context: &SteelCommandContext<CommandSource>,
    mode: Mode,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let pos = loaded_block_position(context, "pos")?;
    let Some(block) = context.block_state("block") else {
        return Err(missing_position_argument("block"));
    };

    if mode == Mode::Keep && !world.get_block_state(pos).is_air() {
        return Err(failed());
    }

    let place_needed = if mode == Mode::Destroy {
        world.destroy_block(pos, true);
        !block.state().is_air() || !world.get_block_state(pos).is_air()
    } else {
        true
    };

    let old_state = world.get_block_state(pos);
    let strict = mode == Mode::Strict;
    let flags = if strict { STRICT_FLAGS } else { DEFAULT_FLAGS };
    if place_needed && !block.place(world, pos, flags) {
        return Err(failed());
    }

    if !strict {
        world.update_neighbours_on_block_set(pos, old_state);
    }

    let message = translations::COMMANDS_SETBLOCK_SUCCESS
        .message([
            pos.x().to_string(),
            pos.y().to_string(),
            pos.z().to_string(),
        ])
        .component();
    source.send_success(&message, true);
    Ok(1)
}

fn failed() -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(&translations::COMMANDS_SETBLOCK_FAILED))
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use crate::command::{
        brigadier::{CommandDispatcher, NodeId},
        execution::{CommandSource, SteelArgumentType, SteelCommandRuntime},
    };
    use steel_registry::test_support::init_test_registry;

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

    /// `/setblock` is only executable once a block is given, and each mode is a leaf under it.
    #[test]
    fn setblock_graph_exposes_every_mode_under_the_block_argument() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "setblock");
        let Some(root_node) = dispatcher.node(root) else {
            panic!("setblock root should exist");
        };
        assert!(!root_node.is_executable());

        let pos = child(&dispatcher, root, "pos");
        assert_eq!(
            dispatcher.node(pos).and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::block_pos())
        );
        let Some(pos_node) = dispatcher.node(pos) else {
            panic!("setblock pos should exist");
        };
        assert!(!pos_node.is_executable());

        let block = child(&dispatcher, pos, "block");
        assert_eq!(
            dispatcher.node(block).and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::block_state())
        );
        let Some(block_node) = dispatcher.node(block) else {
            panic!("setblock block should exist");
        };
        assert!(block_node.is_executable());

        for mode in ["destroy", "keep", "replace", "strict"] {
            let node = child(&dispatcher, block, mode);
            let Some(mode_node) = dispatcher.node(node) else {
                panic!("setblock {mode} should exist");
            };
            assert!(mode_node.is_executable(), "{mode} should be executable");
        }
    }
}
