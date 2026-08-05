//! Block behavior implementations for vanilla blocks.
//!
//! The actual behavior registration is auto-generated from classes.json.
//! See `src/generated/behaviors.rs` for the generated registration code.

mod building;
mod colored;
mod container;
mod decoration;
mod fluid;
mod portal;
mod redstone;
mod terrain;
mod utils;
pub mod vegetation;

pub use building::{
    AmethystBlock, AmethystClusterBlock, BarrierBlock, BedBlock, BuddingAmethystBlock,
    CampfireBlock, CauldronBlock, ComposterBlock, ConduitBlock, DoorBlock, FenceBlock,
    FenceGateBlock, FrostedIceBlock, GlazedTerracottaBlock, HayBlock, HeavyCoreBlock, HoneyBlock,
    IceBlock, IronBarsBlock, LadderBlock, LavaCauldronBlock, LayeredCauldronBlock, MagmaBlock,
    PotentSulfurBlock, PowderSnowBlock, RotatedPillarBlock, ScaffoldingBlock, SlabBlock,
    SlimeBlock, SpongeBlock, StairBlock, TrapDoorBlock, WallBlock, WaterloggedTransparentBlock,
    WeatherState, WeatheringCopper, WeatheringCopperBarsBlock, WeatheringCopperDoorBlock,
    WeatheringCopperFullBlock, WeatheringCopperGrateBlock, WeatheringCopperSlabBlock,
    WeatheringCopperStairBlock, WeatheringCopperTrapDoorBlock, WebBlock, WetSpongeBlock,
};
pub use colored::StainedGlassPaneBlock;
pub use container::{
    AnvilBlock, BarrelBlock, BeehiveBlock, ChestBlock, ChiseledBookShelfBlock, CopperChestBlock,
    CrafterBlock, CraftingTableBlock, DispenserBlock, DropperBlock, EnderChestBlock, HopperBlock,
    JukeboxBlock, LecternBlock, ShelfBlock, ShulkerBoxBlock, TrappedChestBlock,
    WeatheringCopperChestBlock,
};
pub use decoration::{
    BannerBlock, BellBlock, CakeBlock, CandleBlock, CandleCakeBlock, CeilingHangingSignBlock,
    ChainBlock, CopperGolemStatueBlock, DecoratedPotBlock, EndRodBlock, LanternBlock,
    PiglinWallSkullBlock, SkullBlock, StandingSignBlock, TorchBlock, WallBannerBlock,
    WallHangingSignBlock, WallSignBlock, WallSkullBlock, WallTorchBlock,
    WeatheringCopperChainBlock, WeatheringCopperGolemStatueBlock, WeatheringLanternBlock,
};
pub use fluid::{BubbleColumnBlock, LiquidBlock};
pub use portal::{
    EndGatewayBlock, EndPortalBlock, EndPortalFrameBlock, FireBlock, NetherPortalBlock,
    RespawnAnchorBlock, SoulFireBlock,
};
pub use redstone::{
    ButtonBlock, ComparatorBlock, CopperBulbBlock, DaylightDetectorBlock, DetectorRailBlock,
    LeverBlock, MovingPistonBlock, NoteBlock, ObserverBlock, PistonBaseBlock, PistonHeadBlock,
    PoweredBlock, PoweredRailBlock, PressurePlateBlock, PressurePlateSensitivity, RailBlock,
    RedStoneOreBlock, RedStoneWireBlock, RedstoneLampBlock, RedstoneTorchBlock,
    RedstoneWallTorchBlock, RepeaterBlock, TargetBlock, TripWireBlock, TripWireHookBlock,
    WeatheringCopperBulbBlock, WeightedPressurePlateBlock,
};
pub use terrain::falling_block::is_free as falling_block_is_free;
pub use terrain::{
    ColoredFallingBlock, ConcretePowderBlock, DirtPathBlock, DragonEggBlock, DropExperienceBlock,
    GrassBlock, MudBlock, MyceliumBlock, NyliumBlock, SandBlock, SnowyBlock, SoulSandBlock,
};
pub use vegetation::{
    AzaleaBlock, BambooSaplingBlock, BambooStalkBlock, BeetrootBlock, CactusBlock,
    CactusFlowerBlock, CarrotBlock, CocoaBlock, CoralBlock, CropBlock, DoublePlantBlock,
    FlowerBlock, MangroveLeavesBlock, NetherSproutsBlock, NetherWartBlock, PitcherCropBlock,
    PotatoBlock, RootedDirtBlock, SeagrassBlock, SugarCaneBlock, SweetBerryBushBlock,
    TallFlowerBlock, TallGrassBlock, TallSeagrassBlock, TintedParticleLeavesBlock,
    TorchflowerCropBlock, UntintedParticleLeavesBlock,
};
pub use vegetation::{
    BaseCoralFanBlock, BaseCoralPlantBlock, BaseCoralWallFanBlock, BigDripleafBlock,
    BigDripleafStemBlock, BushBlock, CarpetBlock, CaveVinesBlock, CaveVinesPlantBlock,
    ChorusFlowerBlock, ChorusPlantBlock, CoralFanBlock, CoralPlantBlock, CoralWallFanBlock,
    DryVegetationBlock, EyeblossomBlock, EyeblossomType, FarmlandBlock, FireflyBushBlock,
    FlowerBedBlock, GlowLichenBlock, HangingMossBlock, HangingRootsBlock, HugeMushroomBlock,
    KelpBlock, KelpPlantBlock, LeafLitterBlock, LilyPadBlock, MangrovePropaguleBlock,
    MangroveRootsBlock, MossyCarpetBlock, MultifaceBlock, MushroomBlock, NetherFungusBlock,
    NetherRootsBlock, PointedDripstoneBlock, SaplingBlock, SculkVeinBlock, SeaPickleBlock,
    ShortDryGrassBlock, SmallDripleafBlock, SnowLayerBlock, SporeBlossomBlock, SulfurSpikeBlock,
    TallDryGrassBlock, TwistingVinesBlock, TwistingVinesPlantBlock, VineBlock, WeepingVinesBlock,
    WeepingVinesPlantBlock, WitherRoseBlock, WoolCarpetBlock,
};
