//! Sends a private message, mirroring vanilla `MsgCommand`.

use steel_protocol::packets::game::{CDisguisedChat, ChatTypeBound};
use steel_registry::{RegistryEntry as _, vanilla_chat_types};
use steel_utils::Identifier;
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::position::missing_position_argument;
use crate::entity::Entity as _;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("msg"), |_| command())
        .alias("tell")
        .alias("w")
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("msg").then(
        argument("targets", SteelArgumentType::players())
            .then(argument("message", SteelArgumentType::message()).executes(send)),
    )
}

fn send(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let targets = context.optional_players("targets")?;
    // Vanilla resolves the message only when there is somebody to send it to.
    if targets.is_empty() {
        return Ok(0);
    }
    let Some(message) = context.message("message") else {
        return Err(missing_position_argument("message"));
    };
    let resolved = message.to_component(source)?;

    let incoming = ChatTypeBound {
        registry_id: vanilla_chat_types::MSG_COMMAND_INCOMING.id() as i32,
        sender_name: source.display_name(),
        target_name: None,
    };

    for target in &targets {
        // The sender sees an outgoing copy naming the recipient; the recipient sees an
        // incoming one naming the sender.
        let outgoing = ChatTypeBound {
            registry_id: vanilla_chat_types::MSG_COMMAND_OUTGOING.id() as i32,
            sender_name: source.display_name(),
            target_name: Some(TextComponent::plain(target.plain_text_name())),
        };
        send_to_source(source, &resolved, &outgoing);
        target.send_packet(CDisguisedChat::new(&resolved, incoming.clone(), &**target));
    }

    i32::try_from(targets.len())
        .map_err(|_| CommandSyntaxError::dynamic("Target count exceeds the command result range"))
}

/// Sends the outgoing copy back to the sender when the sender is a player.
///
/// A console or command-block source has no chat stream of its own; vanilla routes the copy
/// through the source's own message sink, which for those is the server log.
fn send_to_source(source: &CommandSource, content: &TextComponent, chat_type: &ChatTypeBound) {
    let Some(player) = source.player() else {
        source.sender().send_message(content);
        return;
    };
    player.send_packet(CDisguisedChat::new(content, chat_type.clone(), player));
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use crate::command::execution::SteelArgumentType;
    use steel_registry::test_support::init_test_registry;

    /// `/msg` and both of its aliases expose the same targets/message shape.
    #[test]
    fn msg_and_its_aliases_take_targets_and_a_message() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let Some(roots) = dispatcher.children(dispatcher.root()) else {
            panic!("dispatcher root should exist");
        };

        for name in ["msg", "tell", "w"] {
            let Some(root) = roots.iter().copied().find(|root| {
                dispatcher
                    .node(*root)
                    .is_some_and(|node| node.name() == name)
            }) else {
                panic!("{name} root should exist");
            };
            let Some(children) = dispatcher.children(root) else {
                panic!("{name} should have a targets child");
            };
            let Some(targets) = dispatcher.node(children[0]) else {
                panic!("{name} targets should exist");
            };
            assert_eq!(targets.name(), "targets");
            assert_eq!(targets.argument_type(), Some(&SteelArgumentType::players()));

            let Some(target_children) = dispatcher.children(children[0]) else {
                panic!("{name} should have a message child");
            };
            let Some(message) = dispatcher.node(target_children[0]) else {
                panic!("{name} message should exist");
            };
            assert_eq!(message.name(), "message");
            assert!(message.is_executable());
            assert_eq!(message.argument_type(), Some(&SteelArgumentType::message()));
        }
    }
}
