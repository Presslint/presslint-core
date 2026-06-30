use presslint_types::ColorSpace;

use super::assert_json_round_trip;
use super::json::Json;
use crate::{
    ObservedOverprintInteraction, OverprintDecision, OverprintMitigation,
    OverprintMitigationAction, OverprintPolicy, OverprintRejection, OverprintSensitivity,
    OverprintSkipReason, SkippedOverprintMitigation, resolve_overprint_policy,
};

fn interaction(
    id: &str,
    source: ColorSpace,
    destination: ColorSpace,
    sensitivity: OverprintSensitivity,
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
        OverprintSensitivity::ProcessColorantExpansion,
    )
}

fn spot_conversion(id: &str) -> ObservedOverprintInteraction {
    interaction(
        id,
        ColorSpace::Separation,
        ColorSpace::DeviceCmyk,
        OverprintSensitivity::SpotColorantConversion,
    )
}

fn unsupported(id: &str) -> ObservedOverprintInteraction {
    interaction(
        id,
        ColorSpace::Pattern,
        ColorSpace::DeviceCmyk,
        OverprintSensitivity::Unsupported,
    )
}

fn safe(id: &str) -> ObservedOverprintInteraction {
    interaction(
        id,
        ColorSpace::DeviceGray,
        ColorSpace::DeviceCmyk,
        OverprintSensitivity::Safe,
    )
}

fn supported(
    interaction: ObservedOverprintInteraction,
    action: OverprintMitigationAction,
) -> OverprintMitigation {
    OverprintMitigation {
        interaction,
        action,
    }
}

fn skipped(interaction: ObservedOverprintInteraction) -> SkippedOverprintMitigation {
    SkippedOverprintMitigation {
        interaction,
        reason: OverprintSkipReason::UnsupportedInteraction,
    }
}

fn process_expansion_json(id: &'static str) -> Json {
    interaction_json(
        id,
        Json::string("device_cmyk"),
        Json::string("device_cmyk"),
        "process_colorant_expansion",
    )
}

fn spot_conversion_json(id: &'static str) -> Json {
    interaction_json(
        id,
        Json::string("separation"),
        Json::string("device_cmyk"),
        "spot_colorant_conversion",
    )
}

fn unsupported_json(id: &'static str) -> Json {
    interaction_json(
        id,
        Json::string("pattern"),
        Json::string("device_cmyk"),
        "unsupported",
    )
}

fn safe_json(id: &'static str) -> Json {
    interaction_json(
        id,
        Json::string("device_gray"),
        Json::string("device_cmyk"),
        "safe",
    )
}

fn interaction_json(
    id: &'static str,
    source: Json,
    destination: Json,
    sensitivity: &'static str,
) -> Json {
    Json::object([
        ("id", Json::string(id)),
        ("source", source),
        ("destination", destination),
        ("sensitivity", Json::string(sensitivity)),
    ])
}

// --- Overprint resolution tests ---------------------------------------------
//
// These cover every policy/observed-state combination of
// `resolve_overprint_policy`: `Preserve` regardless of observed state,
// `RejectUnsafe` with safe and unsafe observations, and `Mitigate` with all
// supported, all skipped, and mixed input where each partition preserves caller
// iteration order.

#[test]
fn preserve_leaves_as_is_regardless_of_observed_state() {
    assert_eq!(
        resolve_overprint_policy(OverprintPolicy::Preserve, []),
        OverprintDecision::Preserve,
    );
    assert_eq!(
        resolve_overprint_policy(
            OverprintPolicy::Preserve,
            [process_expansion("overprint-cmyk")],
        ),
        OverprintDecision::Preserve,
    );
}

#[test]
fn reject_unsafe_is_satisfied_when_no_unsafe_interaction_is_observed() {
    assert_eq!(
        resolve_overprint_policy(OverprintPolicy::RejectUnsafe, []),
        OverprintDecision::NoUnsafeOverprint,
    );
    assert_eq!(
        resolve_overprint_policy(OverprintPolicy::RejectUnsafe, [safe("safe-gray")]),
        OverprintDecision::NoUnsafeOverprint,
    );
}

#[test]
fn reject_unsafe_rejects_with_unsafe_interactions_in_caller_order() {
    assert_eq!(
        resolve_overprint_policy(
            OverprintPolicy::RejectUnsafe,
            [
                safe("safe-ignored"),
                process_expansion("first-unsafe"),
                spot_conversion("second-unsafe"),
                unsupported("third-unsafe"),
            ],
        ),
        OverprintDecision::Rejected {
            rejection: OverprintRejection::UnsafeOverprintSensitiveConversions {
                interactions: vec![
                    process_expansion("first-unsafe"),
                    spot_conversion("second-unsafe"),
                    unsupported("third-unsafe"),
                ],
            },
        },
    );
}

#[test]
fn mitigate_marks_all_supported_interactions_supported() {
    assert_eq!(
        resolve_overprint_policy(
            OverprintPolicy::Mitigate,
            [process_expansion("process"), spot_conversion("spot")],
        ),
        OverprintDecision::Mitigate {
            supported: vec![
                supported(
                    process_expansion("process"),
                    OverprintMitigationAction::PreserveZeroProcessColorants,
                ),
                supported(
                    spot_conversion("spot"),
                    OverprintMitigationAction::PreserveSpotOverprintAppearance,
                ),
            ],
            skipped: vec![],
        },
    );
}

#[test]
fn mitigate_skips_all_unsupported_interactions() {
    assert_eq!(
        resolve_overprint_policy(
            OverprintPolicy::Mitigate,
            [unsupported("first"), unsupported("second")],
        ),
        OverprintDecision::Mitigate {
            supported: vec![],
            skipped: vec![
                skipped(unsupported("first")),
                skipped(unsupported("second"))
            ],
        },
    );
}

#[test]
fn mitigate_preserves_caller_order_in_both_partitions() {
    assert_eq!(
        resolve_overprint_policy(
            OverprintPolicy::Mitigate,
            [
                safe("safe-ignored"),
                unsupported("first-skip"),
                process_expansion("first-supported"),
                unsupported("second-skip"),
                spot_conversion("second-supported"),
                process_expansion("third-supported"),
            ],
        ),
        OverprintDecision::Mitigate {
            supported: vec![
                supported(
                    process_expansion("first-supported"),
                    OverprintMitigationAction::PreserveZeroProcessColorants,
                ),
                supported(
                    spot_conversion("second-supported"),
                    OverprintMitigationAction::PreserveSpotOverprintAppearance,
                ),
                supported(
                    process_expansion("third-supported"),
                    OverprintMitigationAction::PreserveZeroProcessColorants,
                ),
            ],
            skipped: vec![
                skipped(unsupported("first-skip")),
                skipped(unsupported("second-skip")),
            ],
        },
    );
}

// --- Overprint shape tests ---------------------------------------------------
//
// These lock the public JSON encoding of every new overprint type. Each fixture
// asserts a full round-trip exactly as the current `#[serde(...)]` attributes
// emit it.

#[test]
fn observed_overprint_interaction_has_stable_json_shape() {
    assert_json_round_trip(
        &process_expansion("process"),
        process_expansion_json("process"),
    );
}

#[test]
fn overprint_sensitivity_variants_have_stable_json_shape() {
    assert_json_round_trip(&OverprintSensitivity::Safe, Json::string("safe"));
    assert_json_round_trip(
        &OverprintSensitivity::ProcessColorantExpansion,
        Json::string("process_colorant_expansion"),
    );
    assert_json_round_trip(
        &OverprintSensitivity::SpotColorantConversion,
        Json::string("spot_colorant_conversion"),
    );
    assert_json_round_trip(
        &OverprintSensitivity::Unsupported,
        Json::string("unsupported"),
    );
}

#[test]
fn overprint_mitigation_action_variants_have_stable_json_shape() {
    assert_json_round_trip(
        &OverprintMitigationAction::PreserveZeroProcessColorants,
        Json::string("preserve_zero_process_colorants"),
    );
    assert_json_round_trip(
        &OverprintMitigationAction::PreserveSpotOverprintAppearance,
        Json::string("preserve_spot_overprint_appearance"),
    );
}

#[test]
fn overprint_mitigation_has_stable_json_shape() {
    assert_json_round_trip(
        &supported(
            process_expansion("process"),
            OverprintMitigationAction::PreserveZeroProcessColorants,
        ),
        Json::object([
            ("interaction", process_expansion_json("process")),
            ("action", Json::string("preserve_zero_process_colorants")),
        ]),
    );
}

#[test]
fn overprint_rejection_has_stable_json_shape() {
    assert_json_round_trip(
        &OverprintRejection::UnsafeOverprintSensitiveConversions {
            interactions: vec![process_expansion("process"), unsupported("unsupported")],
        },
        Json::object([
            (
                "reason",
                Json::string("unsafe_overprint_sensitive_conversions"),
            ),
            (
                "interactions",
                Json::array([
                    process_expansion_json("process"),
                    unsupported_json("unsupported"),
                ]),
            ),
        ]),
    );
}

#[test]
fn overprint_skip_reason_has_stable_json_shape() {
    assert_json_round_trip(
        &OverprintSkipReason::UnsupportedInteraction,
        Json::object([("reason", Json::string("unsupported_interaction"))]),
    );
}

#[test]
fn skipped_overprint_mitigation_has_stable_json_shape() {
    assert_json_round_trip(
        &skipped(unsupported("unsupported")),
        Json::object([
            ("interaction", unsupported_json("unsupported")),
            (
                "reason",
                Json::object([("reason", Json::string("unsupported_interaction"))]),
            ),
        ]),
    );
}

#[test]
fn overprint_decision_unit_variants_have_stable_json_shape() {
    assert_json_round_trip(
        &OverprintDecision::Preserve,
        Json::object([("decision", Json::string("preserve"))]),
    );
    assert_json_round_trip(
        &OverprintDecision::NoUnsafeOverprint,
        Json::object([("decision", Json::string("no_unsafe_overprint"))]),
    );
}

#[test]
fn overprint_decision_rejected_has_stable_json_shape() {
    assert_json_round_trip(
        &OverprintDecision::Rejected {
            rejection: OverprintRejection::UnsafeOverprintSensitiveConversions {
                interactions: vec![process_expansion("process")],
            },
        },
        Json::object([
            ("decision", Json::string("rejected")),
            (
                "rejection",
                Json::object([
                    (
                        "reason",
                        Json::string("unsafe_overprint_sensitive_conversions"),
                    ),
                    (
                        "interactions",
                        Json::array([process_expansion_json("process")]),
                    ),
                ]),
            ),
        ]),
    );
}

#[test]
fn overprint_decision_mitigate_has_stable_json_shape() {
    assert_json_round_trip(
        &OverprintDecision::Mitigate {
            supported: vec![supported(
                spot_conversion("spot"),
                OverprintMitigationAction::PreserveSpotOverprintAppearance,
            )],
            skipped: vec![skipped(unsupported("unsupported"))],
        },
        Json::object([
            ("decision", Json::string("mitigate")),
            (
                "supported",
                Json::array([Json::object([
                    ("interaction", spot_conversion_json("spot")),
                    ("action", Json::string("preserve_spot_overprint_appearance")),
                ])]),
            ),
            (
                "skipped",
                Json::array([Json::object([
                    ("interaction", unsupported_json("unsupported")),
                    (
                        "reason",
                        Json::object([("reason", Json::string("unsupported_interaction"))]),
                    ),
                ])]),
            ),
        ]),
    );
}

#[test]
fn safe_interaction_json_fixture_is_usable_for_safe_variant() {
    assert_json_round_trip(&safe("safe"), safe_json("safe"));
}
