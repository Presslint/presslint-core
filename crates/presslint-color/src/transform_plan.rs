//! Report-only transform planning.

use serde::{Deserialize, Serialize};

use crate::color_policy::{ColorPolicyDecision, resolve_color_policy};
use crate::devicelink::{DeviceLinkDecision, DeviceLinkDescription, resolve_device_link_policy};
use crate::output_intent::{
    ObservedOutputIntent, OutputIntentDecision, resolve_output_intent_policy,
};
use crate::overprint::ObservedOverprintInteraction;
use crate::policy::TransformPlanRequest;
use crate::spot::ObservedSpotColor;

/// Pure report-only decision for one abstract transform plan.
///
/// This decision carries the focused sub-decisions verbatim. It does not add a
/// top-level rejection policy, execute transforms, parse ICC profiles, inspect
/// PDF catalogs or graphics state, or mutate PDF bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformPlanDecision {
    /// Resolved `DeviceLink` sub-decision.
    pub device_link: DeviceLinkDecision,
    /// Resolved output-intent sub-decision.
    pub output_intent: OutputIntentDecision,
    /// Resolved spot/overprint color-policy sub-decision.
    pub color_policy: ColorPolicyDecision,
}

/// Resolve one report-only transform plan by delegating to focused resolvers.
///
/// This function is pure: it performs no I/O, reads no PDF bytes, parses no ICC
/// profile, inspects no PDF catalog or graphics state, evaluates no tint
/// transform, simulates no overprint behavior, executes no color transform, and
/// mutates no PDF bytes.
///
/// It is intentionally only an aggregator. The returned fields equal:
///
/// - `resolve_device_link_policy(request.device_link, &request.transform, ...)`
/// - `resolve_output_intent_policy(&request.output_intent, ...)`
/// - `resolve_color_policy(&request.transform.policy, ...)`
///
/// Caller iteration order is therefore preserved exactly as the nested
/// resolvers preserve it.
#[must_use]
pub fn resolve_transform_plan<D, I, S, O>(
    request: &TransformPlanRequest,
    device_links: D,
    output_intents: I,
    spot_observed: S,
    overprint_observed: O,
) -> TransformPlanDecision
where
    D: IntoIterator<Item = DeviceLinkDescription>,
    I: IntoIterator<Item = ObservedOutputIntent>,
    S: IntoIterator<Item = ObservedSpotColor>,
    O: IntoIterator<Item = ObservedOverprintInteraction>,
{
    TransformPlanDecision {
        device_link: resolve_device_link_policy(
            request.device_link,
            &request.transform,
            device_links,
        ),
        output_intent: resolve_output_intent_policy(&request.output_intent, output_intents),
        color_policy: resolve_color_policy(
            &request.transform.policy,
            spot_observed,
            overprint_observed,
        ),
    }
}
