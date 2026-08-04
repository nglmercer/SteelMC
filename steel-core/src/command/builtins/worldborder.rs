//! Reads and edits the world border, mirroring vanilla `WorldBorderCommand`.

use steel_utils::{Identifier, translations};
use text_components::{TextComponent, translation::Translation};

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::position::missing_position_argument;

/// The furthest a border centre may sit from the origin.
const MAX_CENTER: f64 = 2.999_998_4e7;
/// The widest a border may be made.
const MAX_SIZE: f64 = 5.999_997e7;

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("worldborder"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("worldborder")
        .then(size_branch("add", true))
        .then(size_branch("set", false))
        .then(
            literal("center")
                .then(argument("pos", SteelArgumentType::vec2(false)).executes(set_center)),
        )
        .then(damage_branch())
        .then(literal("get").executes(get_size))
        .then(warning_branch())
}

/// `add` offsets the current size; `set` replaces it. Both take an optional lerp time.
fn size_branch(
    name: &'static str,
    relative: bool,
) -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal(name).then(
        argument("distance", ArgumentType::double(-MAX_SIZE, MAX_SIZE))
            .executes(move |context| set_size(context, relative, 0))
            .then(
                argument("time", SteelArgumentType::time(0)).executes(move |context| {
                    let ticks = context.time("time").unwrap_or(0);
                    set_size(context, relative, i64::from(ticks))
                }),
            ),
    )
}

fn damage_branch() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("damage")
        .then(
            literal("amount").then(
                argument("damagePerBlock", ArgumentType::float(0.0, f32::MAX))
                    .executes(set_damage_amount),
            ),
        )
        .then(literal("buffer").then(
            argument("distance", ArgumentType::float(0.0, f32::MAX)).executes(set_damage_buffer),
        ))
}

fn warning_branch() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("warning")
        .then(literal("distance").then(
            argument("distance", ArgumentType::integer(0, i32::MAX)).executes(set_warning_distance),
        ))
        .then(
            literal("time")
                .then(argument("time", SteelArgumentType::time(0)).executes(set_warning_time)),
        )
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "command executors share one fallible callback signature"
)]
fn get_size(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let size = source.world().world_border_snapshot().old_size;
    let message = translations::COMMANDS_WORLDBORDER_GET
        .message([format!("{size:.0}")])
        .component();
    source.send_success(&message, false);
    // Vanilla returns the rounded width.
    Ok((size + 0.5).floor() as i32)
}

fn set_size(
    context: &SteelCommandContext<CommandSource>,
    relative: bool,
    ticks: i64,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let Some(distance) = context.double("distance") else {
        return Err(missing_position_argument("distance"));
    };

    let current = world.world_border_snapshot().old_size;
    let target = if relative {
        current + distance
    } else {
        distance
    };

    if (current - target).abs() < f64::EPSILON {
        return Err(failed(
            &translations::COMMANDS_WORLDBORDER_SET_FAILED_NOCHANGE,
        ));
    }
    if target < 1.0 {
        return Err(failed(&translations::COMMANDS_WORLDBORDER_SET_FAILED_SMALL));
    }
    if target > MAX_SIZE {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_WORLDBORDER_SET_FAILED_BIG
                .message([MAX_SIZE.to_string()])
                .component(),
        ));
    }

    let formatted = format!("{target:.1}");
    let message = if ticks > 0 {
        world
            .lerp_world_border_size_between(current, target, ticks)
            .map_err(|error| CommandSyntaxError::dynamic(error.to_string()))?;
        let seconds = (ticks / 20).to_string();
        let translation = if target > current {
            &translations::COMMANDS_WORLDBORDER_SET_GROW
        } else {
            &translations::COMMANDS_WORLDBORDER_SET_SHRINK
        };
        translation.message([formatted, seconds]).component()
    } else {
        world
            .set_world_border_size(target)
            .map_err(|error| CommandSyntaxError::dynamic(error.to_string()))?;
        translations::COMMANDS_WORLDBORDER_SET_IMMEDIATE
            .message([formatted])
            .component()
    };
    source.send_success(&message, true);
    Ok(0)
}

fn set_center(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let Some(coordinates) = context.coordinates("pos") else {
        return Err(missing_position_argument("pos"));
    };
    let position = coordinates.position(source);
    let (x, z) = (position.x, position.z);

    let snapshot = world.world_border_snapshot();
    if (snapshot.center_x - x).abs() < f64::EPSILON && (snapshot.center_z - z).abs() < f64::EPSILON
    {
        return Err(failed(&translations::COMMANDS_WORLDBORDER_CENTER_FAILED));
    }
    if x.abs() > MAX_CENTER || z.abs() > MAX_CENTER {
        return Err(CommandSyntaxError::dynamic(
            translations::COMMANDS_WORLDBORDER_SET_FAILED_FAR
                .message([MAX_CENTER.to_string()])
                .component(),
        ));
    }

    world
        .set_world_border_center(x, z)
        .map_err(|error| CommandSyntaxError::dynamic(error.to_string()))?;
    let message = translations::COMMANDS_WORLDBORDER_CENTER_SUCCESS
        .message([format!("{x:.2}"), format!("{z:.2}")])
        .component();
    source.send_success(&message, true);
    Ok(0)
}

fn set_damage_amount(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let Some(amount) = context.float("damagePerBlock") else {
        return Err(missing_position_argument("damagePerBlock"));
    };
    let amount = f64::from(amount);

    if (world.world_border_snapshot().damage_per_block - amount).abs() < f64::EPSILON {
        return Err(failed(
            &translations::COMMANDS_WORLDBORDER_DAMAGE_AMOUNT_FAILED,
        ));
    }
    world
        .set_world_border_damage_per_block(amount)
        .map_err(|error| CommandSyntaxError::dynamic(error.to_string()))?;
    let message = translations::COMMANDS_WORLDBORDER_DAMAGE_AMOUNT_SUCCESS
        .message([format!("{amount:.2}")])
        .component();
    source.send_success(&message, true);
    Ok(0)
}

fn set_damage_buffer(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let Some(distance) = context.float("distance") else {
        return Err(missing_position_argument("distance"));
    };
    let distance = f64::from(distance);

    if (world.world_border_snapshot().safe_zone - distance).abs() < f64::EPSILON {
        return Err(failed(
            &translations::COMMANDS_WORLDBORDER_DAMAGE_BUFFER_FAILED,
        ));
    }
    world
        .set_world_border_safe_zone(distance)
        .map_err(|error| CommandSyntaxError::dynamic(error.to_string()))?;
    let message = translations::COMMANDS_WORLDBORDER_DAMAGE_BUFFER_SUCCESS
        .message([format!("{distance:.2}")])
        .component();
    source.send_success(&message, true);
    Ok(0)
}

fn set_warning_distance(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let Some(distance) = context.integer("distance") else {
        return Err(missing_position_argument("distance"));
    };

    if world.world_border_snapshot().warning_blocks == distance {
        return Err(failed(
            &translations::COMMANDS_WORLDBORDER_WARNING_DISTANCE_FAILED,
        ));
    }
    world.set_world_border_warning_blocks(distance);
    let message = translations::COMMANDS_WORLDBORDER_WARNING_DISTANCE_SUCCESS
        .message([distance.to_string()])
        .component();
    source.send_success(&message, true);
    Ok(0)
}

fn set_warning_time(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();
    let Some(ticks) = context.time("time") else {
        return Err(missing_position_argument("time"));
    };

    if world.world_border_snapshot().warning_time == ticks {
        return Err(failed(
            &translations::COMMANDS_WORLDBORDER_WARNING_TIME_FAILED,
        ));
    }
    world.set_world_border_warning_time(ticks);
    let message = translations::COMMANDS_WORLDBORDER_WARNING_TIME_SUCCESS
        .message([ticks.to_string()])
        .component();
    source.send_success(&message, true);
    Ok(0)
}

fn failed(translation: &'static Translation<0>) -> CommandSyntaxError {
    CommandSyntaxError::dynamic(TextComponent::from(translation))
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

    /// `add` and `set` both take an optional lerp time; `center` takes the vec2 pair.
    #[test]
    fn worldborder_graph_covers_every_subcommand() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "worldborder");

        for name in ["add", "set"] {
            let distance = child(&dispatcher, child(&dispatcher, root, name), "distance");
            assert_executable(&dispatcher, distance, name);
            assert_executable(
                &dispatcher,
                child(&dispatcher, distance, "time"),
                "lerp time",
            );
        }

        let center = child(&dispatcher, child(&dispatcher, root, "center"), "pos");
        assert_eq!(
            dispatcher
                .node(center)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::vec2(false))
        );
        assert_executable(&dispatcher, center, "center pos");

        assert_executable(&dispatcher, child(&dispatcher, root, "get"), "get");

        let damage = child(&dispatcher, root, "damage");
        assert_executable(
            &dispatcher,
            child(
                &dispatcher,
                child(&dispatcher, damage, "amount"),
                "damagePerBlock",
            ),
            "damage amount",
        );
        assert_executable(
            &dispatcher,
            child(
                &dispatcher,
                child(&dispatcher, damage, "buffer"),
                "distance",
            ),
            "damage buffer",
        );

        let warning = child(&dispatcher, root, "warning");
        assert_executable(
            &dispatcher,
            child(
                &dispatcher,
                child(&dispatcher, warning, "distance"),
                "distance",
            ),
            "warning distance",
        );
        assert_executable(
            &dispatcher,
            child(&dispatcher, child(&dispatcher, warning, "time"), "time"),
            "warning time",
        );
    }
}
