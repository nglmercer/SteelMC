//! Disconnects players, mirroring vanilla `KickCommand`.

use steel_utils::{Identifier, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("kick"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("kick").then(
        argument("targets", SteelArgumentType::players())
            .executes(|context| {
                kick(
                    context,
                    &TextComponent::from(&translations::MULTIPLAYER_DISCONNECT_KICKED),
                )
            })
            .then(
                argument("reason", SteelArgumentType::message()).executes(|context| {
                    let Some(reason) = context.message("reason") else {
                        return Err(CommandSyntaxError::dynamic(
                            "Parsed value for reason is missing from the command context",
                        ));
                    };
                    let reason = reason.to_component(context.source())?;
                    kick(context, &reason)
                }),
            ),
    )
}

fn kick(
    context: &SteelCommandContext<CommandSource>,
    reason: &TextComponent,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let targets = context.players("targets")?;

    // Vanilla refuses to kick the LAN host and refuses entirely in singleplayer. A dedicated
    // server has neither, so every resolved target is kickable.
    let mut count = 0_i32;
    for target in &targets {
        target.disconnect(reason.clone());
        let message = translations::COMMANDS_KICK_SUCCESS
            .message([
                TextComponent::plain(target.gameprofile.name.clone()),
                reason.clone(),
            ])
            .component();
        source.send_success(&message, true);
        count += 1;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use crate::command::execution::SteelArgumentType;
    use steel_registry::test_support::init_test_registry;

    /// `/kick` takes players, and the reason is optional.
    #[test]
    fn kick_takes_players_and_an_optional_reason() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let Some(roots) = dispatcher.children(dispatcher.root()) else {
            panic!("dispatcher root should exist");
        };
        let Some(root) = roots.iter().copied().find(|root| {
            dispatcher
                .node(*root)
                .is_some_and(|node| node.name() == "kick")
        }) else {
            panic!("kick root should exist");
        };

        let Some(children) = dispatcher.children(root) else {
            panic!("kick targets should exist");
        };
        let Some(targets) = dispatcher.node(children[0]) else {
            panic!("kick targets should exist");
        };
        assert_eq!(targets.name(), "targets");
        assert!(targets.is_executable());
        assert_eq!(targets.argument_type(), Some(&SteelArgumentType::players()));

        let Some(reason_children) = dispatcher.children(children[0]) else {
            panic!("kick reason should exist");
        };
        let Some(reason) = dispatcher.node(reason_children[0]) else {
            panic!("kick reason should exist");
        };
        assert_eq!(reason.name(), "reason");
        assert!(reason.is_executable());
        assert_eq!(reason.argument_type(), Some(&SteelArgumentType::message()));
    }
}
