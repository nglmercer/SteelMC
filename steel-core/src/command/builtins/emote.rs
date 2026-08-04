//! Broadcasts an emote, mirroring vanilla `EmoteCommands`.

use steel_protocol::packets::game::ChatTypeBound;
use steel_registry::{RegistryEntry as _, vanilla_chat_types};
use steel_utils::Identifier;

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
    CommandRegistration::new(Identifier::vanilla_static("me"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("me").then(argument("action", SteelArgumentType::message()).executes(emote))
}

fn emote(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let Some(action) = context.message("action") else {
        return Err(missing_position_argument("action"));
    };
    let resolved = action.to_component(source)?;

    let chat_type = ChatTypeBound {
        registry_id: vanilla_chat_types::EMOTE_COMMAND.id() as i32,
        sender_name: source.display_name(),
        target_name: None,
    };
    source
        .server()
        .broadcast_disguised_chat(&resolved, &chat_type);
    Ok(1)
}
