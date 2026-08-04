//! Grants and clears mob effects, mirroring vanilla `EffectCommands`.

use steel_registry::mob_effect::MobEffectRef;
use steel_utils::{Identifier, translations};
use text_components::{
    TextComponent,
    translation::{TranslatedMessage, Translation},
};

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::position::missing_position_argument;
use crate::entity::{LivingEntity, MobEffectInstance, SharedEntity};

/// Vanilla's default duration for a lasting effect, in ticks.
const DEFAULT_DURATION_TICKS: i32 = 600;
/// The duration vanilla stores for an effect that never expires.
const INFINITE_DURATION: i32 = -1;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("effect"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("effect").then(clear_branch()).then(give_branch())
}

fn clear_branch() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("clear")
        .executes(|context: &SteelCommandContext<CommandSource>| {
            let Some(entity) = context.source().entity().cloned() else {
                return Err(CommandSyntaxError::dynamic(TextComponent::from(
                    &translations::PERMISSIONS_REQUIRES_ENTITY,
                )));
            };
            clear_all(context, &[entity])
        })
        .then(
            argument("targets", SteelArgumentType::entities())
                .executes(|context| {
                    let targets = context.optional_entities("targets")?;
                    clear_all(context, &targets)
                })
                .then(
                    argument("effect", SteelArgumentType::mob_effect()).executes(|context| {
                        let targets = context.optional_entities("targets")?;
                        clear_one(context, &targets)
                    }),
                ),
        )
}

fn give_branch() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("give").then(
        argument("targets", SteelArgumentType::entities()).then(
            argument("effect", SteelArgumentType::mob_effect())
                .executes(|context| give(context, None, 0, true))
                .then(
                    argument("seconds", ArgumentType::integer(1, 1_000_000))
                        .executes(|context| {
                            let seconds = context.integer("seconds");
                            give(context, seconds, 0, true)
                        })
                        .then(amplifier_and_particles(false)),
                )
                .then(literal("infinite").then(amplifier_and_particles(true))),
        ),
    )
}

/// The shared `<amplifier> [hideParticles]` tail under both `<seconds>` and `infinite`.
fn amplifier_and_particles(
    infinite: bool,
) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let seconds_of = move |context: &SteelCommandContext<CommandSource>| {
        if infinite {
            Some(INFINITE_DURATION)
        } else {
            context.integer("seconds")
        }
    };
    let mut builder = argument("amplifier", ArgumentType::integer(0, 255));
    if infinite {
        builder = builder.executes(move |context| give(context, Some(INFINITE_DURATION), 0, true));
    }
    builder
        .executes(move |context| {
            let amplifier = context.integer("amplifier").unwrap_or(0);
            give(context, seconds_of(context), amplifier, true)
        })
        .then(
            argument("hideParticles", ArgumentType::bool()).executes(move |context| {
                let amplifier = context.integer("amplifier").unwrap_or(0);
                let hide = context.boolean("hideParticles").unwrap_or(false);
                give(context, seconds_of(context), amplifier, !hide)
            }),
        )
}

fn give(
    context: &SteelCommandContext<CommandSource>,
    seconds: Option<i32>,
    amplifier: i32,
    particles: bool,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let targets = context.optional_entities("targets")?;
    let Some(effect) = context.mob_effect("effect") else {
        return Err(missing_position_argument("effect"));
    };

    let duration = duration_ticks(effect, seconds);
    let mut count = 0_i32;
    for target in &targets {
        let Some(living) = target.as_living_entity() else {
            continue;
        };
        let instance = MobEffectInstance::with_duration(effect, duration, amplifier)
            .with_visible(particles)
            .with_show_icon(particles);
        if living.add_mob_effect(instance) {
            count += 1;
        }
    }

    if count == 0 {
        return Err(failed(&translations::COMMANDS_EFFECT_GIVE_FAILED));
    }

    // Vanilla passes the duration as a third argument, but neither success string has a
    // third placeholder, so the client never renders it.
    let message = if let [only] = targets.as_slice() {
        translations::COMMANDS_EFFECT_GIVE_SUCCESS_SINGLE
            .message([
                effect_name(effect),
                TextComponent::plain(only.plain_text_name()),
            ])
            .component()
    } else {
        translations::COMMANDS_EFFECT_GIVE_SUCCESS_MULTIPLE
            .message([
                effect_name(effect),
                TextComponent::plain(targets.len().to_string()),
            ])
            .component()
    };
    source.send_success(&message, true);
    Ok(count)
}

/// Mirrors vanilla's duration rules, which differ for instantaneous effects.
fn duration_ticks(effect: MobEffectRef, seconds: Option<i32>) -> i32 {
    match seconds {
        // An instantaneous effect stores the given number as ticks, not seconds.
        Some(seconds) if effect.is_instantaneous() => seconds,
        Some(INFINITE_DURATION) => INFINITE_DURATION,
        Some(seconds) => seconds * 20,
        None if effect.is_instantaneous() => 1,
        None => DEFAULT_DURATION_TICKS,
    }
}

fn clear_all(
    context: &SteelCommandContext<CommandSource>,
    targets: &[SharedEntity],
) -> Result<i32, CommandSyntaxError> {
    let mut count = 0_i32;
    for target in targets {
        if target
            .as_living_entity()
            .is_some_and(LivingEntity::remove_all_mob_effects)
        {
            count += 1;
        }
    }
    if count == 0 {
        return Err(failed(
            &translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_FAILED,
        ));
    }

    let message = if let [only] = targets {
        translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_SUCCESS_SINGLE
            .message([TextComponent::plain(only.plain_text_name())])
            .component()
    } else {
        translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_SUCCESS_MULTIPLE
            .message([TextComponent::plain(targets.len().to_string())])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(count)
}

fn clear_one(
    context: &SteelCommandContext<CommandSource>,
    targets: &[SharedEntity],
) -> Result<i32, CommandSyntaxError> {
    let Some(effect) = context.mob_effect("effect") else {
        return Err(missing_position_argument("effect"));
    };

    let mut count = 0_i32;
    for target in targets {
        if target
            .as_living_entity()
            .is_some_and(|living| living.remove_mob_effect(effect))
        {
            count += 1;
        }
    }
    if count == 0 {
        return Err(failed(&translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_FAILED));
    }

    let message = if let [only] = targets {
        translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_SINGLE
            .message([
                effect_name(effect),
                TextComponent::plain(only.plain_text_name()),
            ])
            .component()
    } else {
        translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_MULTIPLE
            .message([
                effect_name(effect),
                TextComponent::plain(targets.len().to_string()),
            ])
            .component()
    };
    context.source().send_success(&message, true);
    Ok(count)
}

/// Vanilla sends the effect's translated display name, which the client resolves.
fn effect_name(effect: MobEffectRef) -> TextComponent {
    TextComponent::translated(TranslatedMessage {
        key: format!("effect.{}.{}", effect.key.namespace, effect.key.path).into(),
        fallback: None,
        args: None,
    })
}

fn failed(translation: &'static Translation<0>) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(translation))
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use super::{DEFAULT_DURATION_TICKS, INFINITE_DURATION, duration_ticks};
    use steel_registry::test_support::init_test_registry;
    use steel_registry::{REGISTRY, RegistryExt as _, mob_effect::MobEffectRef};
    use steel_utils::Identifier;

    fn effect(path: &'static str) -> MobEffectRef {
        let Some(effect) = REGISTRY
            .mob_effects
            .by_key(&Identifier::vanilla_static(path))
        else {
            panic!("{path} should be a registered mob effect");
        };
        effect
    }

    /// An instantaneous effect reads the number as ticks; a lasting one reads it as seconds.
    #[test]
    fn instantaneous_effects_use_a_different_duration_scale() {
        init_test_registry();
        let speed = effect("speed");
        let instant = effect("instant_health");

        assert_eq!(duration_ticks(speed, Some(5)), 100);
        assert_eq!(duration_ticks(instant, Some(5)), 5);

        assert_eq!(duration_ticks(speed, None), DEFAULT_DURATION_TICKS);
        assert_eq!(duration_ticks(instant, None), 1);

        // `infinite` stays -1 rather than being multiplied into -20.
        assert_eq!(
            duration_ticks(speed, Some(INFINITE_DURATION)),
            INFINITE_DURATION
        );
    }

    /// Saturation is instantaneous in vanilla despite not being named "instant".
    #[test]
    fn saturation_is_instantaneous() {
        init_test_registry();
        assert!(effect("saturation").is_instantaneous());
        assert!(effect("instant_damage").is_instantaneous());
        assert!(!effect("regeneration").is_instantaneous());
    }

    #[test]
    fn effect_registers_give_and_clear_branches() {
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
                .is_some_and(|node| node.name() == "effect")
        }) else {
            panic!("effect root should exist");
        };
        let Some(children) = dispatcher.children(root) else {
            panic!("effect children should exist");
        };
        let mut names = Vec::new();
        for child in children {
            let Some(node) = dispatcher.node(*child) else {
                panic!("effect child should exist");
            };
            names.push(node.name());
        }
        names.sort_unstable();
        assert_eq!(names, ["clear", "give"]);
    }
}
