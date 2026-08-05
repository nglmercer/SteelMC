//! Loom menu.

use std::mem;
use std::sync::Arc;

use steel_registry::{
    REGISTRY, TaggedRegistryExt as _,
    banner_pattern::BannerPattern,
    blocks::block_state_ext::BlockStateExt as _,
    data_components::components::{BannerPatternLayer, BannerPatternLayers},
    data_components::vanilla_components::{BANNER_PATTERNS, DYE, PROVIDES_BANNER_PATTERNS},
    item_stack::ItemStack,
    registry::RegistryHolder,
    registry::RegistryHolderSet,
    vanilla_banner_pattern_tags::BannerPatternTag,
    vanilla_blocks,
    vanilla_item_tags::ItemTag,
    vanilla_menu_types,
};
use steel_utils::{
    BlockPos,
    locks::{IntoShared as _, Shared, SyncMutex},
};

use crate::{
    inventory::{
        container::{ResultContainer, SimpleContainer},
        prelude::*,
        slots::LoomResultHandler,
    },
    player::player_inventory::PlayerInventory,
    world::World,
};

/// Vanilla `LoomMenu.PATTERN_NOT_SET`.
const PATTERN_NOT_SET: i16 = -1;
/// Vanilla rejects a seventh layer on a banner.
const MAX_PATTERN_LAYERS: usize = 6;
/// Container-local slot indices of the loom's three inputs.
const BANNER_SLOT: usize = 0;
const DYE_SLOT: usize = 1;
const PATTERN_SLOT: usize = 2;

/// Builds the loom menu.
#[must_use]
pub fn loom(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    pos: BlockPos,
    world: &Arc<World>,
) -> Menu {
    let input_container = SimpleContainer::new(3).into_shared();
    let result_container = ResultContainer::new().into_shared();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::LOOM, container_id);

    // Each input only takes the item kind vanilla allows in that slot.
    let input = builder.section_all_with(
        input_container.clone(),
        SectionKind::restricted(|index, stack: &ItemStack| match index {
            BANNER_SLOT => REGISTRY.items.is_in_tag(stack.item(), &ItemTag::BANNERS),
            DYE_SLOT => is_dye_item(stack),
            PATTERN_SLOT => is_pattern_item(stack),
            _ => false,
        }),
    );
    let result = builder.result_slot(LoomResultHandler::new(
        input_container.clone(),
        result_container.clone(),
        pos,
        world.clone(),
    ));

    let player = builder.player_inventory(&inventory);
    let selected_pattern = builder.data_slot(PATTERN_NOT_SET);

    builder.route_with_remainder_policy(
        result,
        player.all(),
        FillDirection::Backward,
        FakeResultRemainderPolicy::Discard,
    );
    builder.route(input, player.all(), FillDirection::Forward);
    builder.route(player.hotbar(), input, FillDirection::Forward);
    builder.route(player.main(), input, FillDirection::Forward);
    builder.drain(input);

    builder.build(LoomKind {
        input_container,
        result_container,
        block_pos: pos,
        world: Arc::clone(world),
        selected_pattern,
        selectable_patterns: SyncMutex::new(Vec::new()),
    })
}

/// Vanilla `LoomMenu.isDyeItem`.
fn is_dye_item(stack: &ItemStack) -> bool {
    REGISTRY.items.is_in_tag(stack.item(), &ItemTag::LOOM_DYES) && stack.has(DYE)
}

/// Vanilla `LoomMenu.isPatternItem`.
fn is_pattern_item(stack: &ItemStack) -> bool {
    REGISTRY
        .items
        .is_in_tag(stack.item(), &ItemTag::LOOM_PATTERNS)
        && stack.has(PROVIDES_BANNER_PATTERNS)
}

/// Per-menu loom state: the three inputs, the woven result, and the pattern choice.
pub struct LoomKind {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    block_pos: BlockPos,
    world: Arc<World>,
    /// Client-facing index into [`Self::selectable_patterns`], or `-1` for none.
    selected_pattern: DataSlot,
    /// Patterns the current pattern slot offers, in the order the client sees them.
    selectable_patterns: SyncMutex<Vec<&'static BannerPattern>>,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind
// within the process.
unsafe impl steel_utils::DowncastType for LoomKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/loom");
}

impl LoomKind {
    /// Vanilla `LoomMenu.getSelectablePatterns`.
    ///
    /// An empty pattern slot offers everything a loom can weave unaided; a banner pattern
    /// item offers exactly what it provides.
    fn selectable_patterns(pattern_stack: &ItemStack) -> Vec<&'static BannerPattern> {
        if pattern_stack.is_empty() {
            return REGISTRY
                .banner_patterns
                .get_tag(&BannerPatternTag::NO_ITEM_REQUIRED)
                .unwrap_or_default();
        }

        match pattern_stack.get(PROVIDES_BANNER_PATTERNS) {
            Some(RegistryHolderSet::Direct(patterns)) => patterns.clone(),
            Some(RegistryHolderSet::Tag(tag)) => {
                REGISTRY.banner_patterns.get_tag(tag).unwrap_or_default()
            }
            None => Vec::new(),
        }
    }

    /// Vanilla `LoomMenu.setupResultSlot`: the banner plus one more layer.
    fn build_result(
        banner: &ItemStack,
        dye: &ItemStack,
        pattern: &'static BannerPattern,
    ) -> ItemStack {
        if banner.is_empty() || dye.is_empty() {
            return ItemStack::empty();
        }
        let Some(color) = dye.get(DYE).copied() else {
            return ItemStack::empty();
        };

        let mut result = banner.copy_with_count(1);
        let mut layers = result
            .get(BANNER_PATTERNS)
            .cloned()
            .unwrap_or_else(BannerPatternLayers::empty)
            .layers()
            .to_vec();
        layers.push(BannerPatternLayer::new(
            RegistryHolder::reference(pattern),
            color,
        ));
        result.set(BANNER_PATTERNS, BannerPatternLayers::new(layers));
        result
    }

    /// Vanilla `LoomMenu.slotsChanged`.
    ///
    /// # Panics
    /// Panics if the input container is not exactly three slots.
    fn recompute(&mut self, behavior: &mut MenuBehavior, guard: &mut ContainerLockGuard) {
        let Some([input_container, result_container]) = guard.get_disjoint_mut([
            ContainerId::from_arc(&self.input_container),
            ContainerId::from_arc(&self.result_container),
        ]) else {
            panic!("failed to lock input and/or result containers to create loom result")
        };

        let [banner, dye, pattern_stack] = input_container.items() else {
            panic!("input_container in loom menu does not fit expected shape")
        };

        if banner.is_empty() || dye.is_empty() {
            result_container.set_item(0, ItemStack::empty());
            *self.selectable_patterns.lock() = Vec::new();
            self.selected_pattern.set(behavior, PATTERN_NOT_SET);
            return;
        }

        let previous_patterns = mem::replace(
            &mut *self.selectable_patterns.lock(),
            Self::selectable_patterns(pattern_stack),
        );
        let patterns = self.selectable_patterns.lock().clone();

        // Vanilla keeps the chosen pattern across a pattern-slot change when it is still
        // on offer, auto-selects a lone option, and otherwise clears the choice.
        let previous_index = usize::try_from(self.selected_pattern.get(behavior)).ok();
        let display = if patterns.len() == 1 {
            self.selected_pattern.set(behavior, 0);
            patterns.first().copied()
        } else {
            let kept = previous_index
                .and_then(|index| previous_patterns.get(index).copied())
                .and_then(|previous| {
                    let new_index = patterns.iter().position(|pattern| *pattern == previous)?;
                    Some((previous, new_index))
                });
            if let Some((pattern, index)) = kept {
                self.selected_pattern
                    .set(behavior, i16::try_from(index).unwrap_or(PATTERN_NOT_SET));
                Some(pattern)
            } else {
                self.selected_pattern.set(behavior, PATTERN_NOT_SET);
                None
            }
        };

        let Some(display) = display else {
            result_container.set_item(0, ItemStack::empty());
            return;
        };

        // A banner already carrying the maximum layers cannot take another.
        let layer_count = banner
            .get(BANNER_PATTERNS)
            .map_or(0, |layers| layers.layers().len());
        if layer_count >= MAX_PATTERN_LAYERS {
            self.selected_pattern.set(behavior, PATTERN_NOT_SET);
            result_container.set_item(0, ItemStack::empty());
            return;
        }

        result_container.set_item(0, Self::build_result(banner, dye, display));
    }
}

impl MenuKind for LoomKind {
    /// Returns true while the original loom remains in range.
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.world.get_block_state(self.block_pos).get_block() == &vanilla_blocks::LOOM
            && player.is_within_block_interaction_range_with_buffer(self.block_pos, 4.0)
    }

    fn slots_changed(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        self.recompute(behavior, guard);
    }

    /// Vanilla `LoomMenu.clickMenuButton`: the button id picks a selectable pattern.
    fn on_button_click(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        button_id: i32,
        _player: &Player,
    ) -> bool {
        let Ok(index) = usize::try_from(button_id) else {
            return false;
        };
        let Some(pattern) = self.selectable_patterns.lock().get(index).copied() else {
            return false;
        };

        self.selected_pattern
            .set(behavior, i16::try_from(index).unwrap_or(PATTERN_NOT_SET));

        let Some([input_container, result_container]) = guard.get_disjoint_mut([
            ContainerId::from_arc(&self.input_container),
            ContainerId::from_arc(&self.result_container),
        ]) else {
            return false;
        };
        let [banner, dye, _] = input_container.items() else {
            return false;
        };

        result_container.set_item(0, Self::build_result(banner, dye, pattern));
        true
    }

    /// Clears the virtual result on close. Inputs are drained by [`Menu::removed`].
    fn removed(&mut self, _behavior: &mut MenuBehavior, _player: &Player) {
        self.result_container.lock().set_item(0, ItemStack::empty());
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::data_components::vanilla_components::{BANNER_PATTERNS, DYE};
    use steel_registry::item_stack::ItemStack;
    use steel_registry::test_support::init_test_registry;
    use steel_registry::{DyeColor, vanilla_banner_patterns, vanilla_items};

    use super::LoomKind;

    #[test]
    fn an_empty_pattern_slot_offers_the_patterns_a_loom_needs_no_item_for() {
        init_test_registry();

        let patterns = LoomKind::selectable_patterns(&ItemStack::empty());

        assert!(!patterns.is_empty());
        assert!(patterns.contains(&&vanilla_banner_patterns::STRIPE_TOP));
        // A creeper charge needs its banner pattern item, so a bare loom cannot weave it.
        assert!(!patterns.contains(&&vanilla_banner_patterns::CREEPER));
    }

    #[test]
    fn weaving_appends_a_layer_in_the_dye_color_and_keeps_the_existing_ones() {
        init_test_registry();
        let banner = ItemStack::new(&vanilla_items::WHITE_BANNER);
        let mut dye = ItemStack::new(&vanilla_items::RED_DYE);
        dye.set(DYE, DyeColor::Red);

        let first = LoomKind::build_result(&banner, &dye, &vanilla_banner_patterns::CREEPER);
        let second = LoomKind::build_result(&first, &dye, &vanilla_banner_patterns::STRIPE_TOP);

        let layers = second
            .get(BANNER_PATTERNS)
            .expect("woven banner should carry pattern layers");
        assert_eq!(layers.layers().len(), 2);
        assert_eq!(layers.layers()[1].color(), DyeColor::Red);
        assert_eq!(second.count(), 1);
    }

    #[test]
    fn weaving_without_a_banner_yields_nothing() {
        init_test_registry();
        let mut dye = ItemStack::new(&vanilla_items::RED_DYE);
        dye.set(DYE, DyeColor::Red);

        assert!(
            LoomKind::build_result(&ItemStack::empty(), &dye, &vanilla_banner_patterns::CREEPER)
                .is_empty()
        );
    }
}
