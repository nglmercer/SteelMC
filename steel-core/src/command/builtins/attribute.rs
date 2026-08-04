//! Reads and edits entity attributes, mirroring vanilla `AttributeCommand`.

use steel_registry::attribute::AttributeRef;
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
use crate::entity::{
    SharedEntity,
    attribute::{AttributeModifier, AttributeModifierOperation},
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("attribute"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("attribute").then(
        argument("target", SteelArgumentType::entity()).then(
            argument("attribute", SteelArgumentType::attribute())
                .then(scaled("get", get_value))
                .then(base_branch())
                .then(modifier_branch()),
        ),
    )
}

/// A `<literal> [scale]` pair, since vanilla repeats that shape three times.
fn scaled(
    name: &'static str,
    read: fn(&SteelCommandContext<CommandSource>, f64) -> Result<i32, CommandSyntaxError>,
) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal(name)
        .executes(move |context| read(context, 1.0))
        .then(
            argument("scale", ArgumentType::double(f64::MIN, f64::MAX)).executes(move |context| {
                let scale = context.double("scale").unwrap_or(1.0);
                read(context, scale)
            }),
        )
}

fn base_branch() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("base")
        .then(
            literal("set").then(
                argument("value", ArgumentType::double(f64::MIN, f64::MAX)).executes(set_base),
            ),
        )
        .then(scaled("get", get_base))
        .then(literal("reset").executes(reset_base))
}

fn modifier_branch() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    let mut operations = argument("value", ArgumentType::double(f64::MIN, f64::MAX));
    for (name, operation) in [
        ("add_value", AttributeModifierOperation::AddValue),
        (
            "add_multiplied_base",
            AttributeModifierOperation::AddMultipliedBase,
        ),
        (
            "add_multiplied_total",
            AttributeModifierOperation::AddMultipliedTotal,
        ),
    ] {
        operations = operations
            .then(literal(name).executes(move |context| add_modifier(context, operation)));
    }
    let add = argument("id", SteelArgumentType::resource_location()).then(operations);

    literal("modifier")
        .then(literal("add").then(add))
        .then(
            literal("remove").then(
                argument("id", SteelArgumentType::resource_location()).executes(remove_modifier),
            ),
        )
        .then(
            literal("value").then(
                literal("get").then(
                    argument("id", SteelArgumentType::resource_location())
                        .executes(|context| get_modifier_value(context, 1.0))
                        .then(
                            argument("scale", ArgumentType::double(f64::MIN, f64::MAX)).executes(
                                |context| {
                                    let scale = context.double("scale").unwrap_or(1.0);
                                    get_modifier_value(context, scale)
                                },
                            ),
                        ),
                ),
            ),
        )
}

/// Resolves the target and attribute, rejecting a non-living entity or a missing attribute.
///
/// Mirrors vanilla `getEntityWithAttribute`.
fn target_and_attribute(
    context: &SteelCommandContext<CommandSource>,
) -> Result<(SharedEntity, AttributeRef), CommandSyntaxError> {
    let target = context.entity("target")?;
    let Some(attribute) = context.attribute("attribute") else {
        return Err(missing_position_argument("attribute"));
    };

    let Some(living) = target.as_living_entity() else {
        return Err(one_arg(
            &translations::COMMANDS_ATTRIBUTE_FAILED_ENTITY,
            [TextComponent::plain(target.plain_text_name())],
        ));
    };
    if !living.attributes().lock().has_attribute(attribute) {
        return Err(two_args(
            &translations::COMMANDS_ATTRIBUTE_FAILED_NO_ATTRIBUTE,
            [
                TextComponent::plain(target.plain_text_name()),
                attribute_name(attribute),
            ],
        ));
    }
    Ok((target, attribute))
}

fn get_value(
    context: &SteelCommandContext<CommandSource>,
    scale: f64,
) -> Result<i32, CommandSyntaxError> {
    let (target, attribute) = target_and_attribute(context)?;
    let Some(living) = target.as_living_entity() else {
        return Err(missing_position_argument("target"));
    };
    let value = living
        .attributes()
        .lock()
        .get_value(attribute)
        .unwrap_or(0.0);
    report(
        context,
        &translations::COMMANDS_ATTRIBUTE_VALUE_GET_SUCCESS,
        attribute,
        &target,
        value,
    );
    Ok(scaled_result(value, scale))
}

fn get_base(
    context: &SteelCommandContext<CommandSource>,
    scale: f64,
) -> Result<i32, CommandSyntaxError> {
    let (target, attribute) = target_and_attribute(context)?;
    let Some(living) = target.as_living_entity() else {
        return Err(missing_position_argument("target"));
    };
    let value = living
        .attributes()
        .lock()
        .get_base_value(attribute)
        .unwrap_or(0.0);
    report(
        context,
        &translations::COMMANDS_ATTRIBUTE_BASE_VALUE_GET_SUCCESS,
        attribute,
        &target,
        value,
    );
    Ok(scaled_result(value, scale))
}

fn set_base(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let (target, attribute) = target_and_attribute(context)?;
    let Some(value) = context.double("value") else {
        return Err(missing_position_argument("value"));
    };
    let Some(living) = target.as_living_entity() else {
        return Err(missing_position_argument("target"));
    };
    living.attributes().lock().set_base_value(attribute, value);
    report(
        context,
        &translations::COMMANDS_ATTRIBUTE_BASE_VALUE_SET_SUCCESS,
        attribute,
        &target,
        value,
    );
    Ok(scaled_result(value, 1.0))
}

fn reset_base(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let (target, attribute) = target_and_attribute(context)?;
    let Some(living) = target.as_living_entity() else {
        return Err(missing_position_argument("target"));
    };
    let value = attribute.default_value;
    living.attributes().lock().set_base_value(attribute, value);
    report(
        context,
        &translations::COMMANDS_ATTRIBUTE_BASE_VALUE_RESET_SUCCESS,
        attribute,
        &target,
        value,
    );
    Ok(scaled_result(value, 1.0))
}

fn add_modifier(
    context: &SteelCommandContext<CommandSource>,
    operation: AttributeModifierOperation,
) -> Result<i32, CommandSyntaxError> {
    let (target, attribute) = target_and_attribute(context)?;
    let (id, value) = modifier_id_and_value(context)?;
    let Some(living) = target.as_living_entity() else {
        return Err(missing_position_argument("target"));
    };

    let modifier = AttributeModifier {
        id: id.clone(),
        amount: value,
        operation,
    };
    if !living
        .attributes()
        .lock()
        .add_modifier(attribute, modifier, true)
    {
        return Err(three_args(
            &translations::COMMANDS_ATTRIBUTE_FAILED_MODIFIER_ALREADY_PRESENT,
            [
                TextComponent::plain(id.to_string()),
                attribute_name(attribute),
                TextComponent::plain(target.plain_text_name()),
            ],
        ));
    }

    let message = translations::COMMANDS_ATTRIBUTE_MODIFIER_ADD_SUCCESS
        .message([
            TextComponent::plain(id.to_string()),
            attribute_name(attribute),
            TextComponent::plain(target.plain_text_name()),
        ])
        .component();
    context.source().send_success(&message, true);
    Ok(1)
}

fn remove_modifier(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let (target, attribute) = target_and_attribute(context)?;
    let Some(id) = context.identifier("id").cloned() else {
        return Err(missing_position_argument("id"));
    };
    let Some(living) = target.as_living_entity() else {
        return Err(missing_position_argument("target"));
    };

    if !living.attributes().lock().remove_modifier(attribute, &id) {
        return Err(no_such_modifier(&target, attribute, &id));
    }

    let message = translations::COMMANDS_ATTRIBUTE_MODIFIER_REMOVE_SUCCESS
        .message([
            TextComponent::plain(id.to_string()),
            attribute_name(attribute),
            TextComponent::plain(target.plain_text_name()),
        ])
        .component();
    context.source().send_success(&message, true);
    Ok(1)
}

fn get_modifier_value(
    context: &SteelCommandContext<CommandSource>,
    scale: f64,
) -> Result<i32, CommandSyntaxError> {
    let (target, attribute) = target_and_attribute(context)?;
    let Some(id) = context.identifier("id").cloned() else {
        return Err(missing_position_argument("id"));
    };
    let Some(living) = target.as_living_entity() else {
        return Err(missing_position_argument("target"));
    };

    let amount = living
        .attributes()
        .lock()
        .get_instance(attribute)
        .and_then(|instance| instance.modifier(&id).map(|modifier| modifier.amount));
    let Some(amount) = amount else {
        return Err(no_such_modifier(&target, attribute, &id));
    };

    let message = translations::COMMANDS_ATTRIBUTE_MODIFIER_VALUE_GET_SUCCESS
        .message([
            TextComponent::plain(id.to_string()),
            attribute_name(attribute),
            TextComponent::plain(target.plain_text_name()),
            TextComponent::plain(amount.to_string()),
        ])
        .component();
    context.source().send_success(&message, false);
    Ok(scaled_result(amount, scale))
}

fn modifier_id_and_value(
    context: &SteelCommandContext<CommandSource>,
) -> Result<(Identifier, f64), CommandSyntaxError> {
    let Some(id) = context.identifier("id").cloned() else {
        return Err(missing_position_argument("id"));
    };
    let Some(value) = context.double("value") else {
        return Err(missing_position_argument("value"));
    };
    Ok((id, value))
}

/// Vanilla's three attribute readouts share this success shape.
fn report(
    context: &SteelCommandContext<CommandSource>,
    translation: &'static Translation<3>,
    attribute: AttributeRef,
    target: &SharedEntity,
    value: f64,
) {
    let message = translation
        .message([
            attribute_name(attribute),
            TextComponent::plain(target.plain_text_name()),
            TextComponent::plain(value.to_string()),
        ])
        .component();
    context.source().send_success(&message, false);
}

/// Vanilla returns the truncated scaled value as the command result.
fn scaled_result(value: f64, scale: f64) -> i32 {
    (value * scale) as i32
}

fn no_such_modifier(
    target: &SharedEntity,
    attribute: AttributeRef,
    id: &Identifier,
) -> CommandSyntaxError {
    three_args(
        &translations::COMMANDS_ATTRIBUTE_FAILED_NO_MODIFIER,
        [
            attribute_name(attribute),
            TextComponent::plain(target.plain_text_name()),
            TextComponent::plain(id.to_string()),
        ],
    )
}

/// Vanilla sends the attribute's translated description, which the client resolves.
fn attribute_name(attribute: AttributeRef) -> TextComponent {
    TextComponent::translated(TranslatedMessage {
        key: attribute.translation_key.into(),
        fallback: None,
        args: None,
    })
}

fn one_arg(
    translation: &'static Translation<1>,
    arguments: [TextComponent; 1],
) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(translation.message(arguments).component())
}

fn two_args(
    translation: &'static Translation<2>,
    arguments: [TextComponent; 2],
) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(translation.message(arguments).component())
}

fn three_args(
    translation: &'static Translation<3>,
    arguments: [TextComponent; 3],
) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(translation.message(arguments).component())
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

    /// Every readout takes an optional scale, and each modifier operation is its own leaf.
    #[test]
    fn attribute_graph_covers_get_base_and_modifier_branches() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "attribute");
        let target = child(&dispatcher, root, "target");
        let attribute = child(&dispatcher, target, "attribute");
        assert_eq!(
            dispatcher
                .node(attribute)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::attribute())
        );

        // `get` is executable bare and with a scale.
        let get = child(&dispatcher, attribute, "get");
        assert_executable(&dispatcher, get, "attribute get");
        assert_executable(&dispatcher, child(&dispatcher, get, "scale"), "get scale");

        let base = child(&dispatcher, attribute, "base");
        assert_executable(&dispatcher, child(&dispatcher, base, "reset"), "base reset");
        let base_get = child(&dispatcher, base, "get");
        assert_executable(&dispatcher, base_get, "base get");
        assert_executable(
            &dispatcher,
            child(&dispatcher, base_get, "scale"),
            "base get scale",
        );
        assert_executable(
            &dispatcher,
            child(&dispatcher, child(&dispatcher, base, "set"), "value"),
            "base set value",
        );

        let modifier = child(&dispatcher, attribute, "modifier");
        let add_value = child(
            &dispatcher,
            child(&dispatcher, child(&dispatcher, modifier, "add"), "id"),
            "value",
        );
        for operation in ["add_value", "add_multiplied_base", "add_multiplied_total"] {
            assert_executable(
                &dispatcher,
                child(&dispatcher, add_value, operation),
                operation,
            );
        }

        assert_executable(
            &dispatcher,
            child(&dispatcher, child(&dispatcher, modifier, "remove"), "id"),
            "modifier remove id",
        );
        let value_get_id = child(
            &dispatcher,
            child(&dispatcher, child(&dispatcher, modifier, "value"), "get"),
            "id",
        );
        assert_executable(&dispatcher, value_get_id, "modifier value get id");
        assert_executable(
            &dispatcher,
            child(&dispatcher, value_get_id, "scale"),
            "modifier value get scale",
        );
    }
}
