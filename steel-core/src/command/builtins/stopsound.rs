//! Stops playing sounds, mirroring vanilla `StopSoundCommand`.

use steel_protocol::packets::game::{CStopSound, SoundSource};
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
use super::playsound::SOURCES;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("stopsound"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    // `*` means every category, so it takes a sound but no category of its own.
    let mut targets = argument("targets", SteelArgumentType::players())
        .executes(|context| stop(context, None))
        .then(literal("*").then(sound_argument(None)));
    for (name, source) in SOURCES {
        targets = targets.then(
            literal(name)
                .executes(move |context| stop(context, Some(source)))
                .then(sound_argument(Some(source))),
        );
    }
    literal("stopsound").then(targets)
}

fn sound_argument(
    source: Option<SoundSource>,
) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    argument("sound", SteelArgumentType::resource_location())
        .executes(move |context| stop(context, source))
}

fn stop(
    context: &SteelCommandContext<CommandSource>,
    source_category: Option<SoundSource>,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let targets = context.players("targets")?;
    let sound = context.identifier("sound").cloned();

    for target in &targets {
        target.send_packet(CStopSound {
            source: source_category,
            sound: sound.clone(),
        });
    }

    let message = match (source_category, sound.as_ref()) {
        (Some(category), Some(sound)) => translations::COMMANDS_STOPSOUND_SUCCESS_SOURCE_SOUND
            .message([sound.to_string(), category_name(category).to_owned()])
            .component(),
        (Some(category), None) => translations::COMMANDS_STOPSOUND_SUCCESS_SOURCE_ANY
            .message([category_name(category).to_owned()])
            .component(),
        (None, Some(sound)) => translations::COMMANDS_STOPSOUND_SUCCESS_SOURCELESS_SOUND
            .message([sound.to_string()])
            .component(),
        (None, None) => {
            TextComponent::from(&translations::COMMANDS_STOPSOUND_SUCCESS_SOURCELESS_ANY)
        }
    };
    source.send_success(&message, true);

    i32::try_from(targets.len())
        .map_err(|_| CommandSyntaxError::dynamic("Player count exceeds the command result range"))
}

/// The vanilla name for a category, which differs from Steel's enum spelling.
fn category_name(source: SoundSource) -> &'static str {
    SOURCES
        .iter()
        .find(|(_, candidate)| *candidate == source)
        .map_or("master", |(name, _)| *name)
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use super::SOURCES;
    use steel_registry::test_support::init_test_registry;

    /// Every category is a literal under `targets`, alongside `*`, and each takes a sound.
    #[test]
    fn stopsound_offers_every_category_and_a_wildcard() {
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
                .is_some_and(|node| node.name() == "stopsound")
        }) else {
            panic!("stopsound root should exist");
        };
        let Some(targets_children) = dispatcher.children(root) else {
            panic!("stopsound targets should exist");
        };
        let targets = targets_children[0];
        let Some(targets_node) = dispatcher.node(targets) else {
            panic!("targets node should exist");
        };
        assert!(targets_node.is_executable());

        let Some(categories) = dispatcher.children(targets) else {
            panic!("categories should exist");
        };
        // Every source category, plus the `*` wildcard.
        assert_eq!(categories.len(), SOURCES.len() + 1);

        let Some(wildcard) = categories.iter().copied().find(|node| {
            dispatcher
                .node(*node)
                .is_some_and(|node| node.name() == "*")
        }) else {
            panic!("the wildcard category should exist");
        };
        let Some(wildcard_node) = dispatcher.node(wildcard) else {
            panic!("wildcard node should exist");
        };
        // `*` alone is not executable; vanilla requires a sound after it.
        assert!(!wildcard_node.is_executable());
    }
}
