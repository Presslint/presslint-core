use presslint_types::ColorSpace;

use super::assert_json_round_trip;
use super::json::Json;
use crate::{
    ColorPolicy, ObservedOverprintInteraction, ObservedSpotColor, OverprintPolicy, SpotPolicy,
    resolve_color_policy, resolve_overprint_policy, resolve_spot_policy,
};

fn policy(spot: SpotPolicy, overprint: OverprintPolicy) -> ColorPolicy {
    ColorPolicy { spot, overprint }
}

fn spot(name: &str, alternate: ColorSpace) -> ObservedSpotColor {
    ObservedSpotColor {
        name: name.to_owned(),
        alternate,
    }
}

fn interaction(
    id: &str,
    source: ColorSpace,
    destination: ColorSpace,
    sensitivity: crate::OverprintSensitivity,
) -> ObservedOverprintInteraction {
    ObservedOverprintInteraction {
        id: id.to_owned(),
        source,
        destination,
        sensitivity,
    }
}

fn process_expansion(id: &str) -> ObservedOverprintInteraction {
    interaction(
        id,
        ColorSpace::DeviceCmyk,
        ColorSpace::DeviceCmyk,
        crate::OverprintSensitivity::ProcessColorantExpansion,
    )
}

fn spot_conversion(id: &str) -> ObservedOverprintInteraction {
    interaction(
        id,
        ColorSpace::Separation,
        ColorSpace::DeviceCmyk,
        crate::OverprintSensitivity::SpotColorantConversion,
    )
}

fn unsupported(id: &str) -> ObservedOverprintInteraction {
    interaction(
        id,
        ColorSpace::Pattern,
        ColorSpace::DeviceCmyk,
        crate::OverprintSensitivity::Unsupported,
    )
}

fn safe(id: &str) -> ObservedOverprintInteraction {
    interaction(
        id,
        ColorSpace::DeviceGray,
        ColorSpace::DeviceCmyk,
        crate::OverprintSensitivity::Safe,
    )
}

// --- Combined resolution tests -----------------------------------------------
//
// The combined resolver is a thin aggregator: its `spot` field must equal
// `resolve_spot_policy(policy.spot, spot_observed)` and its `overprint` field
// must equal `resolve_overprint_policy(policy.overprint, overprint_observed)`
// for the same inputs, with no behavior divergence and caller order preserved in
// every nested list. Each test below pins the combined result against both
// standalone sub-resolver results, so any divergence in either delegate fails.

fn assert_delegates(
    policy: &ColorPolicy,
    spots: Vec<ObservedSpotColor>,
    interactions: Vec<ObservedOverprintInteraction>,
) {
    let combined = resolve_color_policy(policy, spots.clone(), interactions.clone());
    assert_eq!(combined.spot, resolve_spot_policy(policy.spot, spots));
    assert_eq!(
        combined.overprint,
        resolve_overprint_policy(policy.overprint, interactions),
    );
}

#[test]
fn preserve_preserve_delegates_to_both_sub_resolvers() {
    assert_delegates(
        &policy(SpotPolicy::Preserve, OverprintPolicy::Preserve),
        vec![spot("Pantone 185 C", ColorSpace::DeviceCmyk)],
        vec![process_expansion("op")],
    );
}

#[test]
fn reject_reject_unsafe_delegates_to_both_sub_resolvers() {
    assert_delegates(
        &policy(SpotPolicy::Reject, OverprintPolicy::RejectUnsafe),
        vec![spot("Pantone 185 C", ColorSpace::DeviceCmyk)],
        vec![safe("safe"), spot_conversion("unsafe")],
    );
}

#[test]
fn convert_alternate_mitigate_delegates_to_both_sub_resolvers() {
    assert_delegates(
        &policy(SpotPolicy::ConvertAlternate, OverprintPolicy::Mitigate),
        vec![
            spot("Cmyk Spot", ColorSpace::DeviceCmyk),
            spot("Lab Spot", ColorSpace::Lab),
        ],
        vec![
            process_expansion("process"),
            unsupported("skip"),
            spot_conversion("spot"),
        ],
    );
}

#[test]
fn mixed_policies_delegate_independently() {
    // Spot rejects while overprint mitigates: the two fields resolve through
    // unrelated branches, so the aggregator must keep them independent.
    assert_delegates(
        &policy(SpotPolicy::Reject, OverprintPolicy::Mitigate),
        vec![spot("Sep Spot", ColorSpace::Separation)],
        vec![
            safe("safe"),
            process_expansion("process"),
            unsupported("skip"),
        ],
    );
}

#[test]
fn empty_observations_delegate_to_both_sub_resolvers() {
    assert_delegates(
        &policy(SpotPolicy::Reject, OverprintPolicy::RejectUnsafe),
        vec![],
        vec![],
    );
    assert_delegates(
        &policy(SpotPolicy::ConvertAlternate, OverprintPolicy::Mitigate),
        vec![],
        vec![],
    );
}

// --- Combined decision shape test --------------------------------------------
//
// This locks the public JSON encoding of `ColorPolicyDecision`. The struct only
// nests the already-locked `SpotDecision` and `OverprintDecision` shapes, so the
// fixture reuses the existing harness and pins exactly the two field names
// (`spot`, `overprint`) and that each nests its decision object verbatim.

#[test]
fn color_policy_decision_has_stable_json_shape() {
    let decision = resolve_color_policy(
        &policy(SpotPolicy::ConvertAlternate, OverprintPolicy::Mitigate),
        vec![
            spot("Cmyk Spot", ColorSpace::DeviceCmyk),
            spot("Lab Spot", ColorSpace::Lab),
        ],
        vec![spot_conversion("spot"), unsupported("skip")],
    );
    assert_json_round_trip(
        &decision,
        Json::object([
            (
                "spot",
                Json::object([
                    ("decision", Json::string("convert_alternate")),
                    (
                        "eligible",
                        Json::array([Json::object([
                            ("name", Json::string("Cmyk Spot")),
                            ("alternate", Json::string("device_cmyk")),
                        ])]),
                    ),
                    (
                        "skipped",
                        Json::array([Json::object([
                            (
                                "spot",
                                Json::object([
                                    ("name", Json::string("Lab Spot")),
                                    ("alternate", Json::string("lab")),
                                ]),
                            ),
                            (
                                "reason",
                                Json::object([("reason", Json::string("unsupported_alternate"))]),
                            ),
                        ])]),
                    ),
                ]),
            ),
            (
                "overprint",
                Json::object([
                    ("decision", Json::string("mitigate")),
                    (
                        "supported",
                        Json::array([Json::object([
                            (
                                "interaction",
                                Json::object([
                                    ("id", Json::string("spot")),
                                    ("source", Json::string("separation")),
                                    ("destination", Json::string("device_cmyk")),
                                    ("sensitivity", Json::string("spot_colorant_conversion")),
                                ]),
                            ),
                            ("action", Json::string("preserve_spot_overprint_appearance")),
                        ])]),
                    ),
                    (
                        "skipped",
                        Json::array([Json::object([
                            (
                                "interaction",
                                Json::object([
                                    ("id", Json::string("skip")),
                                    ("source", Json::string("pattern")),
                                    ("destination", Json::string("device_cmyk")),
                                    ("sensitivity", Json::string("unsupported")),
                                ]),
                            ),
                            (
                                "reason",
                                Json::object([("reason", Json::string("unsupported_interaction"))]),
                            ),
                        ])]),
                    ),
                ]),
            ),
        ]),
    );
}
