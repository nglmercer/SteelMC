use super::*;
use crate::command::execution::MessageArgument;

fn parse_message(input: &str) -> Result<MessageArgument, ()> {
    init_test_registry();
    let dispatcher = resource_dispatcher(SteelArgumentType::message());
    let command = format!("resource {input}");
    let parse = dispatcher.parse(&command, TestSource::new());
    let chain = dispatcher.context_chain(parse).map_err(|_| ())?;
    chain.top_context().message("value").cloned().ok_or(())
}

#[test]
fn message_argument_keeps_the_text_verbatim() {
    let Ok(message) = parse_message("hello world!") else {
        panic!("plain text should parse");
    };
    assert_eq!(message.text(), "hello world!");
}

#[test]
fn message_argument_treats_a_non_selector_at_sign_as_text() {
    // Vanilla recovers from "missing selector type" and "unknown selector type" by stepping
    // past the '@' and continuing to scan, so these stay ordinary characters.
    for input in ["email@example.com", "@", "hi @ there", "@zzz nope"] {
        let Ok(message) = parse_message(input) else {
            panic!("{input} should parse as plain text");
        };
        assert_eq!(message.text(), input);
    }
}

#[test]
fn message_argument_rejects_a_malformed_selector() {
    // Unlike an unknown selector type, a selector that starts validly and then breaks is a
    // hard error rather than literal text.
    assert!(parse_message("look at @e[type=]").is_err());
    assert!(parse_message("look at @e[nonsense=1]").is_err());
}

#[test]
fn message_argument_stops_a_selector_at_its_closing_bracket() {
    // Vanilla's incremental parser ends the selector at ']', so the rest stays text; a
    // whitespace-delimited read would have swallowed "!" into the selector and failed.
    let Ok(message) = parse_message("ping @e[type=pig]!") else {
        panic!("a selector followed by text should parse");
    };
    assert_eq!(message.text(), "ping @e[type=pig]!");
}

#[test]
fn message_argument_rejects_text_over_the_vanilla_length_cap() {
    assert!(parse_message(&"a".repeat(256)).is_ok());
    assert!(parse_message(&"a".repeat(257)).is_err());
}
