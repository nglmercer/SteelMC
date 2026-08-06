//! Packet sent to update the client's statistics screen.

use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::C_AWARD_STATS;

/// One statistic entry.
///
/// Vanilla encodes `Stat<?>` as the stat type's registry id followed by the id of the entry
/// within that type's own registry (block, item, entity type, or custom stat).
#[derive(Debug, Clone, Copy, PartialEq, Eq, WriteTo)]
pub struct StatEntry {
    /// Registry id of the `StatType`.
    #[write(as = VarInt)]
    pub stat_type_id: i32,
    /// Registry id of the entry this statistic counts.
    #[write(as = VarInt)]
    pub value_id: i32,
    /// The statistic's current total.
    #[write(as = VarInt)]
    pub value: i32,
}

/// Sends updated statistic totals to the client.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_AWARD_STATS)]
pub struct CAwardStats {
    /// The statistics that changed.
    #[write(as = Prefixed(VarInt))]
    pub stats: Vec<StatEntry>,
}
