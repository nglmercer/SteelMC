//! Vanilla `Mirror` — structure mirroring, maps to a `Rotation` for a facing.

use crate::{Direction, axis::Axis, rotation::Rotation};

/// Mirrors vanilla `net.minecraft.world.level.block.Mirror`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mirror {
    /// No mirroring.
    None,
    /// Mirrors left ↔ right (east ↔ west), i.e. Z axis.
    LeftRight,
    /// Mirrors front ↔ back (north ↔ south), i.e. X axis.
    FrontBack,
}

impl Mirror {
    /// Matches vanilla `Mirror.getRotation(Direction)`.
    #[must_use]
    pub const fn get_rotation(self, dir: Direction) -> Rotation {
        match self {
            Self::None => Rotation::None,
            Self::LeftRight => {
                if dir.get_axis() == Axis::Z {
                    Rotation::Clockwise180
                } else {
                    Rotation::None
                }
            }
            Self::FrontBack => {
                if dir.get_axis() == Axis::X {
                    Rotation::Clockwise180
                } else {
                    Rotation::None
                }
            }
        }
    }

    /// Matches vanilla `Mirror.mirror(Direction)`.
    #[must_use]
    pub const fn mirror(self, dir: Direction) -> Direction {
        match self {
            Self::None => dir,
            Self::LeftRight => {
                if dir.get_axis() == Axis::Z {
                    dir.opposite()
                } else {
                    dir
                }
            }
            Self::FrontBack => {
                if dir.get_axis() == Axis::X {
                    dir.opposite()
                } else {
                    dir
                }
            }
        }
    }
}
