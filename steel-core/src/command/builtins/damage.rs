//! Applies damage to an entity, mirroring vanilla `DamageCommand`.

use steel_registry::{RegistryExt as _, damage_type::DamageTypeRef};
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
use crate::entity::damage::DamageSource;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("damage"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("damage").then(
        argument("target", SteelArgumentType::entity()).then(
            argument("amount", ArgumentType::float(0.0, f32::MAX))
                .executes(|context| damage(context, Source::Generic))
                .then(
                    argument("damageType", SteelArgumentType::damage_type())
                        .executes(|context| damage(context, Source::Typed))
                        .then(
                            literal("at").then(
                                argument("location", SteelArgumentType::vec3(true))
                                    .executes(|context| damage(context, Source::At)),
                            ),
                        )
                        .then(
                            literal("by").then(
                                argument("entity", SteelArgumentType::entity())
                                    .executes(|context| damage(context, Source::By))
                                    .then(
                                        literal("from").then(
                                            argument("cause", SteelArgumentType::entity())
                                                .executes(|context| damage(context, Source::From)),
                                        ),
                                    ),
                            ),
                        ),
                ),
        ),
    )
}

/// Which optional context the damage carries.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    /// No damage type given; vanilla falls back to `generic`.
    Generic,
    /// A damage type, with no attacker or position.
    Typed,
    /// Dealt from a position, as an explosion would be.
    At,
    /// Dealt directly by an entity.
    By,
    /// Dealt by an entity on behalf of another, as a projectile is.
    From,
}

fn damage(
    context: &SteelCommandContext<CommandSource>,
    kind: Source,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let target = context.entity("target")?;
    let Some(amount) = context.float("amount") else {
        return Err(missing_position_argument("amount"));
    };

    let damage_source = build_source(context, kind)?;
    let Some(living) = target.as_living_entity() else {
        // Only living entities take damage; vanilla reports this as invulnerability.
        return Err(invulnerable());
    };
    if !living.hurt_server(source.world(), &damage_source, amount) {
        return Err(invulnerable());
    }

    let message = translations::COMMANDS_DAMAGE_SUCCESS
        .message([
            TextComponent::plain(amount.to_string()),
            TextComponent::plain(target.plain_text_name()),
        ])
        .component();
    source.send_success(&message, true);
    Ok(1)
}

fn build_source(
    context: &SteelCommandContext<CommandSource>,
    kind: Source,
) -> Result<DamageSource, CommandSyntaxError> {
    let damage_type = match kind {
        Source::Generic => generic_damage_type()?,
        _ => context
            .damage_type("damageType")
            .ok_or_else(|| missing_position_argument("damageType"))?,
    };
    let mut damage_source = DamageSource::environment(damage_type);

    match kind {
        Source::Generic | Source::Typed => {}
        Source::At => {
            let position = context
                .coordinates("location")
                .ok_or_else(|| missing_position_argument("location"))?
                .position(context.source());
            damage_source = damage_source.with_source_position(position);
        }
        Source::By => {
            let dealer = context.entity("entity")?;
            damage_source = damage_source.with_direct_entity(dealer.id());
        }
        Source::From => {
            // `by <entity> from <cause>`: the first dealt the blow, the second is blamed.
            let dealer = context.entity("entity")?;
            let cause = context.entity("cause")?;
            damage_source = damage_source
                .with_direct_entity(dealer.id())
                .with_causing_entity(cause.id());
        }
    }
    Ok(damage_source)
}

fn generic_damage_type() -> Result<DamageTypeRef, CommandSyntaxError> {
    steel_registry::REGISTRY
        .damage_types
        .by_key(&Identifier::vanilla_static("generic"))
        .ok_or_else(|| CommandSyntaxError::dynamic("the generic damage type is not registered"))
}

fn invulnerable() -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(
        &translations::COMMANDS_DAMAGE_INVULNERABLE,
    ))
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

    /// Each optional stage stays executable, so the damage type, position and attacker are
    /// all independently optional.
    #[test]
    fn damage_graph_makes_every_stage_optional() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let amount = child(
            &dispatcher,
            child(
                &dispatcher,
                child(&dispatcher, dispatcher.root(), "damage"),
                "target",
            ),
            "amount",
        );
        assert_executable(&dispatcher, amount, "amount");

        let damage_type = child(&dispatcher, amount, "damageType");
        assert_eq!(
            dispatcher
                .node(damage_type)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::damage_type())
        );
        assert_executable(&dispatcher, damage_type, "damageType");

        assert_executable(
            &dispatcher,
            child(
                &dispatcher,
                child(&dispatcher, damage_type, "at"),
                "location",
            ),
            "at location",
        );

        let by = child(&dispatcher, child(&dispatcher, damage_type, "by"), "entity");
        assert_executable(&dispatcher, by, "by entity");
        assert_executable(
            &dispatcher,
            child(&dispatcher, child(&dispatcher, by, "from"), "cause"),
            "from cause",
        );
    }
}
