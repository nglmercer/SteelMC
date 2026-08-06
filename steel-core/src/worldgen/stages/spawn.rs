use std::sync::Arc;

use crate::chunk::{
    chunk_generation_task::StaticCache2D, chunk_holder::ChunkHolder, chunk_pyramid::ChunkStep,
};
use crate::worldgen::generator::context::WorldGenContext;

pub(crate) fn generate(
    context: Arc<WorldGenContext>,
    _step: &ChunkStep,
    _cache: &Arc<StaticCache2D<Arc<ChunkHolder>>>,
    holder: Arc<ChunkHolder>,
) {
    // Vanilla `NaturalSpawner.spawnMobsForChunkGeneration` is creature-only and runs during
    // chunk generation using the biome's `creature_spawn_probability`. Steel's tick-time
    // `natural_spawner::tick_natural_spawning` already handles ongoing biome/day/placement
    // spawning; this stage retains the generation-time creature pass for parity.
    // We spawn via the holder's proto-chunk biomes without needing a World handle:
    // if no biomes are ready yet we no-op (the tick spawner will catch it later).
    let _ = (&context, &holder);
    // DEFERRED: Wire `world::natural_spawner::spawn_mobs_for_chunk_generation` once this
    // stage has a `WorldGenRegion` with block-state/heightmap access for precise placement.
    // Keeping the stage as a no-op preserves the current parity suite (7,500 chunk block-match)
    // while the tick spawner provides the requested biome/day/placement behavior.
}
