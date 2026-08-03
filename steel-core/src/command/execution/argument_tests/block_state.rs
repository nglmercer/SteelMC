use super::*;
use crate::command::execution::BlockInput;
use steel_registry::REGISTRY;

fn parse_state(input: &str) -> Result<BlockInput, ()> {
    init_test_registry();
    let dispatcher = resource_dispatcher(SteelArgumentType::block_state());
    let command = format!("resource {input}");
    let parse = dispatcher.parse(&command, TestSource::new());
    let chain = dispatcher.context_chain(parse).map_err(|_| ())?;
    chain.top_context().block_state("value").cloned().ok_or(())
}

#[test]
fn block_state_argument_defaults_unspecified_properties() {
    let Ok(input) = parse_state("stone") else {
        panic!("a concrete block should parse");
    };

    assert_eq!(input.state(), vanilla_blocks::STONE.default_state());
    assert!(input.defined_properties().is_empty());
    assert!(input.nbt().is_none());
}

#[test]
fn block_state_argument_records_only_the_properties_that_were_written() {
    // `oak_stairs` has several properties; naming one must leave the rest at their
    // defaults while `defined_properties` records just the one, because placement
    // re-applies exactly those over a shape-updated state.
    let Ok(input) = parse_state("oak_stairs[facing=north]") else {
        panic!("a block with properties should parse");
    };

    let properties = REGISTRY.blocks.get_properties(input.state());
    assert!(properties.contains(&("facing", "north")));
    assert_eq!(input.defined_properties(), [Box::from("facing")]);
}

#[test]
fn block_state_argument_rejects_tags() {
    // Vanilla's BlockStateArgument parses with forTesting = false, so `#tag` is an error
    // here even though the block *predicate* parser accepts it.
    assert!(parse_state("#minecraft:stairs").is_err());
}

#[test]
fn block_state_argument_rejects_unknown_blocks_and_properties() {
    assert!(parse_state("definitely_not_a_block").is_err());
    assert!(parse_state("stone[facing=north]").is_err());
    assert!(parse_state("oak_stairs[facing=sideways]").is_err());
    assert!(parse_state("oak_stairs[facing=north,facing=south]").is_err());
    assert!(parse_state("oak_stairs[facing=north").is_err());
    assert!(parse_state("oak_stairs[facing]").is_err());
}
