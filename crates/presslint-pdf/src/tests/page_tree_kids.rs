use crate::{
    ClassicXrefObjectLocation, IndirectRef, IndirectReferenceByteRange,
    IndirectReferenceInspectionRejection, PageTreeKidsInspectionRejection, SkippedPageTreeKidKind,
    inspect_catalog_pages, inspect_classic_xref_table, inspect_classic_xref_trailer_root,
    inspect_page_tree_kids, resolve_classic_xref_object,
};

#[test]
fn page_tree_kids_reports_multiple_direct_references_in_source_order() {
    let source = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R 4 2 R 5 0 R ] /Count 3 >>\nendobj\n";

    let report = inspect_page_tree_kids(source, 0).expect("kids should inspect");

    assert_eq!(
        report
            .kids
            .iter()
            .map(|kid| kid.reference)
            .collect::<Vec<_>>(),
        vec![
            IndirectRef {
                object_number: 3,
                generation: 0,
            },
            IndirectRef {
                object_number: 4,
                generation: 2,
            },
            IndirectRef {
                object_number: 5,
                generation: 0,
            },
        ]
    );
    assert_eq!(
        report.kids[0].reference_range,
        IndirectReferenceByteRange { start: 32, end: 37 }
    );
    assert!(report.skipped.is_empty());
    assert_eq!(
        &source[report.node.kids_value_range.start..report.node.kids_value_range.end],
        b"[ 3 0 R 4 2 R 5 0 R ]"
    );
}

#[test]
fn page_tree_kids_reports_empty_kids_array() {
    let source = b"2 0 obj\n<< /Kids [   % no children\n] /Count 0 >>\nendobj\n";

    let report = inspect_page_tree_kids(source, 0).expect("kids should inspect");

    assert!(report.kids.is_empty());
    assert!(report.skipped.is_empty());
}

#[test]
fn page_tree_kids_reports_malformed_reference_candidates_as_skips() {
    let source = b"2 0 obj\n<< /Kids [ 3 0 Robot 4294967296 0 R 4 70000 R ] /Count 3 >>\nendobj\n";

    let report = inspect_page_tree_kids(source, 0).expect("kids should inspect");

    assert!(report.kids.is_empty());
    assert_eq!(report.skipped.len(), 3);
    assert_eq!(
        report.skipped[0].kind,
        SkippedPageTreeKidKind::MalformedIndirectReference {
            reference_reason: IndirectReferenceInspectionRejection::MalformedReference,
        }
    );
    assert_eq!(
        &source[report.skipped[0].entry_range.start..report.skipped[0].entry_range.end],
        b"3 0 Robot"
    );
    assert_eq!(
        report.skipped[1].kind,
        SkippedPageTreeKidKind::MalformedIndirectReference {
            reference_reason: IndirectReferenceInspectionRejection::ObjectNumberOutOfRange,
        }
    );
    assert_eq!(
        report.skipped[2].kind,
        SkippedPageTreeKidKind::MalformedIndirectReference {
            reference_reason: IndirectReferenceInspectionRejection::GenerationOutOfRange,
        }
    );
}

#[test]
fn page_tree_kids_reports_unsupported_direct_non_reference_entries() {
    let source = b"2 0 obj\n<< /Kids [ /Name 12 true false null other (str) <abcd> << /K 9 >> ] /Count 0 >>\nendobj\n";

    let report = inspect_page_tree_kids(source, 0).expect("kids should inspect");

    assert!(report.kids.is_empty());
    assert_eq!(
        report
            .skipped
            .iter()
            .map(|skip| skip.kind)
            .collect::<Vec<_>>(),
        vec![
            SkippedPageTreeKidKind::Name,
            SkippedPageTreeKidKind::NumberLike,
            SkippedPageTreeKidKind::Boolean,
            SkippedPageTreeKidKind::Boolean,
            SkippedPageTreeKidKind::Null,
            SkippedPageTreeKidKind::OtherScalar,
            SkippedPageTreeKidKind::String,
            SkippedPageTreeKidKind::String,
            SkippedPageTreeKidKind::Dictionary,
        ]
    );
}

#[test]
fn page_tree_kids_does_not_descend_into_nested_entries() {
    let source = b"2 0 obj\n<< /Kids [ [ 3 0 R ] << /Kid 4 0 R >> (5 0 R) <3620302052> /7 8 0 R ] /Count 0 >>\nendobj\n";

    let report = inspect_page_tree_kids(source, 0).expect("kids should inspect");

    assert_eq!(report.kids.len(), 1);
    assert_eq!(
        report.kids[0].reference,
        IndirectRef {
            object_number: 8,
            generation: 0,
        }
    );
    assert_eq!(
        report
            .skipped
            .iter()
            .map(|skip| skip.kind)
            .collect::<Vec<_>>(),
        vec![
            SkippedPageTreeKidKind::Array,
            SkippedPageTreeKidKind::Dictionary,
            SkippedPageTreeKidKind::String,
            SkippedPageTreeKidKind::String,
            SkippedPageTreeKidKind::Name,
        ]
    );
}

#[test]
fn page_tree_kids_surfaces_delegated_node_errors() {
    let source = b"2 0 obj\n<< /Count 0 >>\nendobj\n";

    let error = inspect_page_tree_kids(source, 0).expect_err("missing kids should reject");

    assert!(matches!(
        error.reason,
        PageTreeKidsInspectionRejection::PageTreeNode { .. }
    ));
    assert_eq!(error.node_header_byte_offset, Some(0));
}

#[test]
fn page_tree_kids_report_does_not_retain_source_bytes() {
    let source = b"2 0 obj\n<< /Kids [ 3 0 R (secret-child) /SecretName ] /Count 1 >>\nendobj\n";

    let report = inspect_page_tree_kids(source, 0).expect("kids should inspect");

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("secret-child"));
    assert!(!debug_report.contains("SecretName"));
    assert!(!debug_report.contains("/Kids"));
    assert!(!debug_report.contains("/Count"));
}

#[test]
fn page_tree_kids_composes_from_xref_trailer_root_through_catalog_pages() {
    let prefix = b"%PDF-1.7\n";
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R 4 0 R ] /Count 2 >>\nendobj\n";
    let page_three = b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n";
    let page_four = b"4 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n";
    let catalog_offset = prefix.len();
    let pages_offset = prefix.len() + catalog.len();
    let page_three_offset = prefix.len() + catalog.len() + pages.len();
    let page_four_offset = prefix.len() + catalog.len() + pages.len() + page_three.len();
    let xref_offset =
        prefix.len() + catalog.len() + pages.len() + page_three.len() + page_four.len();
    let source = format!(
        "{}{}{}{}{}xref\n0 5\n0000000000 65535 f \n{catalog_offset:010} 00000 n \n{pages_offset:010} 00000 n \n{page_three_offset:010} 00000 n \n{page_four_offset:010} 00000 n \ntrailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        String::from_utf8_lossy(prefix),
        String::from_utf8_lossy(catalog),
        String::from_utf8_lossy(pages),
        String::from_utf8_lossy(page_three),
        String::from_utf8_lossy(page_four),
    )
    .into_bytes();

    let xref_report =
        inspect_classic_xref_table(&source, xref_offset).expect("xref should inspect");
    let root_report = inspect_classic_xref_trailer_root(&source, xref_report.trailer_byte_offset)
        .expect("root should inspect");
    let catalog_location =
        resolve_classic_xref_object(&xref_report, root_report.root_reference.object_number);
    assert_eq!(
        catalog_location,
        ClassicXrefObjectLocation::InUse {
            object_number: 1,
            generation: 0,
            byte_offset: catalog_offset,
        }
    );

    let catalog_report =
        inspect_catalog_pages(&source, catalog_offset).expect("catalog pages should inspect");
    let page_tree_location =
        resolve_classic_xref_object(&xref_report, catalog_report.pages_reference.object_number);
    assert_eq!(
        page_tree_location,
        ClassicXrefObjectLocation::InUse {
            object_number: 2,
            generation: 0,
            byte_offset: pages_offset,
        }
    );

    let kids_report =
        inspect_page_tree_kids(&source, pages_offset).expect("page tree kids should inspect");

    assert_eq!(
        kids_report
            .kids
            .iter()
            .map(|kid| kid.reference)
            .collect::<Vec<_>>(),
        vec![
            IndirectRef {
                object_number: 3,
                generation: 0,
            },
            IndirectRef {
                object_number: 4,
                generation: 0,
            },
        ]
    );
    assert!(kids_report.skipped.is_empty());
}
