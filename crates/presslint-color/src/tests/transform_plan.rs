use presslint_types::ColorSpace;

use super::assert_json_round_trip;
use super::json::Json;
use crate::{
    ColorPolicy, DeviceLinkDecision, DeviceLinkDescription, DeviceLinkPolicy, DeviceLinkRejection,
    ObservedOutputIntent, ObservedOverprintInteraction, ObservedSpotColor, OutputIntentDecision,
    OutputIntentPolicy, OutputIntentSubtype, OutputIntentTarget, OverprintPolicy,
    OverprintSensitivity, SpotPolicy, TransformPlanDecision, TransformPlanRequest,
    TransformRequest, resolve_color_policy, resolve_device_link_policy,
    resolve_output_intent_policy, resolve_transform_plan,
};

fn policy(spot: SpotPolicy, overprint: OverprintPolicy) -> ColorPolicy {
    ColorPolicy { spot, overprint }
}

fn transform(policy: ColorPolicy) -> TransformRequest {
    TransformRequest {
        source: ColorSpace::DeviceRgb,
        destination: ColorSpace::DeviceCmyk,
        policy,
    }
}

fn plan_request(
    device_link: DeviceLinkPolicy,
    output_intent: OutputIntentPolicy,
    policy: ColorPolicy,
) -> TransformPlanRequest {
    TransformPlanRequest {
        transform: transform(policy),
        device_link,
        output_intent,
    }
}

fn device_link(id: &str, source: ColorSpace, destination: ColorSpace) -> DeviceLinkDescription {
    DeviceLinkDescription {
        id: id.to_owned(),
        source,
        destination,
    }
}

fn rgb_to_cmyk_link(id: &str) -> DeviceLinkDescription {
    device_link(id, ColorSpace::DeviceRgb, ColorSpace::DeviceCmyk)
}

fn observed_intent(identifier: &str) -> ObservedOutputIntent {
    ObservedOutputIntent {
        subtype: OutputIntentSubtype::GtsPdfx,
        output_condition_identifier: identifier.to_owned(),
    }
}

fn named_target(identifier: &str) -> OutputIntentTarget {
    OutputIntentTarget::NamedCondition {
        condition: super::named_condition_with_identifier(identifier),
    }
}

fn ensure_target(identifier: &str) -> OutputIntentPolicy {
    OutputIntentPolicy::EnsureTarget {
        target: named_target(identifier),
    }
}

fn spot(name: &str, alternate: ColorSpace) -> ObservedSpotColor {
    ObservedSpotColor {
        name: name.to_owned(),
        alternate,
    }
}

fn overprint(id: &str, sensitivity: OverprintSensitivity) -> ObservedOverprintInteraction {
    ObservedOverprintInteraction {
        id: id.to_owned(),
        source: ColorSpace::DeviceCmyk,
        destination: ColorSpace::DeviceCmyk,
        sensitivity,
    }
}

fn assert_delegates(
    request: &TransformPlanRequest,
    device_links: Vec<DeviceLinkDescription>,
    output_intents: Vec<ObservedOutputIntent>,
    spots: Vec<ObservedSpotColor>,
    overprints: Vec<ObservedOverprintInteraction>,
) -> TransformPlanDecision {
    let decision = resolve_transform_plan(
        request,
        device_links.clone(),
        output_intents.clone(),
        spots.clone(),
        overprints.clone(),
    );
    assert_eq!(
        decision.device_link,
        resolve_device_link_policy(request.device_link, &request.transform, device_links),
    );
    assert_eq!(
        decision.output_intent,
        resolve_output_intent_policy(&request.output_intent, output_intents),
    );
    assert_eq!(
        decision.color_policy,
        resolve_color_policy(&request.transform.policy, spots, overprints),
    );
    decision
}

#[test]
fn prefer_selects_first_matching_link_and_returns_other_decisions() {
    let request = plan_request(
        DeviceLinkPolicy::Prefer,
        OutputIntentPolicy::Preserve,
        policy(SpotPolicy::ConvertAlternate, OverprintPolicy::Mitigate),
    );

    let decision = assert_delegates(
        &request,
        vec![
            device_link(
                "gray-to-cmyk",
                ColorSpace::DeviceGray,
                ColorSpace::DeviceCmyk,
            ),
            rgb_to_cmyk_link("first"),
            rgb_to_cmyk_link("second"),
        ],
        vec![observed_intent("FOGRA51")],
        vec![
            spot("Cmyk Spot", ColorSpace::DeviceCmyk),
            spot("Lab Spot", ColorSpace::Lab),
        ],
        vec![
            overprint("process", OverprintSensitivity::ProcessColorantExpansion),
            overprint("unsupported", OverprintSensitivity::Unsupported),
        ],
    );

    assert_eq!(
        decision.device_link,
        DeviceLinkDecision::UseDeviceLink {
            device_link: rgb_to_cmyk_link("first"),
        },
    );
    assert_eq!(decision.output_intent, OutputIntentDecision::Preserve);
}

#[test]
fn require_without_match_keeps_nested_rejection_and_other_decisions() {
    let request = plan_request(
        DeviceLinkPolicy::Require,
        OutputIntentPolicy::RequireExisting,
        policy(SpotPolicy::Reject, OverprintPolicy::RejectUnsafe),
    );

    let decision = assert_delegates(
        &request,
        vec![device_link(
            "gray-to-cmyk",
            ColorSpace::DeviceGray,
            ColorSpace::DeviceCmyk,
        )],
        vec![observed_intent("FOGRA51")],
        vec![spot("Pantone 185 C", ColorSpace::DeviceCmyk)],
        vec![overprint("safe", OverprintSensitivity::Safe)],
    );

    assert_eq!(
        decision.device_link,
        DeviceLinkDecision::Rejected {
            rejection: DeviceLinkRejection::NoMatchingDeviceLink,
        },
    );
    assert_eq!(
        decision.output_intent,
        OutputIntentDecision::SatisfiedByExisting,
    );
}

#[test]
fn ensure_target_already_satisfied_is_preserved() {
    let request = plan_request(
        DeviceLinkPolicy::Forbid,
        ensure_target("FOGRA51"),
        policy(SpotPolicy::Preserve, OverprintPolicy::Preserve),
    );

    let decision = assert_delegates(
        &request,
        vec![rgb_to_cmyk_link("ignored")],
        vec![observed_intent("FOGRA51")],
        vec![spot("Pantone 185 C", ColorSpace::DeviceCmyk)],
        vec![overprint(
            "process",
            OverprintSensitivity::ProcessColorantExpansion,
        )],
    );

    assert_eq!(
        decision.output_intent,
        OutputIntentDecision::AlreadySatisfied {
            target: named_target("FOGRA51"),
        },
    );
}

#[test]
fn transform_plan_decision_has_stable_json_shape() {
    let decision = TransformPlanDecision {
        device_link: DeviceLinkDecision::UseDeviceLink {
            device_link: rgb_to_cmyk_link("rgb-to-cmyk"),
        },
        output_intent: OutputIntentDecision::AlreadySatisfied {
            target: named_target("FOGRA51"),
        },
        color_policy: resolve_color_policy(
            &policy(SpotPolicy::ConvertAlternate, OverprintPolicy::Mitigate),
            [
                spot("Cmyk Spot", ColorSpace::DeviceCmyk),
                spot("Lab Spot", ColorSpace::Lab),
            ],
            [overprint(
                "spot",
                OverprintSensitivity::SpotColorantConversion,
            )],
        ),
    };

    assert_json_round_trip(
        &decision,
        Json::object([
            ("device_link", use_device_link_json()),
            ("output_intent", already_satisfied_output_intent_json()),
            ("color_policy", transform_color_policy_decision_json()),
        ]),
    );
}

fn use_device_link_json() -> Json {
    Json::object([
        ("decision", Json::string("use_device_link")),
        (
            "device_link",
            Json::object([
                ("id", Json::string("rgb-to-cmyk")),
                ("source", Json::string("device_rgb")),
                ("destination", Json::string("device_cmyk")),
            ]),
        ),
    ])
}

fn already_satisfied_output_intent_json() -> Json {
    Json::object([
        ("decision", Json::string("already_satisfied")),
        (
            "target",
            Json::object([
                ("kind", Json::string("named_condition")),
                ("condition", super::named_condition_json()),
            ]),
        ),
    ])
}

fn transform_color_policy_decision_json() -> Json {
    Json::object([
        ("spot", transform_spot_decision_json()),
        ("overprint", transform_overprint_decision_json()),
    ])
}

fn transform_spot_decision_json() -> Json {
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
    ])
}

fn transform_overprint_decision_json() -> Json {
    Json::object([
        ("decision", Json::string("mitigate")),
        (
            "supported",
            Json::array([Json::object([
                ("interaction", spot_overprint_json()),
                ("action", Json::string("preserve_spot_overprint_appearance")),
            ])]),
        ),
        ("skipped", Json::array([])),
    ])
}

fn spot_overprint_json() -> Json {
    Json::object([
        ("id", Json::string("spot")),
        ("source", Json::string("device_cmyk")),
        ("destination", Json::string("device_cmyk")),
        ("sensitivity", Json::string("spot_colorant_conversion")),
    ])
}
