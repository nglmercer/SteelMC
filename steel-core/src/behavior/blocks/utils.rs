use glam::DVec3;
use steel_registry::blocks::BlockRef;
use steel_registry::items::item::BlockHitResult;
use steel_utils::Direction;

use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks::{
    BARRIER, CARVED_PUMPKIN, JACK_O_LANTERN, MANGROVE_LEAVES, MELON, PUMPKIN,
};

pub fn is_excluded_for_connection(block: BlockRef) -> bool {
    block.has_tag(&BlockTag::LEAVES)
        || block == &BARRIER
        || block == &CARVED_PUMPKIN
        || block == &JACK_O_LANTERN
        || block == &MELON
        || block == &PUMPKIN
        || block.has_tag(&BlockTag::SHULKER_BOXES)
        || block == &MANGROVE_LEAVES
}

/// Converts a rotation in degrees to a 16-segment rotation value (0-15).
///
/// Vanilla `RotationSegment.convertToSegment(float)`. Each segment is 22.5 degrees, and
/// rotation is measured clockwise from south.
pub(super) fn convert_to_rotation_segment(degrees: f32) -> u8 {
    let normalized = degrees.rem_euclid(360.0);
    (((normalized / 22.5) + 0.5) as u8) & 15
}

/// Vanilla `SelectableSlotContainer.getHitSlot`.
///
/// Maps a click on the block's front face onto a slot index in a `rows` x `columns` grid,
/// counting left-to-right then top-to-bottom. Returns `None` when the player clicked any
/// face other than `facing`.
pub(super) fn selectable_slot_hit(
    hit_result: &BlockHitResult,
    facing: Direction,
    rows: usize,
    columns: usize,
) -> Option<usize> {
    if hit_result.direction != facing {
        return None;
    }

    // Vanilla measures the hit against the block in front of the clicked face.
    let front_pos = hit_result.block_pos.relative(hit_result.direction);
    let relative = hit_result.location
        - DVec3::new(
            f64::from(front_pos.x()),
            f64::from(front_pos.y()),
            f64::from(front_pos.z()),
        );

    let horizontal = match hit_result.direction {
        Direction::North => 1.0 - relative.x,
        Direction::South => relative.x,
        Direction::West => relative.z,
        Direction::East => 1.0 - relative.z,
        Direction::Up | Direction::Down => return None,
    };

    let row = selectable_slot_section(1.0 - relative.y, rows);
    let column = selectable_slot_section(horizontal, columns);
    Some(column + row * columns)
}

/// Vanilla `SelectableSlotContainer.getSection`.
fn selectable_slot_section(relative_coordinate: f64, max_sections: usize) -> usize {
    let targeted_pixel = relative_coordinate * 16.0;
    let section_size = 16.0 / max_sections as f64;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the value is clamped into the section range immediately after"
    )]
    let section = (targeted_pixel / section_size).floor() as i64;
    section.clamp(0, max_sections as i64 - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::selectable_slot_section;

    #[test]
    fn sections_split_a_face_into_equal_columns_and_clamp_out_of_range_hits() {
        assert_eq!(selectable_slot_section(0.0, 3), 0);
        assert_eq!(selectable_slot_section(0.32, 3), 0);
        assert_eq!(selectable_slot_section(0.34, 3), 1);
        assert_eq!(selectable_slot_section(0.67, 3), 2);
        assert_eq!(selectable_slot_section(1.5, 3), 2);
        assert_eq!(selectable_slot_section(-0.5, 3), 0);
    }
}
