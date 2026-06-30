//! Policy and request input contracts shared across color decisions.

use presslint_types::ColorSpace;
use serde::{Deserialize, Serialize};

/// Color-conversion policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorPolicy {
    /// Spot handling.
    pub spot: SpotPolicy,
    /// Overprint handling.
    pub overprint: OverprintPolicy,
}

/// Spot-color handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpotPolicy {
    /// Preserve spot colors.
    Preserve,
    /// Reject jobs that would require spot conversion.
    Reject,
    /// Convert spot alternate colors when supported.
    ConvertAlternate,
}

/// Overprint handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverprintPolicy {
    /// Preserve and report.
    Preserve,
    /// Reject unsafe overprint-sensitive conversions.
    RejectUnsafe,
    /// Apply supported mitigation rules.
    Mitigate,
}

/// Abstract color transform request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformRequest {
    /// Source color space.
    pub source: ColorSpace,
    /// Destination color space.
    pub destination: ColorSpace,
    /// Policy for ambiguous prepress semantics.
    pub policy: ColorPolicy,
}

/// Report-only planning request for one abstract color transform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformPlanRequest {
    /// Abstract color transform to plan.
    pub transform: TransformRequest,
    /// `DeviceLink` selection policy for the transform.
    pub device_link: DeviceLinkPolicy,
    /// Document-level output-intent policy for the transform plan.
    pub output_intent: OutputIntentPolicy,
}

/// `DeviceLink` usage policy for a transform request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceLinkPolicy {
    /// Require a matching `DeviceLink` and reject the plan when none is supplied.
    Require,
    /// Use a matching `DeviceLink` when present, otherwise plan via PCS.
    Prefer,
    /// Never use `DeviceLink` profiles for this request; always plan via PCS.
    Forbid,
}

/// Document-level output-intent policy for future color planning.
///
/// This contract is a planning input only. It does not inspect existing PDF
/// catalog entries, parse ICC profile contents, embed streams, or mutate PDF
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "policy", rename_all = "snake_case")]
pub enum OutputIntentPolicy {
    /// Leave any existing output intents untouched and do not require one.
    Preserve,
    /// Require a suitable output intent to already be present before writing.
    RequireExisting,
    /// Ask a later PDF writer to ensure that the requested target exists.
    EnsureTarget {
        /// Requested output intent target.
        target: OutputIntentTarget,
    },
}

/// Target output condition requested by an output-intent policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputIntentTarget {
    /// A named production condition, typically resolved by a registry.
    NamedCondition {
        /// Named output condition to reference.
        condition: NamedOutputCondition,
    },
    /// A target backed by an explicit profile source supplied to a future writer.
    ProfileBacked {
        /// Profile-backed output intent request.
        intent: ProfileBackedOutputIntent,
    },
}

/// Output intent subtype for the `S` entry of a future `OutputIntent` dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputIntentSubtype {
    /// PDF/X output intent subtype.
    GtsPdfx,
    /// PDF/A-1 output intent subtype.
    GtsPdfa1,
    /// PDF/E-1 output intent subtype.
    IsoPdfe1,
}

/// Named output condition reference.
///
/// This describes a registry-backed output condition by name. It intentionally
/// carries no ICC data; resolution of the named condition belongs to later
/// planning or writing layers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedOutputCondition {
    /// Output intent subtype to request.
    pub subtype: OutputIntentSubtype,
    /// Registry identifier for the intended output condition.
    pub output_condition_identifier: String,
    /// Registry URI or stable registry name that defines the condition.
    pub registry_name: String,
}

/// Profile-backed output intent request.
///
/// The profile source is opaque to `presslint-color`: these contracts do not
/// validate ICC bytes, derive profile metadata, or decide how a PDF catalog is
/// updated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileBackedOutputIntent {
    /// Output intent subtype to request.
    pub subtype: OutputIntentSubtype,
    /// Identifier for the intended output condition.
    pub output_condition_identifier: String,
    /// Human-readable output condition label.
    pub output_condition: String,
    /// Additional human-readable target condition information.
    pub info: String,
    /// Opaque profile source for a later writer.
    pub profile: OutputProfileSource,
}

/// Opaque profile source for a future output-intent writer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum OutputProfileSource {
    /// Stable profile handle supplied by a caller or higher-level planner.
    OpaqueId {
        /// Opaque profile identifier.
        id: String,
    },
    /// ICC profile bytes supplied by a caller and left unparsed by this crate.
    EmbeddedBytes {
        /// Raw profile bytes.
        bytes: Vec<u8>,
    },
}
