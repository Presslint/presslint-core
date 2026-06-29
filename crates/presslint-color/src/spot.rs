//! Spot-color policy resolution.

use presslint_core::ColorSpace;
use serde::{Deserialize, Serialize};

use crate::policy::SpotPolicy;

/// Caller-supplied, ICC-free description of one spot color observed in a job.
///
/// This is a planning input only. It carries no ICC data, no tint-transform
/// function, and no color components: an observed spot is described abstractly
/// by its colorant `name` and the abstract `alternate` color space its tint
/// transform targets. `presslint-color` never derives these values from PDF
/// bytes; a caller supplies them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedSpotColor {
    /// Colorant name of the observed spot (e.g. the `Separation` name).
    pub name: String,
    /// Abstract alternate color space the spot's tint transform targets.
    pub alternate: ColorSpace,
}

/// Reason a spot policy could not be satisfied by the observed state.
///
/// This is a report-only planning result; it triggers no PDF mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum SpotRejection {
    /// `Reject` was requested but at least one spot color was observed.
    SpotConversionRequired,
}

/// Reason a single observed spot was skipped instead of converted.
///
/// This is a report-only planning result; it triggers no PDF mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum SpotSkipReason {
    /// The spot's alternate color space is not a process device color space, so
    /// this slice cannot plan its conversion.
    UnsupportedAlternate,
}

/// One observed spot that `ConvertAlternate` explicitly skipped, paired with the
/// reason it was skipped.
///
/// Reporting a skip rather than dropping the spot keeps unsupported shapes
/// preserved or explicitly skipped, never silently converted partially wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedSpotConversion {
    /// The observed spot that was skipped.
    pub spot: ObservedSpotColor,
    /// Structured reason the spot was skipped.
    pub reason: SpotSkipReason,
}

/// Pure resolution of a [`SpotPolicy`] against the observed spot colors.
///
/// This decision is a planning input for a later planner only. Producing it
/// inspects no PDF catalog, reads no `Separation`/`DeviceN` color space, parses
/// no ICC profile, evaluates no tint transform, and mutates no PDF bytes; it
/// reports what a later planner should do, not what it has done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum SpotDecision {
    /// `Preserve`: leave any observed spot colors as is; nothing to plan.
    Preserve,
    /// `Reject` satisfied: no spot color was observed.
    NoSpotColors,
    /// `Reject` could not be satisfied: at least one spot color was observed.
    Rejected {
        /// Structured rejection reason.
        rejection: SpotRejection,
    },
    /// `ConvertAlternate`: the observed spots partitioned into those eligible for
    /// alternate conversion and those explicitly skipped, both in caller order.
    ConvertAlternate {
        /// Spots whose alternate is a process device color space, in caller order.
        eligible: Vec<ObservedSpotColor>,
        /// Spots explicitly skipped with a structured reason, in caller order.
        skipped: Vec<SkippedSpotConversion>,
    },
}

/// Whether a color space is a process device color space.
///
/// "Process device color space" means exactly [`ColorSpace::DeviceGray`],
/// [`ColorSpace::DeviceRgb`], or [`ColorSpace::DeviceCmyk`], matching the
/// process-color notion used elsewhere in the workspace. Every other
/// `ColorSpace` (`IccBased`, `Lab`, `CalGray`, `CalRgb`, `Indexed`,
/// `Separation`, `DeviceN`, `Pattern`, `Resource`, and `Unknown`) is not a
/// process device color space for this slice.
const fn is_process_device_color_space(space: &ColorSpace) -> bool {
    matches!(
        space,
        ColorSpace::DeviceGray | ColorSpace::DeviceRgb | ColorSpace::DeviceCmyk
    )
}

/// Resolve a [`SpotPolicy`] against the job's observed spot colors into a
/// structured [`SpotDecision`].
///
/// This function is pure: it performs no I/O, reads no PDF bytes, parses no ICC
/// profile, evaluates no tint transform, and does not panic on valid input. It
/// is a planning input for a later planner only.
///
/// Resolution rules:
///
/// - `Preserve` resolves to [`SpotDecision::Preserve`] regardless of the observed
///   spot colors.
/// - `Reject` resolves to [`SpotDecision::NoSpotColors`] when no spot color is
///   observed, and otherwise to a [`SpotDecision::Rejected`] with
///   [`SpotRejection::SpotConversionRequired`].
/// - `ConvertAlternate` partitions the observed spots in a single pass: a spot is
///   `eligible` only when its alternate is a process device color space (see
///   [`is_process_device_color_space`]), otherwise it is `skipped` with
///   [`SpotSkipReason::UnsupportedAlternate`]. Both lists preserve caller
///   iteration order.
#[must_use]
pub fn resolve_spot_policy<I>(policy: SpotPolicy, observed: I) -> SpotDecision
where
    I: IntoIterator<Item = ObservedSpotColor>,
{
    match policy {
        SpotPolicy::Preserve => SpotDecision::Preserve,
        SpotPolicy::Reject => {
            if observed.into_iter().next().is_some() {
                SpotDecision::Rejected {
                    rejection: SpotRejection::SpotConversionRequired,
                }
            } else {
                SpotDecision::NoSpotColors
            }
        }
        SpotPolicy::ConvertAlternate => {
            let mut eligible = Vec::new();
            let mut skipped = Vec::new();
            for spot in observed {
                if is_process_device_color_space(&spot.alternate) {
                    eligible.push(spot);
                } else {
                    skipped.push(SkippedSpotConversion {
                        spot,
                        reason: SpotSkipReason::UnsupportedAlternate,
                    });
                }
            }
            SpotDecision::ConvertAlternate { eligible, skipped }
        }
    }
}
