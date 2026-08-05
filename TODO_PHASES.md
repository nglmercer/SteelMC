# SteelMC TODO Implementation Plan — Phased

> **Scope:** 182 `TODO` hits in `steel-core/src` as of `e8b1c37d7` (was 186, 4 campfire/shovel resolved). Audited vs `minecraft-src` (mc26.2) and `AGENTS.md` (`ASK,DON'T GUESS` / `FOUNDATIONAL INTEGRITY` / `VANILLA FUNCTIONALITY`).
> **Rule:** Generated registry/worldgen → `build/` + extractor; no hardcoded registry values; seeded RNG for deterministic gameplay.

## Progress Log
- **Phase 0 — Done:** `cargo run` 37 warnings → 0 (`missing_docs`/`unused_imports`/`dead_code`); campfire `CookingTimes`/`CookingTotalTimes` `IntArray` + `cookTick`/`cooldownTick` + `place_food`/`take_items_for_dowse`/`clear_cooking_state` + `campfire_block use_item_on`; shovel `ITEM_SHOVEL_FLATTEN` + dowse `SOUND_EXTINGUISH_FIRE`/`LIT=false`/`hurt_and_break`/`BLOCK_CHANGE`. Verified `cargo check` 0, `cargo test -p steel-core --lib` 2332 passed, `verify_campfire.rs` 6 passed.

## Phases

### Phase 1 — Block-State Geometry (Low-risk, no extractor)
**Goal:** `mirror`/`rotate` for directional blocks; pure `Direction` transforms, no registry.
**Check:** `cargo check`, `cargo test` directional placement.

| File | Line | TODO | Exact Change |
|------|------|------|--------------|
| `building/amethyst_cluster.rs` | 83 | `Mirror and Rotate` | Add `BlockBehavior::mirror`/`rotate` impl: `FACING: rotation.rotate(state.get(FACING))`, `mirror.getRotation` |
| `building/ladder_block.rs` | 87 | `mirror and rotate` | Same for `HORIZONTAL_FACING` |
| `vegetation/sculk_vein_block.rs` | 15 | `rotation/mirror overrides` | Delegate to `MultifaceBlock` via `Direction` mapping |

**Adds ABI:** `BlockBehavior::rotate(Mirror|Rotation) -> BlockStateId` (requires owner approval).

### Phase 2 — Brewing Stand (Needs extractor)
**Goal:** Full `BrewingStandBlockEntity.serverTick` (`brewTime=400`, `FUEL_USES=20`, `HAS_BOTTLE[3]`).
**Blockers:** `potionBrewing` registry, `ItemTags.BREWING_FUEL`, `Potion` components. `REGISTRY.recipes.find_*` does not cover brewing.
**Extractor:** `steel-registry/build_assets/brewing.json` + `steel-core/build` generation; do not hardcode `BLAZE_POWDER`.
**Files:** `block_entity/entities/brewing_stand.rs:23`, `behavior/blocks/container/brewing_stand_block.rs` (add ticker)
**Verify:** `find_brewing_recipe` with potion inputs, NBT `BrewTime`/`Fuel`.

### Phase 3 — Simple Block Behaviors (No foundations)
**Goal:** Close `TODO: Implement full vanilla behavior beyond can_survive.` where only `can_survive` exists.
**Files (~18):** `lily_pad_block.rs:14`, `mushroom_block.rs:15`, `nether_fungus_block.rs:12`, `sea_pickle_block.rs:15`, `small_dripleaf_block.rs:17`, `mossy_carpet_block.rs:14`, `kelp_block.rs:17`, `kelp_plant_block.rs:19`, `spore_blossom_block.rs:14` etc.
**Change:** Keep `can_survive`; add explicit `// VANILLA: no tick/bonemeal beyond survival` comment per `Particle routing` style, remove TODO if intentionally stubbed. Requires confirming vanilla has no tick via `minecraft-src`.

### Phase 4 — Ticking / Growth (Needs scheduler/worldgen)
**Goal:** Random-tick / `scheduledTick` / `bonemeal` growth.
**Needs:** `ScheduledTickAccess`, seeded RNG, `TreeGrower`.
**Files:** `sapling_block.rs:12` (randomTick+tree), `chorus_flower_block.rs:23`, `chorus_plant_block.rs:22`, `mangrove_propagule_block.rs:19`, `big_dripleaf_stem_block.rs:27`, `sculk_vein` spread, `glow_lichen_block.rs:18`, `snow_layer_block.rs:21` (melting/layering), etc.
**Phase after** `steel-worldgen` chunk tick plumbing is stable.

### Phase 5 — Container & Redstone (Needs `scheduleTick` + stats)
**Goal:** `ContainerOpenersCounter`, `scheduleTick` recheck, piglin anger, stats.
**Files:** `barrel_block.rs:80-82` (`OPEN_BARREL`, `angerNearbyPiglins`, `ContainerOpenersCounter`), `crafting_table_block.rs:55` (`INTERACT_WITH_CRAFTING_TABLE`), `scaffolding_block.rs:34,63` (stability/waterlogging)
**Blocker:** No `scheduleTick` area-copy (`clone.rs:214`); idle-dimension flag (`chunk_map:1338`).

### Phase 6 — Items & Projectiles (Needs entity/projectile foundations)
**Goal:** `PotionItem.useOn` (water→mud), `ThrowablePotion` thrown entity, `Compass` lodestone, `Brush` dust particles, `Bucket` per-pos `water_evaporates`.
**Files:** `items/potion.rs:12-17`, `throwable_potion.rs:12-16`, `tipped_arrow.rs:12`, `compass.rs:14`, `brush_item.rs:84`, `bucket.rs:218`, `bonemeal.rs:76` (coral/underwater), `shovel.rs` remaining sound (done), etc.
**Verify:** `UseOnContext` + `ProjectileItem` dispatch.

### Phase 7 — World / Commands / Portal (Needs runtime registries)
**Goal:** `locate biome/poi` async search, `execute store bossbar/entity`/`predicate`/`function`, `clone` tick copy, `summon` SNBT, `fire` flammability, `nether_portal` placement.
**Files:** `command/builtins/locate.rs:45`, `execute/store.rs:33`, `execute/condition.rs:33`, `clone.rs:214`, `summon.rs:40`, `portal/fire.rs:80,192`, `portal/nether_portal_block.rs:64`, `chunk_saver/*`, `entity/*` (end crystal, ender pearl 5% endermite)

### Phase 8 — Polish (Sounds/Advancements/Particles)
**Goal:** Stats + advancements + client ambient ticks.
**Files:** `honey_block.rs:201` (slide advancement), `sign_block.rs:194,242,252` (dye, waxed fail sound, `may_build`), `potent_sulfur_block.rs:144` (`animateTick`), `entity/animal.rs:343` etc.
**Defer until** stats/advancement foundations exist (allowed `// TODO` per code standard).

## How to Execute a Phase
1. Pick one phase table row, read `minecraft-src/minecraft/src/net/minecraft/world/level/block/<Name>.java` + entity.
2. Update `steel-registry/build_assets` + `build.rs` if registry; `cargo check` before `cargo run`.
3. Implement behind documented `BlockBehavior`/`ItemBehavior` API; keep `BlockStateId` via `BlockRef`+properties.
4. `cargo check -p steel-core --all-features` → `cargo test -p steel-core --lib` (narrow) → `cargo clippy -r --all-targets --all-features` if touched.
5. Mark todo checkbox `- [x]` in this file and remove `// TODO` line.

## Full Raw List (182)
```
steel-core/src/behavior/blocks/building/amethyst_cluster.rs:83    // TODO: Mirror and Rotate functions
steel-core/src/behavior/blocks/building/bed_block.rs:20            /// TODO: Add two-block placement, bed block entities, sleep interaction
steel-core/src/behavior/blocks/building/honey_block.rs:201          // TODO: Award the honey-block slide advancement
steel-core/src/behavior/blocks/building/ladder_block.rs:87          // TODO: Implement the mirror and rotate functions
steel-core/src/behavior/blocks/building/potent_sulfur_block.rs:144  // TODO: Implement vanilla animateTick
steel-core/src/behavior/blocks/building/scaffolding_block.rs:34,63  /// TODO: Add vanilla placement, stability, falling, waterlogging
steel-core/src/behavior/blocks/building/weathering_block.rs:46      // TODO: Add weathering for lanterns/chests/golem statues
steel-core/src/behavior/blocks/container/barrel_block.rs:80-82       // TODO: Award stat, anger piglins, ContainerOpenersCounter
steel-core/src/behavior/blocks/container/beehive_block.rs:18        // TODO: Implement full beehive interactions
steel-core/src/behavior/blocks/container/crafting_table_block.rs:55 // TODO: Award stat INTERACT_WITH_CRAFTING_TABLE
steel-core/src/behavior/blocks/decoration/cake_block.rs:25, candle_cake_block.rs:26,83, sign_block.rs:194,242,252, portal/fire.rs:80,192, etc. (see grep output for full 182)
... (full `grep -rn TODO steel-core/src --include="*.rs"` appended as workflow artifact on demand)
```
*Run `grep -rn "TODO" steel-core/src --include="*.rs" > /tmp/todo_full.txt` to regenerate.*

## Decision Needed Before Phase 1
- Approve `BlockBehavior::mirror`/`rotate` ABI addition? (Steel-owned, `steel:` key, no `unsafe` beyond `DowncastType`).
- Confirm Phase 2 brewing should wait for SteelExtractor output vs stub with `#[expect(dead_code)]`.

---
*Generated for `feat/command-parity-foundations` — keep file untracked or commit as `docs/TODO_PHASES.md` per local convention.*
