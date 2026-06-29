//! Combined color policy resolution.

use serde::{Deserialize, Serialize};

use crate::overprint::{ObservedOverprintInteraction, OverprintDecision, resolve_overprint_policy};
use crate::policy::ColorPolicy;
use crate::spot::{ObservedSpotColor, SpotDecision, resolve_spot_policy};

/// Pure resolution of a whole [`ColorPolicy`] against caller-supplied
/// observations.
///
/// This decision is a planning input for a later planner only. It carries the
/// resolved spot and overprint sub-decisions and nothing more; producing it
/// inspects no PDF catalog, reads no graphics state, parses no ICC profile,
/// evaluates no tint transform, simulates no overprint behavior, and mutates no
/// PDF bytes. It is observably identical to calling [`resolve_spot_policy`] and
/// [`resolve_overprint_policy`] separately and pairing their outputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorPolicyDecision {
    /// Resolved spot sub-decision, equal to
    /// `resolve_spot_policy(policy.spot, spot_observed)`.
    pub spot: SpotDecision,
    /// Resolved overprint sub-decision, equal to
    /// `resolve_overprint_policy(policy.overprint, overprint_observed)`.
    pub overprint: OverprintDecision,
}

/// Resolve a whole [`ColorPolicy`] against caller-supplied spot and overprint
/// observations into a structured [`ColorPolicyDecision`].
///
/// This function is pure: it performs no I/O, reads no PDF bytes, parses no ICC
/// profile, evaluates no tint transform, inspects no graphics state, executes no
/// color transform, and does not panic on valid input. It is a planning input
/// for a later planner only.
///
/// It is a thin aggregator: it delegates to the existing sub-resolvers without
/// reimplementing any spot or overprint logic. The returned `spot` field equals
/// the result of [`resolve_spot_policy`] for `policy.spot` and `spot_observed`,
/// and the `overprint` field equals the result of [`resolve_overprint_policy`]
/// for `policy.overprint` and `overprint_observed`, so caller iteration order is
/// preserved in every nested list exactly as the sub-resolvers produce it.
/// `ColorPolicy.spot` and `ColorPolicy.overprint` are `Copy`, so the policy is
/// read by reference without cloning.
#[must_use]
pub fn resolve_color_policy<S, O>(
    policy: &ColorPolicy,
    spot_observed: S,
    overprint_observed: O,
) -> ColorPolicyDecision
where
    S: IntoIterator<Item = ObservedSpotColor>,
    O: IntoIterator<Item = ObservedOverprintInteraction>,
{
    ColorPolicyDecision {
        spot: resolve_spot_policy(policy.spot, spot_observed),
        overprint: resolve_overprint_policy(policy.overprint, overprint_observed),
    }
}
