//! Output-intent policy resolution.

use serde::{Deserialize, Serialize};

use crate::policy::{OutputIntentPolicy, OutputIntentSubtype, OutputIntentTarget};

/// Caller-supplied, ICC-free description of one output intent already observed
/// in a document.
///
/// This is a planning input only. It carries no ICC data and no profile bytes:
/// an observed intent is described abstractly by its [`OutputIntentSubtype`]
/// and its output-condition identifier string. `presslint-color` never derives
/// these values from PDF bytes; a caller supplies them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedOutputIntent {
    /// Observed output intent subtype (the `S` entry of a real dictionary).
    pub subtype: OutputIntentSubtype,
    /// Observed output-condition identifier string.
    pub output_condition_identifier: String,
}

/// Reason an output-intent policy could not be satisfied by the observed state.
///
/// This is a report-only planning result; it triggers no PDF mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum OutputIntentRejection {
    /// `RequireExisting` was requested but no output intent was observed.
    NoExistingIntent,
}

/// Pure resolution of an [`OutputIntentPolicy`] against the observed state.
///
/// This decision is a planning input for a later PDF writer only. Producing it
/// inspects no PDF catalog, parses no ICC profile, and mutates no PDF bytes; it
/// reports what a writer should do, not what it has done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum OutputIntentDecision {
    /// `Preserve`: leave any existing intents untouched; nothing to plan.
    Preserve,
    /// `RequireExisting` satisfied: at least one output intent is present.
    SatisfiedByExisting,
    /// A policy could not be satisfied by the observed state.
    Rejected {
        /// Structured rejection reason.
        rejection: OutputIntentRejection,
    },
    /// `EnsureTarget` already satisfied: an observed intent matches the
    /// requested target identity (same subtype and output-condition identifier).
    AlreadySatisfied {
        /// Requested target that the observed state already satisfies.
        target: OutputIntentTarget,
    },
    /// `EnsureTarget` conflict: an observed intent shares the requested subtype
    /// but carries a different output-condition identifier.
    ConflictsWithExisting {
        /// Requested target a later writer was asked to ensure.
        requested: OutputIntentTarget,
        /// First observed intent that conflicts with the requested target.
        existing: ObservedOutputIntent,
    },
    /// `EnsureTarget` otherwise: a later writer must ensure the requested target.
    RequiresEnsureTarget {
        /// Requested target a later writer must ensure.
        target: OutputIntentTarget,
    },
}

/// Extract the comparable identity (`subtype`, output-condition identifier) of a
/// requested target.
///
/// Target identity is compared only by [`OutputIntentSubtype`] and the
/// output-condition identifier string. This deliberately ignores
/// `registry_name`, `info`, and any profile bytes; both target variants expose
/// the same two comparable fields.
const fn target_identity(target: &OutputIntentTarget) -> (OutputIntentSubtype, &str) {
    match target {
        OutputIntentTarget::NamedCondition { condition } => (
            condition.subtype,
            condition.output_condition_identifier.as_str(),
        ),
        OutputIntentTarget::ProfileBacked { intent } => {
            (intent.subtype, intent.output_condition_identifier.as_str())
        }
    }
}

/// Resolve an [`OutputIntentPolicy`] against the document's observed output
/// intents into a structured [`OutputIntentDecision`].
///
/// This function is pure: it performs no I/O, reads no PDF bytes, parses no ICC
/// profile, and does not panic on valid input. It is a planning input for a
/// later writer only.
///
/// Resolution rules:
///
/// - `Preserve` resolves to [`OutputIntentDecision::Preserve`] regardless of the
///   observed state.
/// - `RequireExisting` resolves to [`OutputIntentDecision::SatisfiedByExisting`]
///   when at least one intent is observed, otherwise to a
///   [`OutputIntentDecision::Rejected`] with
///   [`OutputIntentRejection::NoExistingIntent`].
/// - `EnsureTarget` resolves to [`OutputIntentDecision::AlreadySatisfied`] when
///   an observed intent matches the requested target identity, to
///   [`OutputIntentDecision::ConflictsWithExisting`] when an observed intent
///   shares the requested subtype but carries a different identifier, and
///   otherwise to [`OutputIntentDecision::RequiresEnsureTarget`].
///
/// When several intents are observed, a match takes priority over a conflict,
/// and a conflict takes priority over requires-ensure-target.
#[must_use]
pub fn resolve_output_intent_policy<I>(
    policy: &OutputIntentPolicy,
    observed: I,
) -> OutputIntentDecision
where
    I: IntoIterator<Item = ObservedOutputIntent>,
{
    match policy {
        OutputIntentPolicy::Preserve => OutputIntentDecision::Preserve,
        OutputIntentPolicy::RequireExisting => {
            if observed.into_iter().next().is_some() {
                OutputIntentDecision::SatisfiedByExisting
            } else {
                OutputIntentDecision::Rejected {
                    rejection: OutputIntentRejection::NoExistingIntent,
                }
            }
        }
        OutputIntentPolicy::EnsureTarget { target } => {
            let (subtype, identifier) = target_identity(target);
            // The first same-subtype, different-identifier intent is remembered
            // as a conflict, but a later exact match still wins: match takes
            // priority over conflict.
            let mut conflict: Option<ObservedOutputIntent> = None;
            for intent in observed {
                if intent.subtype == subtype {
                    if intent.output_condition_identifier.as_str() == identifier {
                        return OutputIntentDecision::AlreadySatisfied {
                            target: target.clone(),
                        };
                    }
                    if conflict.is_none() {
                        conflict = Some(intent);
                    }
                }
            }
            conflict.map_or_else(
                || OutputIntentDecision::RequiresEnsureTarget {
                    target: target.clone(),
                },
                |existing| OutputIntentDecision::ConflictsWithExisting {
                    requested: target.clone(),
                    existing,
                },
            )
        }
    }
}
