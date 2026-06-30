//! `DeviceLink` selection policy resolution.

use presslint_types::ColorSpace;
use serde::{Deserialize, Serialize};

use crate::policy::{DeviceLinkPolicy, TransformRequest};

/// Caller-supplied, ICC-free description of an available `DeviceLink` profile.
///
/// This is a planning input only. It carries no ICC data and no profile bytes:
/// callers describe the link by a stable identifier plus the abstract source
/// and destination color spaces it connects. `presslint-color` never derives
/// these values from ICC bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceLinkDescription {
    /// Stable caller-defined `DeviceLink` identifier.
    pub id: String,
    /// Source color space accepted by the `DeviceLink`.
    pub source: ColorSpace,
    /// Destination color space produced by the `DeviceLink`.
    pub destination: ColorSpace,
}

/// Reason a `DeviceLink` policy could not be satisfied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum DeviceLinkRejection {
    /// `Require` was requested but no supplied `DeviceLink` matched exactly.
    NoMatchingDeviceLink,
}

/// Pure resolution of a [`DeviceLinkPolicy`] against a transform request.
///
/// This decision is report-only. Producing it parses no ICC bytes, executes no
/// color transform, and mutates no PDF bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum DeviceLinkDecision {
    /// Plan conversion with the first matching caller-supplied `DeviceLink`.
    UseDeviceLink {
        /// Matching `DeviceLink` selected in caller order.
        device_link: DeviceLinkDescription,
    },
    /// Plan ordinary source-profile to destination-profile conversion via PCS.
    UseProfileConnectionSpace,
    /// A required `DeviceLink` could not be selected.
    Rejected {
        /// Structured rejection reason.
        rejection: DeviceLinkRejection,
    },
}

/// Resolve a [`DeviceLinkPolicy`] against an abstract [`TransformRequest`] and
/// caller-supplied available `DeviceLink` descriptions.
///
/// This function is pure: it performs no I/O, reads no PDF bytes, parses no ICC
/// profile, and executes no transform. Matching is deliberately limited to exact
/// equality of `request.source == device_link.source` and
/// `request.destination == device_link.destination`.
///
/// Resolution rules:
///
/// - `Forbid` resolves to [`DeviceLinkDecision::UseProfileConnectionSpace`]
///   without inspecting supplied `DeviceLink` profiles.
/// - `Prefer` resolves to [`DeviceLinkDecision::UseDeviceLink`] for the first
///   exact match in caller order, otherwise to
///   [`DeviceLinkDecision::UseProfileConnectionSpace`].
/// - `Require` resolves to [`DeviceLinkDecision::UseDeviceLink`] for the first
///   exact match in caller order, otherwise to [`DeviceLinkDecision::Rejected`]
///   with [`DeviceLinkRejection::NoMatchingDeviceLink`].
#[must_use]
pub fn resolve_device_link_policy<I>(
    policy: DeviceLinkPolicy,
    request: &TransformRequest,
    available: I,
) -> DeviceLinkDecision
where
    I: IntoIterator<Item = DeviceLinkDescription>,
{
    if policy == DeviceLinkPolicy::Forbid {
        return DeviceLinkDecision::UseProfileConnectionSpace;
    }
    let requires_match = policy == DeviceLinkPolicy::Require;

    for device_link in available {
        if device_link.source == request.source && device_link.destination == request.destination {
            return DeviceLinkDecision::UseDeviceLink { device_link };
        }
    }

    if requires_match {
        DeviceLinkDecision::Rejected {
            rejection: DeviceLinkRejection::NoMatchingDeviceLink,
        }
    } else {
        DeviceLinkDecision::UseProfileConnectionSpace
    }
}
