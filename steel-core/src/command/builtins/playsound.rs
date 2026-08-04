//! Plays a sound to chosen players, mirroring vanilla `PlaySoundCommand`.

use std::sync::Arc;

use glam::{DVec3, IVec3};
use steel_protocol::packets::game::{CSound, SoundHolder, SoundSource};
use steel_utils::{Identifier, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::position::missing_position_argument;
use crate::entity::Entity as _;

/// Vanilla's every source category, in the order `SoundSource` declares them.
const SOURCES: [(&str, SoundSource); 11] = [
    ("master", SoundSource::Master),
    ("music", SoundSource::Music),
    ("record", SoundSource::Records),
    ("weather", SoundSource::Weather),
    ("block", SoundSource::Blocks),
    ("hostile", SoundSource::Hostile),
    ("neutral", SoundSource::Neutral),
    ("player", SoundSource::Players),
    ("ambient", SoundSource::Ambient),
    ("voice", SoundSource::Voice),
    ("ui", SoundSource::Ui),
];

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("playsound"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let mut root = literal("playsound");
    for (name, source) in SOURCES {
        root = root.then(literal(name).then(sound_argument(source)));
    }
    root
}

/// Builds `<sound> [targets] [pos] [volume] [pitch] [minVolume]` under one source category.
fn sound_argument(source: SoundSource) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    argument("sound", SteelArgumentType::resource_location())
        .executes(move |context| play(context, source, None, None, 1.0, 1.0, 0.0))
        .then(
            argument("targets", SteelArgumentType::players())
                .executes(move |context| {
                    play(context, source, Some("targets"), None, 1.0, 1.0, 0.0)
                })
                .then(
                    argument("pos", SteelArgumentType::vec3(true))
                        .executes(move |context| {
                            play(context, source, Some("targets"), Some("pos"), 1.0, 1.0, 0.0)
                        })
                        .then(
                            argument("volume", ArgumentType::float(0.0, f32::MAX))
                                .executes(move |context| {
                                    let volume = context.float("volume").unwrap_or(1.0);
                                    play(
                                        context,
                                        source,
                                        Some("targets"),
                                        Some("pos"),
                                        volume,
                                        1.0,
                                        0.0,
                                    )
                                })
                                .then(
                                    argument("pitch", ArgumentType::float(0.0, 2.0))
                                        .executes(move |context| {
                                            let volume = context.float("volume").unwrap_or(1.0);
                                            let pitch = context.float("pitch").unwrap_or(1.0);
                                            play(
                                                context,
                                                source,
                                                Some("targets"),
                                                Some("pos"),
                                                volume,
                                                pitch,
                                                0.0,
                                            )
                                        })
                                        .then(
                                            argument("minVolume", ArgumentType::float(0.0, 1.0))
                                                .executes(move |context| {
                                                    let volume =
                                                        context.float("volume").unwrap_or(1.0);
                                                    let pitch =
                                                        context.float("pitch").unwrap_or(1.0);
                                                    let min_volume =
                                                        context.float("minVolume").unwrap_or(0.0);
                                                    play(
                                                        context,
                                                        source,
                                                        Some("targets"),
                                                        Some("pos"),
                                                        volume,
                                                        pitch,
                                                        min_volume,
                                                    )
                                                }),
                                        ),
                                ),
                        ),
                ),
        )
}

fn play(
    context: &SteelCommandContext<CommandSource>,
    sound_source: SoundSource,
    targets_argument: Option<&str>,
    position_argument: Option<&str>,
    volume: f32,
    pitch: f32,
    min_volume: f32,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let Some(sound) = context.identifier("sound").cloned() else {
        return Err(missing_position_argument("sound"));
    };

    let targets = match targets_argument {
        Some(name) => context.players(name)?,
        // Vanilla defaults to the calling player alone.
        None => source
            .player()
            .map(|player| vec![Arc::clone(player)])
            .unwrap_or_default(),
    };
    let position = match position_argument {
        Some(name) => context
            .coordinates(name)
            .ok_or_else(|| missing_position_argument(name))?
            .position(source),
        None => source.position(),
    };

    // A variable-range sound is audible within 16 blocks, scaled up by a volume above one.
    let range = if volume > 1.0 { 16.0 * volume } else { 16.0 };
    let max_distance_squared = f64::from(range * range);
    let seed = rand::random::<i64>();

    let mut played_for = Vec::new();
    for target in &targets {
        let delta = position - target.position();
        let distance_squared = delta.length_squared();

        let (heard_at, heard_volume) = if distance_squared > max_distance_squared {
            if min_volume <= 0.0 {
                continue;
            }
            // Out of range but audible at the floor volume: vanilla moves the sound to two
            // blocks from the listener, along the direction it would have come from.
            let distance = distance_squared.sqrt();
            (target.position() + delta / distance * 2.0, min_volume)
        } else {
            (position, volume)
        };

        target.send_packet(sound_packet(
            &sound,
            sound_source,
            heard_at,
            heard_volume,
            pitch,
            seed,
        ));
        played_for.push(target);
    }

    let Some(&only) = played_for.first().filter(|_| played_for.len() == 1) else {
        if played_for.is_empty() {
            return Err(CommandSyntaxError::dynamic(TextComponent::from(
                &translations::COMMANDS_PLAYSOUND_FAILED,
            )));
        }
        let message = translations::COMMANDS_PLAYSOUND_SUCCESS_MULTIPLE
            .message([sound.to_string(), played_for.len().to_string()])
            .component();
        source.send_success(&message, true);
        return i32::try_from(played_for.len()).map_err(|_| {
            CommandSyntaxError::dynamic("Player count exceeds the command result range")
        });
    };

    let message = translations::COMMANDS_PLAYSOUND_SUCCESS_SINGLE
        .message([sound.to_string(), only.gameprofile.name.clone()])
        .component();
    source.send_success(&message, true);
    Ok(1)
}

/// Vanilla writes the sound inline rather than looking it up, so any id is accepted.
fn sound_packet(
    sound: &Identifier,
    source: SoundSource,
    position: DVec3,
    volume: f32,
    pitch: f32,
    seed: i64,
) -> CSound {
    CSound {
        sound: SoundHolder::Direct {
            key: sound.clone(),
            fixed_range: None,
        },
        source: source.as_varint(),
        pos: IVec3::new(
            (position.x * 8.0) as i32,
            (position.y * 8.0) as i32,
            (position.z * 8.0) as i32,
        ),
        volume,
        pitch,
        seed,
    }
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use super::SOURCES;
    use steel_registry::test_support::init_test_registry;

    /// Every sound category is its own literal, each carrying the full argument chain.
    #[test]
    fn playsound_repeats_the_argument_chain_under_every_source() {
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
                .is_some_and(|node| node.name() == "playsound")
        }) else {
            panic!("playsound root should exist");
        };
        let Some(categories) = dispatcher.children(root) else {
            panic!("playsound categories should exist");
        };
        assert_eq!(categories.len(), SOURCES.len());

        for category in categories {
            let Some(node) = dispatcher.node(*category) else {
                panic!("category node should exist");
            };
            assert!(!node.is_executable(), "a category alone is not executable");

            let Some(sound) = dispatcher.children(*category) else {
                panic!("category should carry a sound argument");
            };
            let Some(sound_node) = dispatcher.node(sound[0]) else {
                panic!("sound argument should exist");
            };
            assert_eq!(sound_node.name(), "sound");
            assert!(sound_node.is_executable());
        }
    }
}
