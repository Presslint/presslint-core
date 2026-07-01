#![allow(clippy::expect_used)]

use presslint_pdf::{
    DocumentAccessBackend, ObjectLookup, inspect_document_access,
    inspect_document_page_content_extents_with_lookup,
    inspect_document_page_xobject_resources_with_lookup,
};
use presslint_types::PageIndex;

use crate::document_inventory::inventory_names;
use crate::{
    ColorSpace, ContentScope, FormExpandedInventory, FormWalkContext, ObjectKind, PdfInventorySkip,
    PdfName, SkippedFormInventoryReason, build_classic_pdf_inventory,
    build_page_inventory_with_forms, build_pdf_inventory,
};

const MAX: usize = 4096;

/// Build a classic-xref PDF from object bodies numbered `1..=objects.len()`.
fn classic_pdf(objects: &[&[u8]]) -> Vec<u8> {
    let mut source = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::with_capacity(objects.len());
    for object in objects {
        offsets.push(source.len());
        source.extend_from_slice(object);
    }

    let xref_offset = source.len();
    let object_count = objects.len() + 1;
    source.extend_from_slice(format!("xref\n0 {object_count}\n").as_bytes());
    source.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        source.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    source.extend_from_slice(
        format!(
            "trailer\n<< /Size {object_count} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
        )
        .as_bytes(),
    );
    source
}

/// Build one `N 0 obj` stream object whose `/Length` matches `data` exactly.
fn stream_object(number: u32, dict_extra: &str, data: &[u8]) -> Vec<u8> {
    let mut object = format!(
        "{number} 0 obj\n<< /Length {}{} >>\nstream\n",
        data.len(),
        dict_extra
    )
    .into_bytes();
    object.extend_from_slice(data);
    object.extend_from_slice(b"\nendstream\nendobj\n");
    object
}

const CATALOG: &[u8] = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
const PAGES: &[u8] = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
const PAGE_WITH_FORM: &[u8] = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Fm 4 0 R >> >> /Contents 5 0 R >>\nendobj\n";

/// Single page that invokes form `/Fm` (object 4), whose own content is `form`.
fn page_with_form_pdf(page_content: &[u8], form_dict_extra: &str, form: &[u8]) -> Vec<u8> {
    let form_object = stream_object(4, form_dict_extra, form);
    let page_content_object = stream_object(5, "", page_content);
    classic_pdf(&[
        CATALOG,
        PAGES,
        PAGE_WITH_FORM,
        &form_object,
        &page_content_object,
    ])
}

/// Run the neutral document pipeline and expand the first page's forms directly,
/// exposing the per-form skip diagnostics that the report bridges do not surface.
fn expand_first_page(source: &[u8]) -> FormExpandedInventory {
    let access = inspect_document_access(source).expect("document access");
    let lookup = match &access.backend {
        DocumentAccessBackend::ClassicXref { xref_table, .. } => {
            ObjectLookup::ClassicXref(xref_table)
        }
        DocumentAccessBackend::ClassicXrefChain { chain } => ObjectLookup::ClassicXrefChain(chain),
        DocumentAccessBackend::XrefStreamSection { section } => {
            ObjectLookup::XrefStreamSection(section)
        }
        DocumentAccessBackend::XrefStreamChain { chain } => ObjectLookup::XrefStreamChain(chain),
    };
    let root = access.page_tree_root.object_byte_offset;
    let extents = inspect_document_page_content_extents_with_lookup(source, lookup, root)
        .expect("page content extents");
    let resources = inspect_document_page_xobject_resources_with_lookup(source, lookup, root)
        .expect("page xobject resources");
    let page = &extents.pages[0];
    let page_resources = &resources.pages[0];
    let image_names = inventory_names(&page_resources.image_xobject_names);
    let form_names = inventory_names(&page_resources.form_xobject_names);
    build_page_inventory_with_forms(
        source,
        lookup,
        page,
        PageIndex(0),
        MAX,
        &image_names,
        &form_names,
        &page_resources.form_xobjects,
        FormWalkContext::one_level(),
    )
    .expect("first page inventory")
}

#[test]
fn rgb_inside_page_level_form_surfaces_as_form_scope_marking_entry() {
    let source = page_with_form_pdf(
        b"q\n/Fm Do\nQ",
        " /Type /XObject /Subtype /Form /BBox [ 0 0 100 100 ]",
        b"1 0 0 rg\n0 0 50 50 re\nf",
    );

    let report = build_pdf_inventory(&source, MAX).expect("inventory should build");

    // Page-level form invocation entry, then the form's own content entry.
    assert_eq!(report.inventory.len(), 2);
    let invocation = &report.inventory.entries[0];
    assert_eq!(invocation.kind, ObjectKind::FormXObject);
    assert_eq!(invocation.provenance.scope, ContentScope::Page);

    let form_marking = &report.inventory.entries[1];
    assert_eq!(form_marking.kind, ObjectKind::Vector);
    assert_eq!(
        form_marking.provenance.scope,
        ContentScope::FormXObject {
            name: PdfName(b"Fm".to_vec()),
        }
    );
    assert!(
        form_marking
            .colors
            .iter()
            .any(|color| color.space == ColorSpace::DeviceRgb)
    );
}

#[test]
fn form_entries_carry_invoking_page_index_and_page_global_sequence() {
    let source = page_with_form_pdf(
        b"q\n/Fm Do\nQ",
        " /Type /XObject /Subtype /Form /BBox [ 0 0 100 100 ]",
        b"1 0 0 rg\n0 0 50 50 re\nf",
    );

    let report = build_pdf_inventory(&source, MAX).expect("inventory should build");

    let invocation = &report.inventory.entries[0];
    let form_marking = &report.inventory.entries[1];
    // Nested entry is stamped with the ORIGINAL invoking page index.
    assert_eq!(form_marking.id.page, PageIndex(0));
    assert_eq!(form_marking.provenance.page, PageIndex(0));
    // Sequence is page-global and continues after the page space; it never
    // restarts at 0.
    assert_eq!(invocation.id.sequence, 0);
    assert_eq!(form_marking.id.sequence, 1);
    assert!(form_marking.id.sequence > invocation.id.sequence);
}

#[test]
fn self_referential_form_is_a_skip_not_a_page_failure() {
    // Object 4 is a form whose own `/Resources /XObject /Fm` points back at
    // itself and whose content re-invokes `/Fm`.
    let form_object = stream_object(
        4,
        " /Type /XObject /Subtype /Form /BBox [ 0 0 100 100 ] /Resources << /XObject << /Fm 4 0 R >> >>",
        b"1 0 0 rg\n0 0 50 50 re\nf\n/Fm Do",
    );
    let page_content = stream_object(5, "", b"q\n/Fm Do\nQ");
    let source = classic_pdf(&[CATALOG, PAGES, PAGE_WITH_FORM, &form_object, &page_content]);

    let expanded = expand_first_page(&source);

    // The page's own invocation entry plus the form's own content survive.
    assert!(!expanded.inventory.is_empty());
    assert!(
        expanded
            .inventory
            .entries
            .iter()
            .any(|entry| entry.provenance.scope == ContentScope::Page)
    );
    // The re-invocation is reported as a cycle, not descended into forever.
    assert_eq!(expanded.form_skipped.len(), 1);
    assert_eq!(expanded.form_skipped[0].name, PdfName(b"Fm".to_vec()));
    assert_eq!(
        expanded.form_skipped[0].reason,
        SkippedFormInventoryReason::Cycle
    );
}

#[test]
fn unsupported_filter_form_is_a_skip_with_page_inventory_intact() {
    // The page paints its own CMYK vector, then invokes a form whose stream uses
    // a filter this bridge does not decode.
    let source = page_with_form_pdf(
        b"q\n0 0 0 1 k\n0 0 10 10 re\nf\n/Fm Do\nQ",
        " /Type /XObject /Subtype /Form /BBox [ 0 0 100 100 ] /Filter /ASCIIHexDecode",
        b"00",
    );

    let expanded = expand_first_page(&source);

    // Page's own vector inventory is still produced.
    assert!(
        expanded
            .inventory
            .entries
            .iter()
            .any(|entry| entry.kind == ObjectKind::Vector
                && entry.provenance.scope == ContentScope::Page)
    );
    assert_eq!(expanded.form_skipped.len(), 1);
    assert!(matches!(
        expanded.form_skipped[0].reason,
        SkippedFormInventoryReason::Content {
            skip: PdfInventorySkip::UnsupportedFilter { .. }
        }
    ));
}

#[test]
fn classic_bridge_expands_page_level_form_content() {
    let source = page_with_form_pdf(
        b"q\n/Fm Do\nQ",
        " /Type /XObject /Subtype /Form /BBox [ 0 0 100 100 ]",
        b"1 0 0 rg\n0 0 50 50 re\nf",
    );

    let report = build_classic_pdf_inventory(&source, MAX).expect("classic inventory should build");

    assert!(report.inventory.entries.iter().any(|entry| {
        entry.kind == ObjectKind::Vector
            && entry.provenance.scope
                == ContentScope::FormXObject {
                    name: PdfName(b"Fm".to_vec()),
                }
            && entry
                .colors
                .iter()
                .any(|c| c.space == ColorSpace::DeviceRgb)
    }));
}

#[test]
fn page_without_form_invocations_is_unchanged() {
    let source = classic_pdf(&[
        CATALOG,
        PAGES,
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n",
        &stream_object(4, "", b"q\n0 0 1 rg\n12 12 80 80 re\nf\nQ"),
    ]);

    let report = build_pdf_inventory(&source, MAX).expect("inventory should build");

    // Exactly the page-only vector entry, in `Page` scope, sequence 0: no form
    // machinery altered the page-only output.
    assert_eq!(report.inventory.len(), 1);
    let entry = &report.inventory.entries[0];
    assert_eq!(entry.kind, ObjectKind::Vector);
    assert_eq!(entry.provenance.scope, ContentScope::Page);
    assert_eq!(entry.id.sequence, 0);
    assert_eq!(entry.id.page, PageIndex(0));
}
