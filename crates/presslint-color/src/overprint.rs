//! Overprint policy resolution.

use presslint_core::ColorSpace;
use serde::{Deserialize, Serialize};

use crate::policy::OverprintPolicy;

/// Caller-supplied, PDF-free description of one overprint-sensitive conversion.
///
/// This is a planning input only. It carries no graphics state, page resource,
/// content-stream operand, transparency group, or PDF bytes: callers describe
/// the abstract conversion by a stable `id`, source/destination color spaces,
/// and a compact sensitivity classification. `presslint-color` never derives
/// these values from PDF content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedOverprintInteraction {
    /// Stable caller-defined identifier for this observation.
    pub id: String,
    /// Source color space before the planned conversion.
    pub source: ColorSpace,
    /// Destination color space after the planned conversion.
    pub destination: ColorSpace,
    /// Caller-supplied sensitivity/risk classification for this conversion.
    pub sensitivity: OverprintSensitivity,
}

/// Caller-supplied overprint sensitivity classification.
///
/// `Safe` means the caller observed no unsafe overprint-sensitive conversion for
/// this interaction. The remaining variants are conservative risk markers used
/// only for report-only rejection or mitigation planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverprintSensitivity {
    /// No unsafe overprint-sensitive conversion was observed.
    Safe,
    /// A process-color conversion may turn zero components into non-zero
    /// components, changing overprint-mode knockout behavior.
    ProcessColorantExpansion,
    /// A spot-to-process conversion may change spot/process overprint behavior.
    SpotColorantConversion,
    /// The caller observed an overprint-sensitive conversion this crate cannot
    /// name a mitigation for.
    Unsupported,
}

/// Report-only overprint mitigation candidate for a later planner.
///
/// This value names a possible mitigation only. It does not mutate PDF bytes,
/// rewrite graphics state, simulate overprint, flatten transparency, or execute
/// a color transform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverprintMitigation {
    /// The observed interaction this mitigation would apply to.
    pub interaction: ObservedOverprintInteraction,
    /// Named report-only mitigation action.
    pub action: OverprintMitigationAction,
}

/// Supported report-only overprint mitigation actions this crate can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverprintMitigationAction {
    /// Preserve zero process colorants across conversion when a later color
    /// planner can do so, avoiding accidental knockout under overprint mode.
    PreserveZeroProcessColorants,
    /// Preserve spot/process overprint appearance through a later dedicated
    /// planning step, without promising this crate can rewrite the PDF.
    PreserveSpotOverprintAppearance,
}

/// Reason an overprint policy could not be satisfied by the observed state.
///
/// This is a report-only planning result; it triggers no PDF mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum OverprintRejection {
    /// `RejectUnsafe` was requested but one or more unsafe overprint-sensitive
    /// conversions were observed.
    UnsafeOverprintSensitiveConversions {
        /// Unsafe observations in caller iteration order.
        interactions: Vec<ObservedOverprintInteraction>,
    },
}

/// Reason an observed overprint interaction was skipped instead of mitigated.
///
/// This is a report-only planning result; it triggers no PDF mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum OverprintSkipReason {
    /// The observed interaction is unsafe, but this crate cannot name a
    /// supported mitigation for its sensitivity classification.
    UnsupportedInteraction,
}

/// One observed overprint interaction that `Mitigate` explicitly skipped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedOverprintMitigation {
    /// The observed interaction that was skipped.
    pub interaction: ObservedOverprintInteraction,
    /// Structured reason the interaction was skipped.
    pub reason: OverprintSkipReason,
}

/// Pure resolution of an [`OverprintPolicy`] against observed overprint state.
///
/// This decision is a planning input for a later planner only. Producing it
/// inspects no graphics state, reads no `ExtGState` dictionary, simulates no
/// overprint behavior, flattens no transparency, executes no color transform,
/// and mutates no PDF bytes; it reports what a later planner should do, not what
/// it has done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum OverprintDecision {
    /// `Preserve`: leave any observed overprint-sensitive conversions as is.
    Preserve,
    /// `RejectUnsafe` satisfied: no unsafe overprint-sensitive conversion was
    /// observed.
    NoUnsafeOverprint,
    /// A policy could not be satisfied by the observed state.
    Rejected {
        /// Structured rejection reason.
        rejection: OverprintRejection,
    },
    /// `Mitigate`: unsafe observations partitioned into named report-only
    /// mitigation candidates and explicit skips, both in caller order.
    Mitigate {
        /// Supported report-only mitigation candidates, in caller order.
        supported: Vec<OverprintMitigation>,
        /// Unsafe interactions explicitly skipped, in caller order.
        skipped: Vec<SkippedOverprintMitigation>,
    },
}

/// Resolve an [`OverprintPolicy`] against caller-supplied overprint observations
/// into a structured [`OverprintDecision`].
///
/// This function is pure: it performs no I/O, reads no PDF bytes, inspects no
/// graphics state, parses no page resources, executes no color transform, and
/// does not panic on valid input. It is a planning input for a later planner
/// only.
///
/// Resolution rules:
///
/// - `Preserve` resolves to [`OverprintDecision::Preserve`] regardless of the
///   observed state.
/// - `RejectUnsafe` resolves to [`OverprintDecision::NoUnsafeOverprint`] when
///   every observed interaction is [`OverprintSensitivity::Safe`], and otherwise
///   to [`OverprintDecision::Rejected`] with
///   [`OverprintRejection::UnsafeOverprintSensitiveConversions`]. Rejected
///   observations preserve caller iteration order.
/// - `Mitigate` ignores [`OverprintSensitivity::Safe`] observations and
///   partitions unsafe observations in a single pass. Process colorant expansion
///   and spot colorant conversion become supported report-only mitigations;
///   unsupported interactions become explicit skips. Both lists preserve caller
///   iteration order.
#[must_use]
pub fn resolve_overprint_policy<I>(policy: OverprintPolicy, observed: I) -> OverprintDecision
where
    I: IntoIterator<Item = ObservedOverprintInteraction>,
{
    match policy {
        OverprintPolicy::Preserve => OverprintDecision::Preserve,
        OverprintPolicy::RejectUnsafe => {
            let interactions: Vec<_> = observed
                .into_iter()
                .filter(|interaction| interaction.sensitivity != OverprintSensitivity::Safe)
                .collect();
            if interactions.is_empty() {
                OverprintDecision::NoUnsafeOverprint
            } else {
                OverprintDecision::Rejected {
                    rejection: OverprintRejection::UnsafeOverprintSensitiveConversions {
                        interactions,
                    },
                }
            }
        }
        OverprintPolicy::Mitigate => {
            let mut supported = Vec::new();
            let mut skipped = Vec::new();
            for interaction in observed {
                match interaction.sensitivity {
                    OverprintSensitivity::Safe => {}
                    OverprintSensitivity::ProcessColorantExpansion => {
                        supported.push(OverprintMitigation {
                            interaction,
                            action: OverprintMitigationAction::PreserveZeroProcessColorants,
                        });
                    }
                    OverprintSensitivity::SpotColorantConversion => {
                        supported.push(OverprintMitigation {
                            interaction,
                            action: OverprintMitigationAction::PreserveSpotOverprintAppearance,
                        });
                    }
                    OverprintSensitivity::Unsupported => {
                        skipped.push(SkippedOverprintMitigation {
                            interaction,
                            reason: OverprintSkipReason::UnsupportedInteraction,
                        });
                    }
                }
            }
            OverprintDecision::Mitigate { supported, skipped }
        }
    }
}
