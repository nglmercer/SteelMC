//! Turns an entity to face a rotation, position or other entity, mirroring vanilla
//! `RotateCommand`.

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
use super::position::missing_position_argument;
use crate::entity::{EntityAnchor, SharedEntity};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("rotate"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("rotate").then(
        argument("target", SteelArgumentType::entity())
            .then(argument("rotation", SteelArgumentType::rotation()).executes(rotate_to))
            .then(
                literal("facing")
                    .then(
                        argument("facingLocation", SteelArgumentType::vec3(true))
                            .executes(face_location),
                    )
                    .then(
                        literal("entity").then(
                            argument("facingEntity", SteelArgumentType::entity())
                                .executes(|context| face_entity(context, EntityAnchor::Feet))
                                .then(
                                    argument("facingAnchor", SteelArgumentType::entity_anchor())
                                        .executes(|context| {
                                            let Some(anchor) =
                                                context.entity_anchor("facingAnchor")
                                            else {
                                                return Err(missing_position_argument(
                                                    "facingAnchor",
                                                ));
                                            };
                                            face_entity(context, anchor)
                                        }),
                                ),
                        ),
                    ),
            ),
    )
}

fn rotate_to(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let Some(coordinates) = context.coordinates("rotation") else {
        return Err(missing_position_argument("rotation"));
    };

    // Vanilla resolves the rotation against the source, converts it to a delta from the
    // entity's own rotation, then re-applies that delta -- the two cancel, so the entity ends
    // up at the source-resolved rotation either way. The relative flags only survive to tell
    // the client to interpolate.
    target.set_rotation(coordinates.rotation(context.source()));
    report(context, &target)
}

fn face_location(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let Some(coordinates) = context.coordinates("facingLocation") else {
        return Err(missing_position_argument("facingLocation"));
    };
    let position = coordinates.position(context.source());

    target.look_at(EntityAnchor::Feet, position);
    report(context, &target)
}

fn face_entity(
    context: &SteelCommandContext<CommandSource>,
    anchor: EntityAnchor,
) -> Result<i32, CommandSyntaxError> {
    let target = context.entity("target")?;
    let facing = context.entity("facingEntity")?;

    // The anchor picks which part of the *faced* entity to aim at.
    target.look_at(EntityAnchor::Feet, anchor.position(facing.as_ref()));
    report(context, &target)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "command executors share one fallible callback signature"
)]
fn report(
    context: &SteelCommandContext<CommandSource>,
    target: &SharedEntity,
) -> Result<i32, CommandSyntaxError> {
    let message = translations::COMMANDS_ROTATE_SUCCESS
        .message([TextComponent::plain(target.plain_text_name())])
        .component();
    context.source().send_success(&message, true);
    Ok(1)
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

    /// A rotation, a location, or another entity with an optional anchor.
    #[test]
    fn rotate_graph_covers_rotation_and_both_facing_forms() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let target = child(
            &dispatcher,
            child(&dispatcher, dispatcher.root(), "rotate"),
            "target",
        );

        let rotation = child(&dispatcher, target, "rotation");
        assert_eq!(
            dispatcher
                .node(rotation)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::rotation())
        );
        assert_executable(&dispatcher, rotation, "rotation");

        let facing = child(&dispatcher, target, "facing");
        assert_executable(
            &dispatcher,
            child(&dispatcher, facing, "facingLocation"),
            "facingLocation",
        );

        // The anchor is optional, so the entity itself is already executable.
        let facing_entity = child(
            &dispatcher,
            child(&dispatcher, facing, "entity"),
            "facingEntity",
        );
        assert_executable(&dispatcher, facing_entity, "facingEntity");
        assert_executable(
            &dispatcher,
            child(&dispatcher, facing_entity, "facingAnchor"),
            "facingAnchor",
        );
    }
}
