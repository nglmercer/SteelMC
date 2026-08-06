//! Lists available commands and shows help for a specific command.

use steel_utils::Identifier;
use text_components::TextComponent;

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{CommandSource, SteelCommandContext, SteelCommandRuntime, argument, literal},
    registration::CommandRegistration,
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("help"), |_| command()).default_access()
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("help")
        .executes(help_list)
        .then(argument("command", ArgumentType::word()).executes(help_for_command))
}

fn help_list(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    // Static list matching the built-in slice in `builtins::tests::first_builtin_slice_has_the_expected_graph_shape`.
    // Permission-filtered listing would require dispatcher access which is private; this still gives
    // players a useful overview and fixes the "Unknown command help" console error.
    const COMMANDS: &[&str] = &[
        "attribute",
        "clear",
        "clone",
        "deop",
        "damage",
        "difficulty",
        "domain",
        "enchant",
        "effect",
        "help",
        "me",
        "execute",
        "experience",
        "xp",
        "fill",
        "fly",
        "gamemode",
        "gamerule",
        "give",
        "kick",
        "kill",
        "list",
        "locate",
        "msg",
        "tell",
        "w",
        "op",
        "playsound",
        "perms",
        "return",
        "rotate",
        "save-all",
        "save-off",
        "save-on",
        "say",
        "seed",
        "setblock",
        "setworldspawn",
        "spawnpoint",
        "stop",
        "stopsound",
        "summon",
        "teleport",
        "tp",
        "tellraw",
        "tick",
        "time",
        "title",
        "weather",
        "worldborder",
        "invsee",
    ];
    let formatted = COMMANDS.join(", ");
    let message = TextComponent::plain(format!("Available commands: {formatted}"));
    source.send_success(&message, false);
    Ok(COMMANDS.len() as i32)
}

fn help_for_command(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let Some(command_name) = context.string("command") else {
        return Err(CommandSyntaxError::dynamic("Missing command argument"));
    };
    let source = context.source();
    // Try to give minimal feedback – full usage strings would need dispatcher access.
    let message = TextComponent::plain(format!(
        "Help for /{command_name}: try /{command_name} with TAB completion for arguments"
    ));
    source.send_success(&message, false);
    Ok(1)
}
