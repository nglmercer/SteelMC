//! Sets players' personal respawn points, mirroring vanilla `SetSpawnCommand`.

use std::sync::Arc;

use steel_utils::{BlockPos, Identifier, translations};
use text_components::{TextComponent, translation::Translation};

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use super::position::missing_position_argument;
use crate::{player::Player, player::player_data::PersistentRespawn, world::World};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("spawnpoint"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("spawnpoint")
        .executes(|context| set_spawn(context, false, false))
        .then(
            argument("targets", SteelArgumentType::players())
                .executes(|context| set_spawn(context, true, false))
                .then(
                    argument("pos", SteelArgumentType::block_pos())
                        .executes(|context| set_spawn(context, true, true))
                        .then(
                            argument("rotation", SteelArgumentType::rotation())
                                .executes(|context| set_spawn(context, true, true)),
                        ),
                ),
        )
}

fn set_spawn(
    context: &SteelCommandContext<CommandSource>,
    explicit_targets: bool,
    explicit_position: bool,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world = source.world();

    let targets = if explicit_targets {
        context.players("targets")?
    } else {
        // Vanilla defaults to the calling player, and errors if there is none.
        let Some(player) = source.player() else {
            return Err(CommandSyntaxError::dynamic(TextComponent::from(
                &translations::PERMISSIONS_REQUIRES_PLAYER,
            )));
        };
        vec![Arc::clone(player)]
    };

    let position = if explicit_position {
        spawnable_position(context)?
    } else {
        BlockPos::from(source.position())
    };
    let (yaw, pitch) = context
        .coordinates("rotation")
        .map_or((0.0, 0.0), |rotation| rotation.rotation(source));

    for target in &targets {
        target.set_respawn_point(Some(PersistentRespawn {
            world: world.key.to_string(),
            pos: [position.x(), position.y(), position.z()],
            yaw,
            pitch,
            // A commanded point is forced, so it survives the block becoming unusable.
            forced: true,
        }));
    }

    report(context, &targets, position, yaw, world);
    i32::try_from(targets.len())
        .map_err(|_| CommandSyntaxError::dynamic("Player count exceeds the command result range"))
}

/// Rejects a position outside vanilla's spawnable bounds, as `getSpawnablePos` does.
fn spawnable_position(
    context: &SteelCommandContext<CommandSource>,
) -> Result<BlockPos, CommandSyntaxError> {
    let Some(coordinates) = context.coordinates("pos") else {
        return Err(missing_position_argument("pos"));
    };
    let position = coordinates.block_pos(context.source());
    if !World::is_in_spawnable_bounds(position) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::ARGUMENT_POS_OUTOFBOUNDS,
        )));
    }
    Ok(position)
}

/// Reports the new spawn point.
///
/// Vanilla passes both yaw and pitch, but neither success string has a placeholder for the
/// pitch, so the client renders only the yaw.
fn report(
    context: &SteelCommandContext<CommandSource>,
    targets: &[Arc<Player>],
    position: BlockPos,
    yaw: f32,
    world: &Arc<World>,
) {
    let mut arguments = [
        TextComponent::plain(position.x().to_string()),
        TextComponent::plain(position.y().to_string()),
        TextComponent::plain(position.z().to_string()),
        TextComponent::plain(yaw.to_string()),
        TextComponent::plain(world.key.to_string()),
        TextComponent::plain(String::new()),
    ];
    let translation: &'static Translation<6> = if let [only] = targets {
        arguments[5] = TextComponent::plain(only.gameprofile.name.clone());
        &translations::COMMANDS_SPAWNPOINT_SUCCESS_SINGLE
    } else {
        arguments[5] = TextComponent::plain(targets.len().to_string());
        &translations::COMMANDS_SPAWNPOINT_SUCCESS_MULTIPLE
    };
    let message = translation.message(arguments).component();
    context.source().send_success(&message, true);
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use steel_registry::test_support::init_test_registry;

    /// Every stage is optional: bare, with targets, with a position, and with a rotation.
    #[test]
    fn spawnpoint_graph_makes_every_stage_optional() {
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
                .is_some_and(|node| node.name() == "spawnpoint")
        }) else {
            panic!("spawnpoint root should exist");
        };

        let mut node = root;
        for name in ["spawnpoint", "targets", "pos", "rotation"] {
            let Some(current) = dispatcher.node(node) else {
                panic!("{name} should exist");
            };
            assert!(current.is_executable(), "{name} should be executable");
            let Some(children) = dispatcher.children(node) else {
                break;
            };
            let Some(next) = children.first().copied() else {
                break;
            };
            node = next;
        }
    }
}
