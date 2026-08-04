//! Saving controls, mirroring vanilla `SaveAllCommand`, `SaveOnCommand` and `SaveOffCommand`.

use std::{io, sync::Arc};

use steel_utils::{Identifier, locks::SyncMutex, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::CommandSyntaxError,
    execution::{
        CommandResultSuspension, CommandResultSuspensionPoll, CommandSource,
        CommandSuspensionOrder, SteelCommandContext, literal,
    },
    registration::CommandRegistration,
};
use crate::{server::Server, world::World};

pub(super) fn save_all_registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("save-all"), |_| {
        literal("save-all")
            .executes_suspended(|context| start_save(context, false))
            .then(literal("flush").executes_suspended(|context| start_save(context, true)))
    })
}

pub(super) fn save_on_registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("save-on"), |_| {
        literal("save-on").executes(|context| set_autosave(context, true))
    })
}

pub(super) fn save_off_registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("save-off"), |_| {
        literal("save-off").executes(|context| set_autosave(context, false))
    })
}

fn set_autosave(
    context: &SteelCommandContext<CommandSource>,
    enabled: bool,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let server = source.server();

    if server.set_autosave_enabled(enabled) {
        let message = if enabled {
            TextComponent::from(&translations::COMMANDS_SAVE_ENABLED)
        } else {
            TextComponent::from(&translations::COMMANDS_SAVE_DISABLED)
        };
        source.send_success(&message, true);
        return Ok(1);
    }

    // Vanilla reports an unchanged toggle as a failure rather than a no-op success.
    let already = if enabled {
        &translations::COMMANDS_SAVE_ALREADY_ON
    } else {
        &translations::COMMANDS_SAVE_ALREADY_OFF
    };
    Err(CommandSyntaxError::dynamic(TextComponent::from(already)))
}

/// The outcome of the background save, written once the spawned task finishes.
type SaveOutcome = Arc<SyncMutex<Option<io::Result<usize>>>>;

/// Waits for a save that runs off the game tick.
///
/// Saving is asynchronous and may take many ticks, which a tick may never block on, so the
/// command suspends until the spawned task reports back.
struct SaveAllSuspension {
    outcome: SaveOutcome,
    source: CommandSource,
}

impl CommandResultSuspension for SaveAllSuspension {
    fn order(&self) -> CommandSuspensionOrder {
        // A save reads every world, so later commands must not mutate them meanwhile.
        CommandSuspensionOrder::Global
    }

    fn poll(&mut self) -> CommandResultSuspensionPoll {
        let Some(outcome) = self.outcome.lock().take() else {
            return CommandResultSuspensionPoll::Pending;
        };
        match outcome {
            Ok(_) => {
                self.source.send_success(
                    &TextComponent::from(&translations::COMMANDS_SAVE_SUCCESS),
                    true,
                );
                CommandResultSuspensionPoll::Ready(Ok(1))
            }
            Err(error) => {
                log::error!("/save-all failed: {error}");
                CommandResultSuspensionPoll::Ready(Err(CommandSyntaxError::dynamic(
                    TextComponent::from(&translations::COMMANDS_SAVE_FAILED),
                )))
            }
        }
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "command executors share one fallible callback signature"
)]
fn start_save(
    context: &SteelCommandContext<CommandSource>,
    flush: bool,
) -> Result<SaveAllSuspension, CommandSyntaxError> {
    let source = context.source();
    source.send_success(
        &TextComponent::from(&translations::COMMANDS_SAVE_SAVING),
        false,
    );

    let server = source.server();
    let outcome: SaveOutcome = Arc::new(SyncMutex::new(None));
    let worlds = server.worlds.values().map(Arc::clone).collect::<Vec<_>>();
    let runtime = Arc::clone(&source.world().chunk_map.chunk_runtime);
    let server = Arc::clone(server);

    let task_outcome = Arc::clone(&outcome);
    runtime.spawn(async move {
        let result = save_everything(&server, &worlds, flush).await;
        *task_outcome.lock() = Some(result);
    });

    Ok(SaveAllSuspension {
        outcome,
        source: source.clone(),
    })
}

/// Saves every world's chunks and the shared command data, reporting the chunk count.
///
/// `flush` is accepted for vanilla parity but Steel's chunk saver always writes through, so
/// there is no buffered state left to force out.
async fn save_everything(
    server: &Arc<Server>,
    worlds: &[Arc<World>],
    _flush: bool,
) -> io::Result<usize> {
    let mut saved = 0;
    for world in worlds {
        saved += world.save_all_chunks().await?;
    }

    let command_data = server.save_command_data().await;
    command_data.scoreboards?;
    command_data.storage?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::super::create_dispatcher;
    use steel_registry::test_support::init_test_registry;

    /// `save-all` takes an optional `flush`; the toggles take nothing.
    #[test]
    fn save_commands_register_with_their_expected_shapes() {
        init_test_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let Some(roots) = dispatcher.children(dispatcher.root()) else {
            panic!("dispatcher root should exist");
        };
        let find = |name: &str| {
            roots
                .iter()
                .copied()
                .find(|root| {
                    dispatcher
                        .node(*root)
                        .is_some_and(|node| node.name() == name)
                })
                .unwrap_or_else(|| panic!("{name} root should exist"))
        };

        for name in ["save-on", "save-off"] {
            let root = find(name);
            let Some(node) = dispatcher.node(root) else {
                panic!("{name} should exist");
            };
            assert!(node.is_executable());
            assert!(
                dispatcher.children(root).is_none_or(<[_]>::is_empty),
                "{name} takes no arguments"
            );
        }

        let save_all = find("save-all");
        let Some(save_all_node) = dispatcher.node(save_all) else {
            panic!("save-all should exist");
        };
        assert!(save_all_node.is_executable());
        let Some(children) = dispatcher.children(save_all) else {
            panic!("save-all should have a flush child");
        };
        assert_eq!(children.len(), 1);
        let Some(flush) = dispatcher.node(children[0]) else {
            panic!("flush should exist");
        };
        assert_eq!(flush.name(), "flush");
        assert!(flush.is_executable());
    }
}
