//! Block-state and block-entity predicates used by commands.
// The block-state parser is complete ahead of the commands that consume it; `/setblock`,
// `/fill` and `/clone` are the callers it was written for. Drop this once they land.
#![expect(
    dead_code,
    reason = "block-state parsing lands before /setblock, /fill and /clone consume it"
)]

use simdnbt::owned::NbtCompound;
use steel_registry::{
    BLOCKS_REGISTRY, REGISTRY, RegistryExt as _, TaggedRegistryExt as _, blocks::BlockRef,
};
use steel_utils::{BlockStateId, Identifier, nbt::parse_snbt_compound_argument, translations};
use text_components::{TextComponent, translation::Translation};

use super::argument::{matches_substring, parse_identifier, unknown_resource};
use crate::command::brigadier::{
    CommandSyntaxError, CommandSyntaxErrorKind, StringReader, SuggestionsBuilder,
};

type BlockProperties = Vec<(Box<str>, Box<str>)>;

/// A concrete block or block tag with optional state and block-entity constraints.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BlockPredicate {
    Block {
        block: BlockRef,
        properties: BlockProperties,
        nbt: Option<NbtCompound>,
    },
    Tag {
        tag: Identifier,
        properties: BlockProperties,
        nbt: Option<NbtCompound>,
    },
}

impl BlockPredicate {
    pub(crate) fn matches_state(&self, state: BlockStateId) -> bool {
        let Some(actual) = REGISTRY.blocks.by_state_id(state) else {
            return false;
        };
        let properties = match self {
            Self::Block {
                block, properties, ..
            } => {
                if actual != *block {
                    return false;
                }
                properties
            }
            Self::Tag {
                tag, properties, ..
            } => {
                if !actual.has_tag(tag) {
                    return false;
                }
                properties
            }
        };
        state_properties_match(state, properties)
    }

    pub(crate) const fn nbt(&self) -> Option<&NbtCompound> {
        match self {
            Self::Block { nbt, .. } | Self::Tag { nbt, .. } => nbt.as_ref(),
        }
    }
}

/// A concrete block state together with the properties the argument spelled out.
///
/// Mirrors vanilla `BlockInput`. Only the explicitly written properties are recorded,
/// because placement re-applies them over a shape-updated state and `/fill … replace`
/// matches on them alone; the block's remaining properties keep their default values.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BlockInput {
    state: BlockStateId,
    defined_properties: Vec<Box<str>>,
    nbt: Option<NbtCompound>,
}

impl BlockInput {
    pub(crate) const fn state(&self) -> BlockStateId {
        self.state
    }

    pub(crate) fn defined_properties(&self) -> &[Box<str>] {
        &self.defined_properties
    }

    pub(crate) const fn nbt(&self) -> Option<&NbtCompound> {
        self.nbt.as_ref()
    }

    /// Tests `actual` the way vanilla `BlockInput::test` does, ignoring block-entity NBT.
    ///
    /// NBT comparison needs the block entity at a position, so callers that care about it
    /// check [`Self::nbt`] separately.
    pub(crate) fn matches_state(&self, actual: BlockStateId) -> bool {
        if REGISTRY.blocks.by_state_id(actual) != REGISTRY.blocks.by_state_id(self.state) {
            return false;
        }
        let expected = REGISTRY.blocks.get_properties(self.state);
        let found = REGISTRY.blocks.get_properties(actual);
        self.defined_properties.iter().all(|name| {
            let value = |properties: &[(&'static str, &'static str)]| {
                properties
                    .iter()
                    .find(|(property, _)| *property == name.as_ref())
                    .map(|(_, value)| *value)
            };
            value(&expected) == value(&found)
        })
    }
}

fn state_properties_match(state: BlockStateId, expected: &BlockProperties) -> bool {
    let actual = REGISTRY.blocks.get_properties(state);
    expected.iter().all(|(name, value)| {
        actual.iter().any(|(actual_name, actual_value)| {
            *actual_name == name.as_ref() && *actual_value == value.as_ref()
        })
    })
}

pub(super) fn parse_block_predicate(
    reader: &mut StringReader<'_>,
) -> Result<BlockPredicate, CommandSyntaxError> {
    if reader.peek() == Some('#') {
        reader.skip();
        return parse_tag_predicate(reader);
    }
    parse_concrete_block_predicate(reader)
}

fn parse_concrete_block_predicate(
    reader: &mut StringReader<'_>,
) -> Result<BlockPredicate, CommandSyntaxError> {
    let key = parse_identifier(reader)?;
    let Some(block) = REGISTRY.blocks.by_key(&key) else {
        return Err(unknown_resource(reader, &key, &BLOCKS_REGISTRY));
    };
    let properties = if reader.peek() == Some('[') {
        parse_properties(reader, Some(block))?
    } else {
        Vec::new()
    };
    let nbt = parse_optional_nbt(reader)?;
    Ok(BlockPredicate::Block {
        block,
        properties,
        nbt,
    })
}

fn parse_tag_predicate(
    reader: &mut StringReader<'_>,
) -> Result<BlockPredicate, CommandSyntaxError> {
    let key = parse_identifier(reader)?;
    if !REGISTRY.blocks.tag_keys().any(|tag| tag == &key) {
        return Err(dynamic_error(reader, format!("Unknown block tag '#{key}'")));
    }
    let properties = if reader.peek() == Some('[') {
        parse_properties(reader, None)?
    } else {
        Vec::new()
    };
    let nbt = parse_optional_nbt(reader)?;
    Ok(BlockPredicate::Tag {
        tag: key,
        properties,
        nbt,
    })
}

/// Parses a concrete block state, as vanilla `BlockStateArgument` does.
///
/// Unlike [`parse_block_predicate`] this rejects `#tags` and resolves the result against the
/// block's default state, so every unspecified property keeps its default value.
pub(super) fn parse_block_state(
    reader: &mut StringReader<'_>,
) -> Result<BlockInput, CommandSyntaxError> {
    let start = reader.checkpoint();
    parse_block_state_inner(reader).inspect_err(|_| reader.restore(start))
}

fn parse_block_state_inner(
    reader: &mut StringReader<'_>,
) -> Result<BlockInput, CommandSyntaxError> {
    if reader.peek() == Some('#') {
        return Err(reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(
            (&translations::ARGUMENT_BLOCK_TAG_DISALLOWED).into(),
        ))));
    }

    let key_start = reader.checkpoint();
    let key = parse_identifier(reader)?;
    let Some(block) = REGISTRY.blocks.by_key(&key) else {
        reader.restore(key_start);
        let message = translations::ARGUMENT_BLOCK_ID_INVALID
            .message([key.to_string()])
            .component();
        return Err(reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(message))));
    };

    let properties = if reader.peek() == Some('[') {
        parse_state_properties(reader, block, &key)?
    } else {
        BlockProperties::new()
    };
    let nbt = parse_optional_nbt(reader)?;

    let state = REGISTRY
        .blocks
        .state_id_from_block_defaulted_properties(
            block,
            properties
                .iter()
                .map(|(name, value)| (name.as_ref(), value.as_ref())),
        )
        .unwrap_or_else(|| block.default_state());

    Ok(BlockInput {
        state,
        defined_properties: properties.into_iter().map(|(name, _)| name).collect(),
        nbt,
    })
}

/// Parses `[name=value, …]` for a known block, reporting vanilla's errors.
fn parse_state_properties(
    reader: &mut StringReader<'_>,
    block: BlockRef,
    key: &Identifier,
) -> Result<BlockProperties, CommandSyntaxError> {
    reader.expect('[')?;
    reader.skip_whitespace();
    let mut properties = BlockProperties::new();

    while reader.can_read() && reader.peek() != Some(']') {
        reader.skip_whitespace();
        let name_start = reader.checkpoint();
        let name = reader.read_string()?;

        let Some(property) = block
            .properties
            .iter()
            .copied()
            .find(|property| property.get_name() == name)
        else {
            reader.restore(name_start);
            return Err(block_property_error(
                reader,
                &translations::ARGUMENT_BLOCK_PROPERTY_UNKNOWN,
                [key.to_string(), name],
            ));
        };
        if properties
            .iter()
            .any(|(existing, _)| existing.as_ref() == name)
        {
            reader.restore(name_start);
            // Vanilla passes the property before the block for this message.
            return Err(block_property_error(
                reader,
                &translations::ARGUMENT_BLOCK_PROPERTY_DUPLICATE,
                [name, key.to_string()],
            ));
        }

        reader.skip_whitespace();
        if reader.peek() != Some('=') {
            // Vanilla passes the property before the block for this message.
            return Err(block_property_error(
                reader,
                &translations::ARGUMENT_BLOCK_PROPERTY_NOVALUE,
                [name, key.to_string()],
            ));
        }
        reader.expect('=')?;
        reader.skip_whitespace();

        let value_start = reader.checkpoint();
        let value = reader.read_string()?;
        if !property
            .get_possible_value_names()
            .contains(&value.as_str())
        {
            reader.restore(value_start);
            // Vanilla orders this one block, value, property.
            return Err(block_property_error(
                reader,
                &translations::ARGUMENT_BLOCK_PROPERTY_INVALID,
                [key.to_string(), value, name],
            ));
        }
        properties.push((name.into(), value.into()));

        reader.skip_whitespace();
        match reader.peek() {
            Some(',') => {
                reader.skip();
            }
            Some(']') => {}
            _ => return Err(unclosed_properties(reader)),
        }
    }

    if reader.peek() != Some(']') {
        return Err(unclosed_properties(reader));
    }
    reader.expect(']')?;
    Ok(properties)
}

fn block_property_error<const N: usize>(
    reader: &StringReader<'_>,
    translation: &'static Translation<N>,
    arguments: [String; N],
) -> CommandSyntaxError {
    let message = translation.message(arguments).component();
    reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(message)))
}

fn unclosed_properties(reader: &StringReader<'_>) -> CommandSyntaxError {
    reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(
        (&translations::ARGUMENT_BLOCK_PROPERTY_UNCLOSED).into(),
    )))
}

fn parse_properties(
    reader: &mut StringReader<'_>,
    block: Option<BlockRef>,
) -> Result<BlockProperties, CommandSyntaxError> {
    reader.expect('[')?;
    reader.skip_whitespace();
    let mut properties = BlockProperties::new();

    while reader.can_read() && reader.peek() != Some(']') {
        reader.skip_whitespace();
        let key = reader.read_string()?;
        if key.is_empty() {
            return Err(dynamic_error(reader, "Expected block property name"));
        }
        if properties
            .iter()
            .any(|(existing, _)| existing.as_ref() == key)
        {
            return Err(dynamic_error(
                reader,
                format!("Duplicate block property '{key}'"),
            ));
        }
        let property = block.and_then(|block| {
            block
                .properties
                .iter()
                .copied()
                .find(|property| property.get_name() == key)
        });
        if block.is_some() && property.is_none() {
            return Err(dynamic_error(
                reader,
                format!("Unknown property '{key}' for block predicate"),
            ));
        }

        reader.skip_whitespace();
        reader.expect('=')?;
        reader.skip_whitespace();
        let value = reader.read_string()?;
        if let Some(property) = property
            && !property
                .get_possible_value_names()
                .contains(&value.as_str())
        {
            return Err(dynamic_error(
                reader,
                format!("Invalid value '{value}' for block property '{key}'"),
            ));
        }
        properties.push((key.into(), value.into()));

        reader.skip_whitespace();
        match reader.peek() {
            Some(',') => {
                reader.skip();
            }
            Some(']') => {}
            _ => return Err(dynamic_error(reader, "Expected ',' or ']'")),
        }
    }

    reader.expect(']')?;
    Ok(properties)
}

fn parse_optional_nbt(
    reader: &mut StringReader<'_>,
) -> Result<Option<NbtCompound>, CommandSyntaxError> {
    if reader.peek() != Some('{') {
        return Ok(None);
    }
    let parsed = parse_snbt_compound_argument(reader.remaining());
    let (nbt, consumed) = match parsed {
        Ok(value) => value,
        Err(error) => {
            if !reader.advance_bytes(error.cursor()) {
                return Err(dynamic_error(reader, "Invalid block entity NBT cursor"));
            }
            return Err(dynamic_error(reader, error.component()));
        }
    };
    if !reader.advance_bytes(consumed) {
        return Err(dynamic_error(reader, "Invalid block entity NBT cursor"));
    }
    Ok(Some(nbt))
}

fn dynamic_error(
    reader: &StringReader<'_>,
    message: impl Into<TextComponent>,
) -> CommandSyntaxError {
    reader.error(CommandSyntaxErrorKind::Dynamic(Box::new(message.into())))
}

pub(super) fn suggest_blocks(builder: &mut SuggestionsBuilder<'_>) {
    let remaining = builder.remaining_lowercase().to_owned();
    if remaining.contains(['[', '{']) {
        return;
    }
    if let Some(prefix) = remaining.strip_prefix('#') {
        for tag in REGISTRY
            .blocks
            .tag_keys()
            .filter(|tag| identifier_matches(prefix, tag))
        {
            builder.suggest(format!("#{tag}"));
        }
        return;
    }
    for block in REGISTRY
        .blocks
        .iter()
        .map(|(_, block)| &block.key)
        .filter(|key| identifier_matches(&remaining, key))
    {
        builder.suggest(block.to_string());
    }
    for tag in REGISTRY
        .blocks
        .tag_keys()
        .filter(|tag| identifier_matches(&remaining, tag))
    {
        builder.suggest(format!("#{tag}"));
    }
}

/// Suggests block ids only. Vanilla's `BlockStateArgument` passes `forTag = false`, so
/// unlike [`suggest_blocks`] no `#tag` completions are offered.
pub(super) fn suggest_block_states(builder: &mut SuggestionsBuilder<'_>) {
    let remaining = builder.remaining_lowercase().to_owned();
    if remaining.contains(['[', '{']) {
        return;
    }
    for block in REGISTRY
        .blocks
        .iter()
        .map(|(_, block)| &block.key)
        .filter(|key| identifier_matches(&remaining, key))
    {
        builder.suggest(block.to_string());
    }
}

fn identifier_matches(pattern: &str, identifier: &Identifier) -> bool {
    if pattern.contains(':') {
        matches_substring(pattern, &identifier.to_string())
    } else {
        matches_substring(pattern, identifier.namespace.as_ref())
            || matches_substring(pattern, identifier.path.as_ref())
    }
}
