//! Furnace fuel values — mirrors vanilla `FuelValues.vanillaBurnTimes`.
//!
//! Values are derived from `minecraft-src/.../block/entity/FuelValues.java`
//! with `baseUnit = 200`. Tag membership is resolved via `Item::has_tag` at
//! call time, so no build-time generation is needed and modded tags remain
//! compatible.

use crate::items::ItemRef;
use crate::vanilla_item_tags::ItemTag;

/// Returns the furnace burn duration for `item`, or 0 if not fuel.
///
/// Mirrors `FuelValues.burnDuration`.
#[must_use]
pub fn burn_duration(item: ItemRef) -> i32 {
    // Early returns for highest-value single items.
    let key = item.key.path.as_ref();
    match key {
        "lava_bucket" => return 20_000,
        "coal_block" => return 16_000,
        "blaze_rod" => return 2_400,
        "coal" => return 1_600,
        "charcoal" => return 1_600,
        _ => {}
    }

    // Evaluate tag-derived fuels in builder order, keeping last match
    // (mirrors Builder.put overwrites). NON_FLAMMABLE_WOOD clears result.
    let mut burn: Option<i32> = None;

    if item.has_tag(&ItemTag::LOGS) { burn = Some(300); }
    if item.has_tag(&ItemTag::BAMBOO_BLOCKS) { burn = Some(300); }
    if item.has_tag(&ItemTag::PLANKS) { burn = Some(300); }
    if key == "bamboo_mosaic" { burn = Some(300); }
    if item.has_tag(&ItemTag::WOODEN_STAIRS) { burn = Some(300); }
    if key == "bamboo_mosaic_stairs" { burn = Some(300); }
    if item.has_tag(&ItemTag::WOODEN_SLABS) { burn = Some(150); }
    if key == "bamboo_mosaic_slab" { burn = Some(150); }
    if item.has_tag(&ItemTag::WOODEN_TRAPDOORS) { burn = Some(300); }
    if item.has_tag(&ItemTag::WOODEN_PRESSURE_PLATES) { burn = Some(300); }
    if item.has_tag(&ItemTag::WOODEN_SHELVES) { burn = Some(300); }
    if item.has_tag(&ItemTag::WOODEN_FENCES) { burn = Some(300); }
    if item.has_tag(&ItemTag::FENCE_GATES) { burn = Some(300); }
    if key == "note_block" { burn = Some(300); }
    if key == "bookshelf" { burn = Some(300); }
    if key == "chiseled_bookshelf" { burn = Some(300); }
    if key == "lectern" { burn = Some(300); }
    if key == "jukebox" { burn = Some(300); }
    if key == "chest" { burn = Some(300); }
    if key == "trapped_chest" { burn = Some(300); }
    if key == "crafting_table" { burn = Some(300); }
    if key == "daylight_detector" { burn = Some(300); }
    if item.has_tag(&ItemTag::BANNERS) { burn = Some(300); }
    if key == "bow" { burn = Some(300); }
    if key == "fishing_rod" { burn = Some(300); }
    if key == "ladder" { burn = Some(300); }
    if item.has_tag(&ItemTag::SIGNS) { burn = Some(200); }
    if item.has_tag(&ItemTag::HANGING_SIGNS) { burn = Some(800); }
    if key == "wooden_shovel" { burn = Some(200); }
    if key == "wooden_sword" { burn = Some(200); }
    if key == "wooden_spear" { burn = Some(200); }
    if key == "wooden_hoe" { burn = Some(200); }
    if key == "wooden_axe" { burn = Some(200); }
    if key == "wooden_pickaxe" { burn = Some(200); }
    if item.has_tag(&ItemTag::WOODEN_DOORS) { burn = Some(200); }
    if item.has_tag(&ItemTag::BOATS) { burn = Some(1_200); }
    if item.has_tag(&ItemTag::WOOL) { burn = Some(100); }
    if item.has_tag(&ItemTag::WOODEN_BUTTONS) { burn = Some(100); }
    if key == "stick" { burn = Some(100); }
    if item.has_tag(&ItemTag::SAPLINGS) { burn = Some(100); }
    if key == "bowl" { burn = Some(100); }
    if item.has_tag(&ItemTag::WOOL_CARPETS) { burn = Some(67); }
    if key == "dried_kelp_block" { burn = Some(4_001); }
    if key == "crossbow" { burn = Some(300); }
    if key == "bamboo" { burn = Some(50); }
    if key == "dead_bush" { burn = Some(100); }
    if key == "short_dry_grass" { burn = Some(100); }
    if key == "tall_dry_grass" { burn = Some(100); }
    if key == "scaffolding" { burn = Some(50); }
    if key == "loom" { burn = Some(300); }
    if key == "barrel" { burn = Some(300); }
    if key == "cartography_table" { burn = Some(300); }
    if key == "fletching_table" { burn = Some(300); }
    if key == "smithing_table" { burn = Some(300); }
    if key == "composter" { burn = Some(300); }
    if key == "azalea" { burn = Some(100); }
    if key == "flowering_azalea" { burn = Some(100); }
    if key == "mangrove_roots" { burn = Some(300); }
    if key == "leaf_litter" { burn = Some(100); }

    if item.has_tag(&ItemTag::NON_FLAMMABLE_WOOD) {
        return 0;
    }

    burn.unwrap_or(0)
}

/// Returns whether `item` is furnace fuel.
#[must_use]
pub fn is_fuel(item: ItemRef) -> bool {
    burn_duration(item) > 0
}
