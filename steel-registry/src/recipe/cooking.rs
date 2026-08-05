//! Cooking recipe types.

use steel_utils::Identifier;

use crate::item_stack::ItemStack;

use super::{Ingredient, RecipeResult};

macro_rules! define_cooking_recipe {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Debug)]
        pub struct $name {
            pub id: Identifier,
            pub ingredient: Ingredient,
            pub result: RecipeResult,
            pub experience: f32,
            pub cooking_time: i32,
        }

        impl $name {
            /// Returns whether this recipe accepts `input`.
            #[must_use]
            pub fn matches(&self, input: &ItemStack) -> bool {
                self.ingredient.test(input)
            }

            /// Assembles the result stack.
            #[must_use]
            pub fn assemble_result(&self, input_count: i32, use_input_count: bool) -> ItemStack {
                let count = if use_input_count { input_count } else { 1 };
                let mut result = self.result.to_item_stack();
                result.set_count(
                    count
                        .saturating_mul(result.count())
                        .min(result.max_stack_size()),
                );
                result
            }

            /// Assembles the result for normal cooking (single output).
            #[must_use]
            pub fn assemble(&self, input: &ItemStack) -> ItemStack {
                let _ = input;
                self.assemble_result(1, false)
            }
        }
    };
}

define_cooking_recipe!(SmeltingRecipe, "A furnace smelting recipe.");
define_cooking_recipe!(BlastingRecipe, "A blast furnace blasting recipe.");
define_cooking_recipe!(SmokingRecipe, "A smoker smoking recipe.");
define_cooking_recipe!(CampfireCookingRecipe, "A campfire cooking recipe.");

/// A stonecutter stonecutting recipe.
#[derive(Debug)]
pub struct StonecuttingRecipe {
    pub id: Identifier,
    pub ingredient: Ingredient,
    pub result: RecipeResult,
}

impl StonecuttingRecipe {
    #[must_use]
    pub fn matches(&self, input: &ItemStack) -> bool {
        self.ingredient.test(input)
    }

    #[must_use]
    pub fn assemble(&self) -> ItemStack {
        self.result.to_item_stack()
    }
}

#[cfg(test)]
mod tests {
    use steel_utils::Identifier;

    use crate::recipe::{Ingredient, RecipeResult};
    use crate::{test_support::init_test_registry, vanilla_items};

    use super::*;

    #[test]
    fn smelting_result_uses_input_count_when_requested() {
        init_test_registry();
        let recipe = SmeltingRecipe {
            id: Identifier::vanilla_static("test"),
            ingredient: Ingredient::Item(&vanilla_items::RAW_IRON),
            result: RecipeResult {
                item: &vanilla_items::IRON_INGOT,
                count: 1,
            },
            experience: 0.0,
            cooking_time: 200,
        };

        let result = recipe.assemble_result(3, true);

        assert!(result.is(&vanilla_items::IRON_INGOT));
        assert_eq!(result.count(), 3);
    }

    #[test]
    fn smelting_result_can_ignore_input_count() {
        init_test_registry();
        let recipe = SmeltingRecipe {
            id: Identifier::vanilla_static("test"),
            ingredient: Ingredient::Item(&vanilla_items::RAW_IRON),
            result: RecipeResult {
                item: &vanilla_items::IRON_INGOT,
                count: 1,
            },
            experience: 0.0,
            cooking_time: 200,
        };

        let result = recipe.assemble_result(3, false);

        assert!(result.is(&vanilla_items::IRON_INGOT));
        assert_eq!(result.count(), 1);
    }
}
