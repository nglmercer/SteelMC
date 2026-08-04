//! Broadcasts a message to every player, mirroring vanilla `SayCommand`.

use steel_protocol::packets::game::ChatTypeBound;
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

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("say"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("say").then(argument("message", SteelArgumentType::message()).executes(say))
}

fn say(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let Some(message) = context.message("message") else {
        return Err(missing_position_argument("message"));
    };
    let resolved = message.to_component(source)?;

    // Vanilla sends this through the signed-chat path only when a player typed it with a
    // signature attached; a command-issued message is always disguised, which is what the
    // server itself and `/execute run say` produce.
    broadcast_disguised(source, &resolved);
    Ok(1)
}

fn broadcast_disguised(source: &CommandSource, content: &TextComponent) {
    let chat_type = ChatTypeBound {
        registry_id: vanilla_chat_types::SAY_COMMAND.id() as i32,
        sender_name: source.display_name(),
        target_name: None,
    };
    source
        .server()
        .broadcast_disguised_chat(content, &chat_type);
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use crate::command::execution::SteelArgumentType;
    use steel_registry::test_support::init_test_registry;

    /// `/say` takes a message argument and is not executable on its own.
    #[test]
    fn say_graph_requires_a_message_argument() {
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
                .is_some_and(|node| node.name() == "say")
        }) else {
            panic!("say root should exist");
        };
        let Some(root_node) = dispatcher.node(root) else {
            panic!("say root should exist");
        };
        assert!(!root_node.is_executable());

        let Some(children) = dispatcher.children(root) else {
            panic!("say children should exist");
        };
        assert_eq!(children.len(), 1);
        let Some(message) = dispatcher.node(children[0]) else {
            panic!("say message should exist");
        };
        assert_eq!(message.name(), "message");
        assert!(message.is_executable());
        assert_eq!(message.argument_type(), Some(&SteelArgumentType::message()));
    }
}
