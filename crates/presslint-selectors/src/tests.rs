#![allow(clippy::expect_used, clippy::missing_errors_doc)]

mod json;

use presslint_core::{
    ColorObservation, ColorSpace, ColorUsage, ContentScope, EditCapability, ObjectId, ObjectKind,
    PageIndex, Provenance,
};
use presslint_inventory::InventoryEntry;
use serde::{Deserialize, Serialize};

use self::json::{Json, JsonSerializer};
use super::{Predicate, Selector, matches};

fn assert_selector_json(selector: &Selector, expected_json: Json) {
    let encoded = selector
        .serialize(JsonSerializer)
        .expect("serialize selector");
    assert_eq!(encoded, expected_json);

    let decoded = Selector::deserialize(expected_json).expect("deserialize selector fixture");
    assert_eq!(&decoded, selector);
}

fn assert_predicate_json(predicate: &Predicate, expected_json: Json) {
    let encoded = predicate
        .serialize(JsonSerializer)
        .expect("serialize predicate");
    assert_eq!(encoded, expected_json);

    let decoded = Predicate::deserialize(expected_json).expect("deserialize predicate fixture");
    assert_eq!(&decoded, predicate);
}

fn form_xobject_scope(name: &[u8]) -> ContentScope {
    ContentScope::FormXObject {
        name: presslint_core::PdfName(name.to_vec()),
    }
}

fn pdf_name_json(name: &[u8]) -> Json {
    Json::array(name.iter().copied().map(u32::from).map(Json::U32))
}

#[test]
fn selector_boolean_variants_have_stable_json_shape() {
    assert_selector_json(&Selector::All, Json::object([("op", Json::string("all"))]));
    assert_selector_json(
        &Selector::None,
        Json::object([("op", Json::string("none"))]),
    );
    assert_selector_json(
        &Selector::Not {
            expr: Box::new(Selector::All),
        },
        Json::object([
            ("op", Json::string("not")),
            ("expr", Json::object([("op", Json::string("all"))])),
        ]),
    );
    assert_selector_json(
        &Selector::And {
            exprs: vec![Selector::All, Selector::None],
        },
        Json::object([
            ("op", Json::string("and")),
            (
                "exprs",
                Json::array([
                    Json::object([("op", Json::string("all"))]),
                    Json::object([("op", Json::string("none"))]),
                ]),
            ),
        ]),
    );
    assert_selector_json(
        &Selector::Or {
            exprs: vec![Selector::None, Selector::All],
        },
        Json::object([
            ("op", Json::string("or")),
            (
                "exprs",
                Json::array([
                    Json::object([("op", Json::string("none"))]),
                    Json::object([("op", Json::string("all"))]),
                ]),
            ),
        ]),
    );
}

#[test]
fn predicate_variants_have_stable_json_shape() {
    assert_predicate_json(
        &Predicate::ObjectKind {
            object_kind: ObjectKind::Vector,
        },
        Json::object([
            ("kind", Json::string("object_kind")),
            ("object_kind", Json::string("vector")),
        ]),
    );
    assert_predicate_json(
        &Predicate::ColorSpace {
            space: ColorSpace::DeviceCmyk,
        },
        Json::object([
            ("kind", Json::string("color_space")),
            ("space", Json::string("device_cmyk")),
        ]),
    );
    assert_predicate_json(
        &Predicate::Page { page: PageIndex(3) },
        Json::object([("kind", Json::string("page")), ("page", Json::U32(3))]),
    );
    assert_predicate_json(
        &Predicate::Editable {
            capability: EditCapability::RewriteColorOperand,
        },
        Json::object([
            ("kind", Json::string("editable")),
            ("capability", Json::string("rewrite_color_operand")),
        ]),
    );
}

#[test]
fn selector_predicate_fixtures_deserialize_to_expected_values() {
    assert_selector_json(
        &Selector::Predicate {
            predicate: Predicate::ObjectKind {
                object_kind: ObjectKind::Image,
            },
        },
        Json::object([
            ("op", Json::string("predicate")),
            (
                "predicate",
                Json::object([
                    ("kind", Json::string("object_kind")),
                    ("object_kind", Json::string("image")),
                ]),
            ),
        ]),
    );
    assert_selector_json(
        &Selector::Predicate {
            predicate: Predicate::ColorSpace {
                space: ColorSpace::IccBased,
            },
        },
        Json::object([
            ("op", Json::string("predicate")),
            (
                "predicate",
                Json::object([
                    ("kind", Json::string("color_space")),
                    ("space", Json::string("icc_based")),
                ]),
            ),
        ]),
    );
    assert_selector_json(
        &Selector::Predicate {
            predicate: Predicate::Page { page: PageIndex(0) },
        },
        Json::object([
            ("op", Json::string("predicate")),
            (
                "predicate",
                Json::object([("kind", Json::string("page")), ("page", Json::U32(0))]),
            ),
        ]),
    );
    assert_selector_json(
        &Selector::Predicate {
            predicate: Predicate::Editable {
                capability: EditCapability::AdjustStrokeWidth,
            },
        },
        Json::object([
            ("op", Json::string("predicate")),
            (
                "predicate",
                Json::object([
                    ("kind", Json::string("editable")),
                    ("capability", Json::string("adjust_stroke_width")),
                ]),
            ),
        ]),
    );
}

#[test]
fn scope_predicate_has_stable_json_shape() {
    assert_predicate_json(
        &Predicate::Scope {
            scope: ContentScope::Page,
        },
        Json::object([
            ("kind", Json::string("scope")),
            ("scope", Json::object([("kind", Json::string("page"))])),
        ]),
    );
    assert_predicate_json(
        &Predicate::Scope {
            scope: form_xobject_scope(b"Fm0"),
        },
        Json::object([
            ("kind", Json::string("scope")),
            (
                "scope",
                Json::object([
                    ("kind", Json::string("form_x_object")),
                    ("name", pdf_name_json(b"Fm0")),
                ]),
            ),
        ]),
    );
    assert_predicate_json(
        &Predicate::Scope {
            scope: ContentScope::AnnotationAppearance,
        },
        Json::object([
            ("kind", Json::string("scope")),
            (
                "scope",
                Json::object([("kind", Json::string("annotation_appearance"))]),
            ),
        ]),
    );
}

#[test]
fn color_usage_predicate_has_stable_json_shape() {
    assert_predicate_json(
        &Predicate::ColorUsage {
            usage: ColorUsage::Fill,
        },
        Json::object([
            ("kind", Json::string("color_usage")),
            ("usage", Json::string("fill")),
        ]),
    );
    assert_predicate_json(
        &Predicate::ColorUsage {
            usage: ColorUsage::Stroke,
        },
        Json::object([
            ("kind", Json::string("color_usage")),
            ("usage", Json::string("stroke")),
        ]),
    );
    assert_selector_json(
        &Selector::Predicate {
            predicate: Predicate::ColorUsage {
                usage: ColorUsage::Image,
            },
        },
        Json::object([
            ("op", Json::string("predicate")),
            (
                "predicate",
                Json::object([
                    ("kind", Json::string("color_usage")),
                    ("usage", Json::string("image")),
                ]),
            ),
        ]),
    );
}

fn color_observation(usage: ColorUsage) -> ColorObservation {
    ColorObservation {
        usage,
        space: ColorSpace::DeviceCmyk,
        components: Vec::new(),
        spot_name: None,
        source: None,
    }
}

fn inventory_entry(scope: ContentScope, colors: Vec<ColorObservation>) -> InventoryEntry {
    InventoryEntry {
        id: ObjectId {
            page: PageIndex(0),
            sequence: 0,
            digest: [0u8; 32],
        },
        kind: ObjectKind::Vector,
        provenance: Provenance {
            page: PageIndex(0),
            scope,
            range: None,
        },
        bounds: None,
        colors,
        capabilities: Vec::new(),
    }
}

fn entry_with_colors(colors: Vec<ColorObservation>) -> InventoryEntry {
    inventory_entry(ContentScope::Page, colors)
}

fn color_usage_selector(usage: ColorUsage) -> Selector {
    Selector::Predicate {
        predicate: Predicate::ColorUsage { usage },
    }
}

#[test]
fn color_usage_predicate_matches_single_matching_observation() {
    let entry = entry_with_colors(vec![color_observation(ColorUsage::Fill)]);
    assert!(matches(&color_usage_selector(ColorUsage::Fill), &entry));
}

#[test]
fn color_usage_predicate_does_not_match_without_usage() {
    let entry = entry_with_colors(vec![color_observation(ColorUsage::Fill)]);
    assert!(!matches(&color_usage_selector(ColorUsage::Stroke), &entry));
}

#[test]
fn color_usage_predicate_matches_one_of_multiple_observations() {
    let entry = entry_with_colors(vec![
        color_observation(ColorUsage::Fill),
        color_observation(ColorUsage::Stroke),
    ]);
    assert!(matches(&color_usage_selector(ColorUsage::Stroke), &entry));
}

#[test]
fn color_usage_predicate_does_not_match_entry_without_observations() {
    let entry = entry_with_colors(Vec::new());
    assert!(!matches(&color_usage_selector(ColorUsage::Fill), &entry));
}

fn entry_with_scope(scope: ContentScope) -> InventoryEntry {
    inventory_entry(scope, Vec::new())
}

fn scope_selector(scope: ContentScope) -> Selector {
    Selector::Predicate {
        predicate: Predicate::Scope { scope },
    }
}

#[test]
fn scope_predicate_matches_page_content_entry() {
    let entry = entry_with_scope(ContentScope::Page);
    assert!(matches(&scope_selector(ContentScope::Page), &entry));
}

#[test]
fn scope_predicate_matches_named_form_xobject_entry() {
    let entry = entry_with_scope(form_xobject_scope(b"Fm0"));
    assert!(matches(&scope_selector(form_xobject_scope(b"Fm0")), &entry));
}

#[test]
fn scope_predicate_does_not_match_different_form_name() {
    let entry = entry_with_scope(form_xobject_scope(b"Fm0"));
    assert!(!matches(
        &scope_selector(form_xobject_scope(b"Fm1")),
        &entry
    ));
}

#[test]
fn scope_predicate_does_not_match_across_scope_kind() {
    let entry = entry_with_scope(ContentScope::Page);
    assert!(!matches(
        &scope_selector(form_xobject_scope(b"Fm0")),
        &entry
    ));
}
