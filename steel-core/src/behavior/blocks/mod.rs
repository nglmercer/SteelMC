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
    AmethystBlock, AmethystClusterBlock, BarrierBlock, BeaconBlock, BedBlock, BuddingAmethystBlock,
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
    AnvilBlock, BarrelBlock, BeehiveBlock, BlastFurnaceBlock, BrewingStandBlock, CartographyTableBlock,
    ChestBlock, ChiseledBookShelfBlock, CopperChestBlock, CrafterBlock, CraftingTableBlock,
    DispenserBlock, DropperBlock, EnchantingTableBlock, EnderChestBlock, FurnaceBlock, GrindstoneBlock,
    HopperBlock, JukeboxBlock, LecternBlock, LoomBlock, ShelfBlock, ShulkerBoxBlock, SmithingTableBlock,
    SmokerBlock, StonecutterBlock, TrappedChestBlock, VaultBlock, WeatheringCopperChestBlock,
};
pub use container::{is_valid_enchanting_bookshelf, valid_enchanting_bookshelf_count};
pub use decoration::{
    BannerBlock, BellBlock, CakeBlock, CandleBlock, CandleCakeBlock, CeilingHangingSignBlock,
    ChainBlock, CopperGolemStatueBlock, DecoratedPotBlock, EndRodBlock, LanternBlock,
    PiglinWallSkullBlock, PlayerHeadBlock, PlayerWallHeadBlock, SkullBlock, StandingSignBlock,
    TorchBlock, WallBannerBlock, WallHangingSignBlock, WallSignBlock, WallSkullBlock,
    WallTorchBlock, WeatheringCopperChainBlock, WeatheringCopperGolemStatueBlock,
    WeatheringLanternBlock,
};
pub use fluid::{BubbleColumnBlock, LiquidBlock};
pub use portal::{
    EndGatewayBlock, EndPortalBlock, EndPortalFrameBlock, FireBlock, NetherPortalBlock,
    RespawnAnchorBlock, SoulFireBlock,
};
pub use redstone::{
    ButtonBlock, CalibratedSculkSensorBlock, ComparatorBlock, CopperBulbBlock,
    DaylightDetectorBlock, DetectorRailBlock, LeverBlock, LightningRod, LightningRodBlock,
    MovingPistonBlock, NoteBlock, ObserverBlock, PistonBaseBlock, PistonHeadBlock, PoweredBlock,
    PoweredRailBlock, PressurePlateBlock, PressurePlateSensitivity, RailBlock, RedStoneOreBlock,
    RedStoneWireBlock, RedstoneLampBlock, RedstoneTorchBlock, RedstoneWallTorchBlock,
    RepeaterBlock, SculkSensorBlock, TargetBlock, TripWireBlock, TripWireHookBlock,
    WeatheringCopperBulbBlock, WeatheringLightningRodBlock, WeightedPressurePlateBlock,
};
pub use terrain::falling_block::is_free as falling_block_is_free;
pub use terrain::{
    BrushableBlock, ColoredFallingBlock, ConcretePowderBlock, DirtPathBlock, DragonEggBlock,
    DropExperienceBlock, GrassBlock, MudBlock, MyceliumBlock, NyliumBlock, SandBlock, SnowyBlock,
    SoulSandBlock,
};
pub use vegetation::{
    AzaleaBlock, BambooSaplingBlock, BambooStalkBlock, BeetrootBlock, CactusBlock,
    CactusFlowerBlock, CarrotBlock, CocoaBlock, CoralBlock, CropBlock, DoublePlantBlock,
    FlowerBlock, MangroveLeavesBlock, NetherSproutsBlock, NetherWartBlock, PitcherCropBlock,
    PotatoBlock, PumpkinBlock, RootedDirtBlock, SeagrassBlock, SugarCaneBlock, SweetBerryBushBlock,
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
