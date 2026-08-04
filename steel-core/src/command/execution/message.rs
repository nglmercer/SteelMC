//! Free-text command arguments that may embed entity selectors.
// Selector resolution is complete ahead of the commands that consume it; `/say`, `/msg`,
// `/me` and `/teammsg` are the callers it was written for. Drop this once they land.
#![expect(
    dead_code,
    reason = "message resolution lands before /say, /msg, /me and /teammsg consume it"
)]

use steel_utils::translations;
use text_components::{Modifier as _, TextComponent, format::Color};

use super::{
    CommandArgumentSource, CommandSource,
    selector::{
        EntitySelector, allow_advanced_selectors, allow_selectors,
        parse_selector_plan_with_permissions,
    },
};
use crate::command::brigadier::{CommandSyntaxError, CommandSyntaxErrorKind, StringReader};

/// Vanilla's chat length cap, enforced by `MessageArgument.Message.parseText`.
const MAX_LENGTH: usize = 256;

/// One selector embedded in a message, with its byte span inside the message text.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MessagePart {
    start: usize,
    end: usize,
    selector: EntitySelector,
}

/// Free text that may contain entity selectors, mirroring vanilla `MessageArgument.Message`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MessageArgument {
    text: String,
    parts: Vec<MessagePart>,
}

impl MessageArgument {
    /// The message exactly as typed, with selectors left unexpanded.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Resolves the message for `source`, expanding selectors to entity names.
    ///
    /// Mirrors vanilla `MessageArgument.Message.toComponent`. Selectors are expanded only when
    /// the source may use them; otherwise the text is returned verbatim, so a player without
    /// selector permission sees their `@` characters as ordinary text rather than an error.
    pub(crate) fn to_component(
        &self,
        source: &CommandSource,
    ) -> Result<TextComponent, CommandSyntaxError> {
        if self.parts.is_empty() || !allow_selectors(source) {
            return Ok(TextComponent::plain(self.text.clone()));
        }

        let mut result = TextComponent::new();
        let mut read_to = 0;
        for part in &self.parts {
            if read_to < part.start {
                result.children.push(TextComponent::plain(
                    self.text[read_to..part.start].to_owned(),
                ));
            }
            result
                .children
                .push(join_entity_names(&part.selector, source)?);
            read_to = part.end;
        }
        if read_to < self.text.len() {
            result
                .children
                .push(TextComponent::plain(self.text[read_to..].to_owned()));
        }
        Ok(result)
    }
}

/// Joins the selected entities' display names, as vanilla `EntitySelector.joinNames` does.
fn join_entity_names(
    selector: &EntitySelector,
    source: &CommandSource,
) -> Result<TextComponent, CommandSyntaxError> {
    let entities = selector.find_entities(source)?;
    let mut joined = TextComponent::new();
    for (index, entity) in entities.iter().enumerate() {
        if index > 0 {
            joined
                .children
                .push(TextComponent::plain(", ").color(Color::Gray));
        }
        joined.children.push(entity.display_name());
    }
    Ok(joined)
}

/// Parses the rest of the input as a message, recording any selectors it embeds.
pub(super) fn parse_message(
    reader: &mut StringReader<'_>,
    source: &dyn CommandArgumentSource,
) -> Result<MessageArgument, CommandSyntaxError> {
    let text = reader.remaining().to_owned();
    if text.chars().count() > MAX_LENGTH {
        let message = translations::ARGUMENT_MESSAGE_TOO_LONG
            .message([text.chars().count().to_string(), MAX_LENGTH.to_string()])
            .component();
        return Err(reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(message))));
    }

    // A source that may not use selectors takes the text verbatim; vanilla does not report the
    // lack of permission here, it simply stops looking for selectors.
    if !allow_selectors(source) {
        reader.advance_bytes(text.len());
        return Ok(MessageArgument {
            text,
            parts: Vec::new(),
        });
    }

    let offset = reader.read_so_far().len();
    let mut parts = Vec::new();
    while reader.can_read() {
        if reader.peek() != Some('@') {
            reader.skip();
            continue;
        }

        let start = reader.checkpoint();
        let start_byte = reader.read_so_far().len();
        let raw = read_selector_text(reader);
        match parse_selector_plan_with_permissions(&raw, true, allow_advanced_selectors(source)) {
            Ok(selector) => parts.push(MessagePart {
                start: start_byte - offset,
                end: reader.read_so_far().len() - offset,
                selector,
            }),
            // Vanilla only recovers from "that was not a selector at all"; a malformed
            // selector such as `@e[` is still an error.
            Err(error) if error.is_not_a_selector() => {
                reader.restore(start);
                reader.skip();
            }
            Err(error) => {
                return Err(
                    reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(error.message())))
                );
            }
        }
    }

    Ok(MessageArgument { text, parts })
}

/// Consumes exactly one `@selector[options]` token, stopping right after it.
///
/// Unlike the standalone selector argument this must not run to the next space: vanilla's
/// incremental parser stops at the closing bracket, so `@e[type=pig]hi` leaves `hi` as text.
fn read_selector_text(reader: &mut StringReader<'_>) -> String {
    let mut raw = String::new();
    // The leading '@', then the selector type character.
    for _ in 0..2 {
        let Some(character) = reader.peek() else {
            return raw;
        };
        raw.push(character);
        reader.skip();
    }

    if reader.peek() != Some('[') {
        return raw;
    }

    let mut depth = 0_usize;
    let mut quote = None;
    let mut escaped = false;
    while let Some(character) = reader.peek() {
        raw.push(character);
        reader.skip();
        if escaped {
            escaped = false;
            continue;
        }
        if quote.is_some() {
            if character == '\\' {
                escaped = true;
            } else if quote == Some(character) {
                quote = None;
            }
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    raw
}
