//! Villager-related systems: professions, trading, gossip, POI assignment.
//!
//! This module groups the 6 phases of villager implementation:
//! 1. VillagerData handling (type / profession / level)
//! 2. Jobs / workstation POI mapping
//! 3. Trading (MerchantOffer lifecycle, restock, XP)
//! 4. POI claiming
//! 5. Gossip / reputation + wandering trader / golem spawning helpers
//! 6. Serialization helpers

pub mod gossip;
pub mod profession;
pub mod trading;
pub mod village;

pub use gossip::{GossipContainer, GossipType};
pub use profession::{VillagerProfessionKind, workstation_for_profession};
pub use trading::{MerchantOffer, MerchantOffers};
