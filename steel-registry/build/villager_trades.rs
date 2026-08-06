//! Build script for vanilla villager trades and trade sets.
//!
//! Mirrors `loot_tables`: walks the extracted datapack, compiles each JSON file into a typed
//! Rust constant, and emits a `register_*` function. Unknown fields and unhandled shapes
//! `panic!` so an extractor change fails the build instead of silently dropping trades.

use std::{fs, path::Path};

use heck::ToShoutySnakeCase;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use serde::Deserialize;

use crate::loot_tables::conditions::generate_condition;
use crate::loot_tables::functions::generate_function;
use crate::loot_tables::values::generate_number_provider;
use crate::loot_tables::{LootConditionJson, LootFunctionJson, NumberProviderJson};

const TRADE_DIR: &str = "../steel-utils/build_assets/builtin_datapacks/minecraft/villager_trade";
const TRADE_SET_DIR: &str = "../steel-utils/build_assets/builtin_datapacks/minecraft/trade_set";

/// Vanilla `TradeCost`: `{id, count?, components?}`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TradeCostJson {
    id: String,
    #[serde(default)]
    count: Option<NumberProviderJson>,
    /// Only one vanilla trade carries this, and it is an exact-component predicate on the
    /// *input* item. Steel does not model input component predicates yet, so a trade using
    /// it would silently accept the wrong item — fail the build instead.
    #[serde(default)]
    components: Option<serde_json::Value>,
}

/// Vanilla `ItemStackTemplate`: `{id, count?}`.
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct GivesJson {
    id: String,
    #[serde(default)]
    count: Option<f32>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct VillagerTradeJson {
    wants: TradeCostJson,
    #[serde(default)]
    additional_wants: Option<TradeCostJson>,
    gives: GivesJson,
    #[serde(default)]
    max_uses: Option<NumberProviderJson>,
    #[serde(default)]
    xp: Option<NumberProviderJson>,
    #[serde(default)]
    reputation_discount: Option<NumberProviderJson>,
    #[serde(default)]
    merchant_predicate: Option<LootConditionJson>,
    #[serde(default)]
    given_item_modifiers: Vec<LootFunctionJson>,
    #[serde(default)]
    double_trade_price_enchantments: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TradeSetJson {
    trades: String,
    amount: NumberProviderJson,
    #[serde(default)]
    allow_duplicates: bool,
    #[serde(default)]
    random_sequence: Option<String>,
}

fn strip_vanilla(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

/// `farmer/1/wheat_emerald` -> `FARMER_1_WHEAT_EMERALD`.
fn const_ident(key: &str) -> Ident {
    Ident::new(
        &key.replace(['/', '.'], "_").to_shouty_snake_case(),
        Span::call_site(),
    )
}

fn generate_trade_cost(cost: &TradeCostJson, key: &str) -> TokenStream {
    assert!(
        cost.components.is_none(),
        "villager trade `{key}` uses `wants.components`, which Steel's TradeCost does not \
         model. Implement an exact-component predicate on the input before regenerating."
    );
    let item = strip_vanilla(&cost.id);
    let count = cost.count.as_ref().map_or_else(
        || quote! { NumberProvider::Constant(1.0) },
        generate_number_provider,
    );
    quote! {
        TradeCost {
            item: &vanilla_items::#{const_ident(item)},
            count: #count,
        }
    }
}

/// Collects every `<profession>/<level>/<name>.json` under `dir`.
fn read_dir_recursive(dir: &Path, prefix: &str, out: &mut Vec<(String, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        let path = entry.path();
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_owned();
        let key = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };

        if path.is_dir() {
            read_dir_recursive(&path, &key, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push((key, fs::read_to_string(&path).expect("read trade json")));
        }
    }
}

pub(crate) fn build() -> TokenStream {
    let mut trades = Vec::new();
    read_dir_recursive(Path::new(TRADE_DIR), "", &mut trades);
    let mut trade_sets = Vec::new();
    read_dir_recursive(Path::new(TRADE_SET_DIR), "", &mut trade_sets);

    assert!(
        !trades.is_empty() && !trade_sets.is_empty(),
        "no villager trade data found; expected extracted datapacks under {TRADE_DIR}"
    );

    let mut stream = quote! {
        use steel_utils::Identifier;

        use crate::item_stack_template::ItemStackTemplate;
        use crate::loot_table::{ConditionalLootFunction, LootCondition, LootFunction, NumberProvider};
        use crate::villager_trade::{TradeCost, TradeSet, TradeSetRegistry, VillagerTrade, VillagerTradeRegistry};
        use crate::vanilla_items;
    };

    let mut trade_idents = Vec::new();
    for (key, source) in &trades {
        let parsed: VillagerTradeJson = serde_json::from_str(source)
            .unwrap_or_else(|error| panic!("villager trade `{key}`: {error}"));

        let ident = const_ident(key);
        let wants = generate_trade_cost(&parsed.wants, key);
        let additional_wants = parsed.additional_wants.as_ref().map_or_else(
            || quote! { None },
            |cost| {
                let cost = generate_trade_cost(cost, key);
                quote! { Some(#cost) }
            },
        );

        let gives_item = const_ident(strip_vanilla(&parsed.gives.id));
        let gives_count = parsed.gives.count.unwrap_or(1.0) as i32;
        let gives = quote! {
            ItemStackTemplate::with_count(&vanilla_items::#gives_item, #gives_count)
        };

        let max_uses = parsed.max_uses.as_ref().map_or_else(
            || quote! { NumberProvider::Constant(4.0) },
            generate_number_provider,
        );
        let xp = parsed.xp.as_ref().map_or_else(
            || quote! { NumberProvider::Constant(1.0) },
            generate_number_provider,
        );
        let reputation_discount = parsed.reputation_discount.as_ref().map_or_else(
            || quote! { NumberProvider::Constant(0.0) },
            generate_number_provider,
        );
        let merchant_predicate = parsed.merchant_predicate.as_ref().map_or_else(
            || quote! { None },
            |condition| {
                let condition = generate_condition(condition);
                quote! { Some(#condition) }
            },
        );
        let modifiers: Vec<TokenStream> = parsed
            .given_item_modifiers
            .iter()
            .map(generate_function)
            .collect();
        let double_price = parsed.double_trade_price_enchantments.as_ref().map_or_else(
            || quote! { None },
            |tag| {
                let tag = strip_vanilla(tag.trim_start_matches('#'));
                quote! { Some(Identifier::vanilla_static(#tag)) }
            },
        );

        stream.extend(quote! {
            pub static #ident: VillagerTrade = VillagerTrade {
                key: Identifier::vanilla_static(#key),
                wants: #wants,
                additional_wants: #additional_wants,
                gives: #gives,
                max_uses: #max_uses,
                xp: #xp,
                reputation_discount: #reputation_discount,
                merchant_predicate: #merchant_predicate,
                given_item_modifiers: &[#(#modifiers),*],
                double_trade_price_enchantments: #double_price,
            };
        });
        trade_idents.push(ident);
    }

    let mut set_idents = Vec::new();
    for (key, source) in &trade_sets {
        let parsed: TradeSetJson = serde_json::from_str(source)
            .unwrap_or_else(|error| panic!("trade set `{key}`: {error}"));

        let ident = Ident::new(
            &format!("SET_{}", key.replace('/', "_").to_shouty_snake_case()),
            Span::call_site(),
        );
        let tag = strip_vanilla(parsed.trades.trim_start_matches('#'));
        let amount = generate_number_provider(&parsed.amount);
        let allow_duplicates = parsed.allow_duplicates;
        let random_sequence = parsed.random_sequence.as_ref().map_or_else(
            || quote! { None },
            |sequence| {
                let sequence = strip_vanilla(sequence);
                quote! { Some(Identifier::vanilla_static(#sequence)) }
            },
        );

        stream.extend(quote! {
            pub static #ident: TradeSet = TradeSet {
                key: Identifier::vanilla_static(#key),
                trades: Identifier::vanilla_static(#tag),
                amount: #amount,
                allow_duplicates: #allow_duplicates,
                random_sequence: #random_sequence,
            };
        });
        set_idents.push(ident);
    }

    stream.extend(quote! {
        pub fn register_villager_trades(registry: &mut VillagerTradeRegistry) {
            #(registry.register(&#trade_idents);)*
        }

        pub fn register_trade_sets(registry: &mut TradeSetRegistry) {
            #(registry.register(&#set_idents);)*
        }
    });

    stream
}
