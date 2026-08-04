//! Shows and clears client titles, mirroring vanilla `TitleCommand`.

use std::sync::Arc;

use steel_protocol::packets::game::{
    CClearTitles, CSetActionBarText, CSetSubtitleText, CSetTitleText, CSetTitlesAnimation,
};
use steel_utils::{Identifier, translations};
use text_components::{TextComponent, translation::Translation};

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::position::missing_position_argument;
use crate::player::Player;

/// Which title line a `show` branch writes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Line {
    Title,
    Subtitle,
    ActionBar,
}

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("title"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let mut targets = argument("targets", SteelArgumentType::players())
        .then(literal("clear").executes(|context| clear(context, false)))
        .then(literal("reset").executes(|context| clear(context, true)))
        .then(times_branch());
    for (name, line) in [
        ("title", Line::Title),
        ("subtitle", Line::Subtitle),
        ("actionbar", Line::ActionBar),
    ] {
        targets = targets.then(
            literal(name).then(
                argument("title", SteelArgumentType::component())
                    .executes(move |context| show(context, line)),
            ),
        );
    }
    literal("title").then(targets)
}

fn times_branch() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("times").then(
        argument("fadeIn", SteelArgumentType::time(0)).then(
            argument("stay", SteelArgumentType::time(0))
                .then(argument("fadeOut", SteelArgumentType::time(0)).executes(set_times)),
        ),
    )
}

fn clear(
    context: &SteelCommandContext<CommandSource>,
    reset_times: bool,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.players("targets")?;
    for target in &targets {
        target.send_packet(CClearTitles { reset_times });
    }

    let (single, multiple) = if reset_times {
        (
            &translations::COMMANDS_TITLE_RESET_SINGLE,
            &translations::COMMANDS_TITLE_RESET_MULTIPLE,
        )
    } else {
        (
            &translations::COMMANDS_TITLE_CLEARED_SINGLE,
            &translations::COMMANDS_TITLE_CLEARED_MULTIPLE,
        )
    };
    report(context, &targets, single, multiple)
}

fn show(
    context: &SteelCommandContext<CommandSource>,
    line: Line,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.players("targets")?;
    let Some(title) = context.text_component("title") else {
        return Err(missing_position_argument("title"));
    };

    for target in &targets {
        // Vanilla resolves the component per recipient, so a selector inside it names
        // whoever is being shown the title rather than the command's source.
        match line {
            Line::Title => target.send_packet(CSetTitleText::new(title, &**target)),
            Line::Subtitle => target.send_packet(CSetSubtitleText::new(title, &**target)),
            Line::ActionBar => target.send_packet(CSetActionBarText::new(title, &**target)),
        }
    }

    let (single, multiple) = match line {
        Line::Title => (
            &translations::COMMANDS_TITLE_SHOW_TITLE_SINGLE,
            &translations::COMMANDS_TITLE_SHOW_TITLE_MULTIPLE,
        ),
        Line::Subtitle => (
            &translations::COMMANDS_TITLE_SHOW_SUBTITLE_SINGLE,
            &translations::COMMANDS_TITLE_SHOW_SUBTITLE_MULTIPLE,
        ),
        Line::ActionBar => (
            &translations::COMMANDS_TITLE_SHOW_ACTIONBAR_SINGLE,
            &translations::COMMANDS_TITLE_SHOW_ACTIONBAR_MULTIPLE,
        ),
    };
    report(context, &targets, single, multiple)
}

fn set_times(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let targets = context.players("targets")?;
    let Some(fade_in) = context.time("fadeIn") else {
        return Err(missing_position_argument("fadeIn"));
    };
    let Some(stay) = context.time("stay") else {
        return Err(missing_position_argument("stay"));
    };
    let Some(fade_out) = context.time("fadeOut") else {
        return Err(missing_position_argument("fadeOut"));
    };

    for target in &targets {
        target.send_packet(CSetTitlesAnimation {
            fade_in,
            stay,
            fade_out,
        });
    }

    report(
        context,
        &targets,
        &translations::COMMANDS_TITLE_TIMES_SINGLE,
        &translations::COMMANDS_TITLE_TIMES_MULTIPLE,
    )
}

/// Every branch reports the one recipient by name, or the count when there are several.
fn report(
    context: &SteelCommandContext<CommandSource>,
    targets: &[Arc<Player>],
    single: &'static Translation<1>,
    multiple: &'static Translation<1>,
) -> Result<i32, CommandSyntaxError> {
    let message = if let [only] = targets {
        single
            .message([TextComponent::plain(only.gameprofile.name.clone())])
            .component()
    } else {
        multiple
            .message([TextComponent::plain(targets.len().to_string())])
            .component()
    };
    context.source().send_success(&message, true);

    i32::try_from(targets.len())
        .map_err(|_| CommandSyntaxError::dynamic("Player count exceeds the command result range"))
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use crate::command::brigadier::{CommandDispatcher, NodeId};
    use crate::command::execution::{CommandSource, SteelArgumentType, SteelCommandRuntime};
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

    fn assert_executable(dispatcher: &Dispatcher, node: NodeId, what: &str) {
        let Some(node) = dispatcher.node(node) else {
            panic!("{what} should exist");
        };
        assert!(node.is_executable(), "{what} should be executable");
    }

    /// The three text lines each take a component; `times` needs all three durations.
    #[test]
    fn title_graph_covers_every_branch() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "title");
        let targets = child(&dispatcher, root, "targets");

        for name in ["clear", "reset"] {
            assert_executable(&dispatcher, child(&dispatcher, targets, name), name);
        }

        for name in ["title", "subtitle", "actionbar"] {
            let text = child(&dispatcher, child(&dispatcher, targets, name), "title");
            assert_eq!(
                dispatcher.node(text).and_then(|node| node.argument_type()),
                Some(&SteelArgumentType::component()),
                "{name} should take a component"
            );
            assert_executable(&dispatcher, text, name);
        }

        // Only the last duration is executable; vanilla requires all three.
        let fade_in = child(&dispatcher, child(&dispatcher, targets, "times"), "fadeIn");
        let stay = child(&dispatcher, fade_in, "stay");
        let Some(fade_in_node) = dispatcher.node(fade_in) else {
            panic!("fadeIn should exist");
        };
        assert!(!fade_in_node.is_executable());
        let Some(stay_node) = dispatcher.node(stay) else {
            panic!("stay should exist");
        };
        assert!(!stay_node.is_executable());
        assert_executable(&dispatcher, child(&dispatcher, stay, "fadeOut"), "fadeOut");
    }
}
