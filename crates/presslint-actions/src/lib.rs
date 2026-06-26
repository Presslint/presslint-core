//! Serializable actions, recipes, and no-op patch planning.

#![forbid(unsafe_code)]

use presslint_core::{ColorSpace, EditCapability, ObjectId};
use presslint_inventory::{Inventory, InventoryEntry};
use presslint_selectors::{Selector, matches as selector_matches};
use serde::{Deserialize, Serialize};

/// Versioned recipe document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Schema version.
    pub schema_version: u32,
    /// Ordered recipe steps.
    pub steps: Vec<RecipeStep>,
}

/// One selector/action pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeStep {
    /// Selector choosing inventory entries.
    pub select: Selector,
    /// Action applied to matching entries.
    pub action: Action,
}

/// Serializable action request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    /// Convert selected process color observations.
    ConvertColor(ConvertColor),
    /// Add spread stroke to selected text.
    SpreadText(SpreadText),
    /// Enforce a minimum vector stroke width.
    MinimumStrokeWidth(MinimumStrokeWidth),
}

impl Action {
    /// Return the inventory edit capability required to plan this action.
    #[must_use]
    pub const fn required_capability(&self) -> EditCapability {
        match self {
            Self::ConvertColor(_) => EditCapability::RewriteColorOperand,
            Self::SpreadText(_) => EditCapability::AddTextSpreadStroke,
            Self::MinimumStrokeWidth(_) => EditCapability::AdjustStrokeWidth,
        }
    }
}

/// Color-conversion action payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConvertColor {
    /// Named target condition or profile identifier.
    pub target: String,
}

/// Text spreading action payload.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SpreadText {
    /// Spread amount in points.
    pub amount_pt: f64,
    /// Whether the added stroke should overprint.
    pub overprint: bool,
}

/// Minimum stroke-width action payload.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MinimumStrokeWidth {
    /// Minimum stroke width in points.
    pub width_pt: f64,
}

/// Explicit patch-plan mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchPlanMode {
    /// Planning report only; no PDF bytes are written or mutated.
    NoOp,
}

/// Deterministic no-op patch plan for a recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchPlan {
    /// Explicitly records that this plan is report-only.
    pub mode: PatchPlanMode,
    /// Planned recipe steps in recipe order.
    pub steps: Vec<ActionPlan>,
}

impl PatchPlan {
    /// Return true when the plan is explicitly report-only.
    #[must_use]
    pub const fn is_no_op(&self) -> bool {
        matches!(self.mode, PatchPlanMode::NoOp)
    }
}

/// Planned action report for one recipe step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionPlan {
    /// Requested action.
    pub action: Action,
    /// Objects selected for the action in inventory order.
    pub targets: Vec<ObjectId>,
    /// Objects matched by the selector but skipped before future mutation.
    pub skipped: Vec<SkippedTarget>,
}

/// Matched inventory entry skipped during planning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedTarget {
    /// Matched object that cannot currently receive the requested action.
    pub object: ObjectId,
    /// Structured reason for the skip.
    pub reason: SkipReason,
}

/// Structured reason a matched object was omitted from action targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum SkipReason {
    /// The inventory entry does not advertise the action's required capability.
    UnsupportedCapability {
        /// Required edit capability for the requested action.
        required: EditCapability,
    },
    /// The matched `ConvertColor` entry advertises the rewrite capability but
    /// carries no process device color observation (`DeviceGray` /
    /// `DeviceRGB` / `DeviceCMYK`) that the no-op planner can target.
    NonProcessColor,
}

/// Return true when `space` is one of the three process device color spaces the
/// no-op convert slice can target (`DeviceGray` / `DeviceRGB` / `DeviceCMYK`).
///
/// Every other `ColorSpace` shape (`IccBased`, `Lab`, `CalGray`, `CalRgb`,
/// `Indexed`, `Separation`, `DeviceN`, `Pattern`, `Resource`, `Unknown`) is
/// treated as non-process for this slice.
const fn is_process_space(space: &ColorSpace) -> bool {
    matches!(
        space,
        ColorSpace::DeviceGray | ColorSpace::DeviceRgb | ColorSpace::DeviceCmyk
    )
}

/// Return the skip reason for a capability-passing matched entry, or `None` when
/// the entry is an action target.
///
/// `ConvertColor` additionally requires at least one process device color
/// observation; `SpreadText` and `MinimumStrokeWidth` rely on the capability
/// check alone and never skip here.
fn action_skip_reason(action: &Action, entry: &InventoryEntry) -> Option<SkipReason> {
    match action {
        Action::ConvertColor(_) => {
            let has_process_color = entry
                .colors
                .iter()
                .any(|color| is_process_space(&color.space));
            (!has_process_color).then_some(SkipReason::NonProcessColor)
        }
        Action::SpreadText(_) | Action::MinimumStrokeWidth(_) => None,
    }
}

/// Evaluate a recipe against an inventory and return a deterministic no-op plan.
///
/// Selectors are evaluated against entries in inventory order. Matching entries
/// without the action's required edit capability are reported as
/// `UnsupportedCapability` skips. For `ConvertColor`, capability-passing entries
/// become targets only when they carry a process device color observation;
/// otherwise they are reported as `NonProcessColor` skips. `SpreadText` and
/// `MinimumStrokeWidth` treat every capability-passing entry as a target.
#[must_use]
pub fn plan_recipe(recipe: &Recipe, inventory: &Inventory) -> PatchPlan {
    let steps = recipe
        .steps
        .iter()
        .map(|step| plan_step(step, inventory))
        .collect();

    PatchPlan {
        mode: PatchPlanMode::NoOp,
        steps,
    }
}

fn plan_step(step: &RecipeStep, inventory: &Inventory) -> ActionPlan {
    let required = step.action.required_capability();
    let mut targets = Vec::new();
    let mut skipped = Vec::new();

    for entry in &inventory.entries {
        if !selector_matches(&step.select, entry) {
            continue;
        }

        // The capability check takes precedence over per-action eligibility.
        let skip_reason = if entry.capabilities.contains(&required) {
            action_skip_reason(&step.action, entry)
        } else {
            Some(SkipReason::UnsupportedCapability { required })
        };

        match skip_reason {
            Some(reason) => skipped.push(SkippedTarget {
                object: entry.id.clone(),
                reason,
            }),
            None => targets.push(entry.id.clone()),
        }
    }

    ActionPlan {
        action: step.action.clone(),
        targets,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use presslint_core::{
        ColorObservation, ColorSpace, ColorUsage, ContentScope, EditCapability, ObjectId,
        ObjectKind, PageIndex, Provenance,
    };
    use presslint_inventory::{Inventory, InventoryEntry};
    use presslint_selectors::{Predicate, Selector};

    use super::{
        Action, ConvertColor, MinimumStrokeWidth, PatchPlanMode, Recipe, RecipeStep, SkipReason,
        SkippedTarget, SpreadText, plan_recipe,
    };

    fn object_id(sequence: u32) -> ObjectId {
        let mut digest = [0; 32];
        digest[0] = u8::try_from(sequence).unwrap_or(u8::MAX);

        ObjectId {
            page: PageIndex(0),
            sequence,
            digest,
        }
    }

    fn entry(
        sequence: u32,
        kind: ObjectKind,
        capabilities: impl IntoIterator<Item = EditCapability>,
    ) -> InventoryEntry {
        InventoryEntry {
            id: object_id(sequence),
            kind,
            provenance: Provenance {
                page: PageIndex(0),
                scope: ContentScope::Page,
                range: None,
            },
            bounds: None,
            colors: Vec::new(),
            capabilities: capabilities.into_iter().collect(),
        }
    }

    fn color_entry(
        sequence: u32,
        kind: ObjectKind,
        capabilities: impl IntoIterator<Item = EditCapability>,
        colors: impl IntoIterator<Item = ColorObservation>,
    ) -> InventoryEntry {
        InventoryEntry {
            colors: colors.into_iter().collect(),
            ..entry(sequence, kind, capabilities)
        }
    }

    fn fill_color(space: ColorSpace) -> ColorObservation {
        ColorObservation {
            usage: ColorUsage::Fill,
            space,
            components: Vec::new(),
            spot_name: None,
            source: None,
        }
    }

    fn convert_color_step(select: Selector) -> RecipeStep {
        recipe_step(
            select,
            Action::ConvertColor(ConvertColor {
                target: "pso-coated-v3".to_owned(),
            }),
        )
    }

    fn recipe_step(select: Selector, action: Action) -> RecipeStep {
        RecipeStep { select, action }
    }

    #[test]
    fn plan_recipe_preserves_selector_matches_in_inventory_order() {
        let inventory = Inventory {
            entries: vec![
                color_entry(
                    2,
                    ObjectKind::Vector,
                    [EditCapability::RewriteColorOperand],
                    [fill_color(ColorSpace::DeviceCmyk)],
                ),
                entry(1, ObjectKind::Text, [EditCapability::AddTextSpreadStroke]),
                color_entry(
                    3,
                    ObjectKind::Vector,
                    [EditCapability::RewriteColorOperand],
                    [fill_color(ColorSpace::DeviceRgb)],
                ),
            ],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![convert_color_step(Selector::Predicate {
                predicate: Predicate::ObjectKind {
                    object_kind: ObjectKind::Vector,
                },
            })],
        };

        let plan = plan_recipe(&recipe, &inventory);

        assert_eq!(plan.mode, PatchPlanMode::NoOp);
        assert!(plan.is_no_op());
        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].targets, vec![object_id(2), object_id(3)]);
        assert!(plan.steps[0].skipped.is_empty());
    }

    #[test]
    fn plan_recipe_reports_empty_matches_without_skips() {
        let inventory = Inventory {
            entries: vec![entry(
                1,
                ObjectKind::Vector,
                [EditCapability::RewriteColorOperand],
            )],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![recipe_step(
                Selector::None,
                Action::MinimumStrokeWidth(MinimumStrokeWidth { width_pt: 0.25 }),
            )],
        };

        let plan = plan_recipe(&recipe, &inventory);

        assert_eq!(plan.mode, PatchPlanMode::NoOp);
        assert!(plan.steps[0].targets.is_empty());
        assert!(plan.steps[0].skipped.is_empty());
    }

    #[test]
    fn plan_recipe_reports_unsupported_capability_skips_in_inventory_order() {
        let inventory = Inventory {
            entries: vec![
                entry(1, ObjectKind::Text, [EditCapability::ReadOnly]),
                entry(2, ObjectKind::Text, [EditCapability::AddTextSpreadStroke]),
                entry(3, ObjectKind::Text, [EditCapability::RewriteColorOperand]),
            ],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![recipe_step(
                Selector::All,
                Action::SpreadText(SpreadText {
                    amount_pt: 0.1,
                    overprint: true,
                }),
            )],
        };

        let plan = plan_recipe(&recipe, &inventory);
        let step = &plan.steps[0];

        assert_eq!(step.targets, vec![object_id(2)]);
        assert_eq!(
            step.skipped,
            vec![
                SkippedTarget {
                    object: object_id(1),
                    reason: SkipReason::UnsupportedCapability {
                        required: EditCapability::AddTextSpreadStroke,
                    },
                },
                SkippedTarget {
                    object: object_id(3),
                    reason: SkipReason::UnsupportedCapability {
                        required: EditCapability::AddTextSpreadStroke,
                    },
                },
            ]
        );
    }

    #[test]
    fn convert_color_accepts_process_device_fill_target() {
        let inventory = Inventory {
            entries: vec![color_entry(
                1,
                ObjectKind::Vector,
                [EditCapability::RewriteColorOperand],
                [fill_color(ColorSpace::DeviceCmyk)],
            )],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![convert_color_step(Selector::All)],
        };

        let plan = plan_recipe(&recipe, &inventory);
        let step = &plan.steps[0];

        assert_eq!(step.targets, vec![object_id(1)]);
        assert!(step.skipped.is_empty());
    }

    #[test]
    fn convert_color_skips_spot_separation_as_non_process() {
        let inventory = Inventory {
            entries: vec![color_entry(
                1,
                ObjectKind::Vector,
                [EditCapability::RewriteColorOperand],
                [fill_color(ColorSpace::Separation)],
            )],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![convert_color_step(Selector::All)],
        };

        let plan = plan_recipe(&recipe, &inventory);
        let step = &plan.steps[0];

        assert!(step.targets.is_empty());
        assert_eq!(
            step.skipped,
            vec![SkippedTarget {
                object: object_id(1),
                reason: SkipReason::NonProcessColor,
            }]
        );
    }

    #[test]
    fn convert_color_accepts_mixed_observation_entry() {
        let inventory = Inventory {
            entries: vec![color_entry(
                1,
                ObjectKind::Vector,
                [EditCapability::RewriteColorOperand],
                [
                    fill_color(ColorSpace::Separation),
                    fill_color(ColorSpace::DeviceGray),
                ],
            )],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![convert_color_step(Selector::All)],
        };

        let plan = plan_recipe(&recipe, &inventory);
        let step = &plan.steps[0];

        assert_eq!(step.targets, vec![object_id(1)]);
        assert!(step.skipped.is_empty());
    }

    #[test]
    fn convert_color_skips_entry_without_color_observations() {
        let inventory = Inventory {
            entries: vec![entry(
                1,
                ObjectKind::Vector,
                [EditCapability::RewriteColorOperand],
            )],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![convert_color_step(Selector::All)],
        };

        let plan = plan_recipe(&recipe, &inventory);
        let step = &plan.steps[0];

        assert!(step.targets.is_empty());
        assert_eq!(
            step.skipped,
            vec![SkippedTarget {
                object: object_id(1),
                reason: SkipReason::NonProcessColor,
            }]
        );
    }

    #[test]
    fn convert_color_unsupported_capability_precedes_non_process() {
        let inventory = Inventory {
            entries: vec![color_entry(
                1,
                ObjectKind::Vector,
                [EditCapability::ReadOnly],
                [fill_color(ColorSpace::Separation)],
            )],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![convert_color_step(Selector::All)],
        };

        let plan = plan_recipe(&recipe, &inventory);
        let step = &plan.steps[0];

        assert!(step.targets.is_empty());
        assert_eq!(
            step.skipped,
            vec![SkippedTarget {
                object: object_id(1),
                reason: SkipReason::UnsupportedCapability {
                    required: EditCapability::RewriteColorOperand,
                },
            }]
        );
    }

    #[test]
    fn non_color_actions_ignore_process_color_eligibility() {
        let inventory = Inventory {
            entries: vec![
                entry(1, ObjectKind::Text, [EditCapability::AddTextSpreadStroke]),
                entry(2, ObjectKind::Vector, [EditCapability::AdjustStrokeWidth]),
            ],
        };
        let recipe = Recipe {
            schema_version: 1,
            steps: vec![
                recipe_step(
                    Selector::Predicate {
                        predicate: Predicate::ObjectKind {
                            object_kind: ObjectKind::Text,
                        },
                    },
                    Action::SpreadText(SpreadText {
                        amount_pt: 0.1,
                        overprint: false,
                    }),
                ),
                recipe_step(
                    Selector::Predicate {
                        predicate: Predicate::ObjectKind {
                            object_kind: ObjectKind::Vector,
                        },
                    },
                    Action::MinimumStrokeWidth(MinimumStrokeWidth { width_pt: 0.25 }),
                ),
            ],
        };

        let plan = plan_recipe(&recipe, &inventory);

        assert_eq!(plan.steps[0].targets, vec![object_id(1)]);
        assert!(plan.steps[0].skipped.is_empty());
        assert_eq!(plan.steps[1].targets, vec![object_id(2)]);
        assert!(plan.steps[1].skipped.is_empty());
    }
}
