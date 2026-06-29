use crate::{
    ArrayExtentInspectionRejection, ClassicXrefObjectLocation, DictionaryEntryInspectionRejection,
    DictionaryValueKind, IndirectObjectDictionaryInspectionRejection, IndirectRef,
    PageTreeNodeInspectionRejection, inspect_catalog_pages, inspect_classic_xref_table,
    inspect_classic_xref_trailer_root, inspect_page_tree_node, resolve_classic_xref_object,
};

#[test]
fn page_tree_node_reports_kids_extent_and_count_span() {
    let source = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R 4 0 R ] /Count 2 >>\nendobj\n";

    let report = inspect_page_tree_node(source, 0).expect("page tree node should inspect");

    assert_eq!(
        report.node_dictionary.reference,
        IndirectRef {
            object_number: 2,
            generation: 0,
        }
    );
    assert_eq!(
        &source[report.kids_key_range.start..report.kids_key_range.end],
        b"/Kids"
    );
    assert_eq!(
        &source[report.kids_value_range.start..report.kids_value_range.end],
        b"[ 3 0 R 4 0 R ]"
    );
    assert_eq!(report.kids_array_extent.max_observed_depth, 1);
    assert_eq!(
        report.kids_array_extent.open_byte_offset,
        report.kids_value_range.start
    );
    assert_eq!(
        report.kids_array_extent.after_close_byte_offset,
        report.kids_value_range.end
    );
    assert_eq!(
        &source[report.count_key_range.start..report.count_key_range.end],
        b"/Count"
    );
    assert_eq!(
        &source[report.count_value_range.start..report.count_value_range.end],
        b"2"
    );
}

#[test]
fn page_tree_node_skips_leading_whitespace_before_node_header() {
    let source = b"\t \r\n2 0 obj << /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";

    let report = inspect_page_tree_node(source, 0).expect("page tree node should inspect");

    assert_eq!(report.node_dictionary.header_range.start, 4);
    assert_eq!(
        &source[report.kids_value_range.start..report.kids_value_range.end],
        b"[ 3 0 R ]"
    );
    assert_eq!(
        &source[report.count_value_range.start..report.count_value_range.end],
        b"1"
    );
}

#[test]
fn page_tree_node_reports_nested_kids_array_with_balanced_extent_and_depth() {
    let source = b"2 0 obj\n<< /Kids [ [ 3 0 R ] [ 4 0 R ] ] /Count 2 >>\nendobj\n";

    let report = inspect_page_tree_node(source, 0).expect("page tree node should inspect");

    assert_eq!(
        &source[report.kids_value_range.start..report.kids_value_range.end],
        b"[ [ 3 0 R ] [ 4 0 R ] ]"
    );
    assert_eq!(report.kids_array_extent.max_observed_depth, 2);
    assert_eq!(
        report.kids_array_extent.after_close_byte_offset,
        report.kids_value_range.end
    );
}

#[test]
fn page_tree_node_rejects_missing_kids() {
    let source = b"2 0 obj\n<< /Type /Pages /Count 1 >>\nendobj\n";

    let error = inspect_page_tree_node(source, 0).expect_err("missing kids should reject");

    assert_eq!(error.byte_offset, 0);
    assert_eq!(error.byte_len, source.len());
    assert_eq!(error.node_header_byte_offset, Some(0));
    assert_eq!(error.reason, PageTreeNodeInspectionRejection::MissingKids);

    let node_dictionary =
        crate::inspect_indirect_object_dictionary(source, 0).expect("node dictionary inspects");
    assert_eq!(
        error.error_byte_offset,
        Some(node_dictionary.dictionary_close_byte_offset)
    );
}

#[test]
fn page_tree_node_rejects_duplicate_kids() {
    let source = b"2 0 obj\n<< /Kids [ 3 0 R ] /Kids [ 4 0 R ] /Count 1 >>\nendobj\n";

    let error = inspect_page_tree_node(source, 0).expect_err("duplicate kids should reject");

    assert!(matches!(
        error.reason,
        PageTreeNodeInspectionRejection::DuplicateKids { .. }
    ));
    if let PageTreeNodeInspectionRejection::DuplicateKids {
        first_key_range,
        duplicate_key_range,
    } = error.reason
    {
        assert_eq!(
            &source[first_key_range.start..first_key_range.end],
            b"/Kids"
        );
        assert_eq!(
            &source[duplicate_key_range.start..duplicate_key_range.end],
            b"/Kids"
        );
        assert!(first_key_range.start < duplicate_key_range.start);
        assert_eq!(error.error_byte_offset, Some(duplicate_key_range.start));
    }
}

#[test]
fn page_tree_node_rejects_direct_dictionary_name_number_and_reference_kids_values() {
    for (source, expected_kind) in [
        (
            b"2 0 obj\n<< /Kids << /Type /Pages >> /Count 1 >>\nendobj\n".as_slice(),
            DictionaryValueKind::Dictionary,
        ),
        (
            b"2 0 obj\n<< /Kids /Pages /Count 1 >>\nendobj\n".as_slice(),
            DictionaryValueKind::Name,
        ),
        (
            b"2 0 obj\n<< /Kids 3 /Count 1 >>\nendobj\n".as_slice(),
            DictionaryValueKind::NumberLike,
        ),
        (
            b"2 0 obj\n<< /Kids 3 0 R /Count 1 >>\nendobj\n".as_slice(),
            DictionaryValueKind::IndirectReferenceLike,
        ),
    ] {
        let error = inspect_page_tree_node(source, 0).expect_err("non-array kids should reject");

        assert_eq!(
            error.reason,
            PageTreeNodeInspectionRejection::NonArrayKidsValue {
                value_kind: expected_kind,
            }
        );
    }
}

#[test]
fn page_tree_node_surfaces_unterminated_kids_array_as_array_extent_rejection() {
    // `inspect_indirect_object_dictionary` already bounds array values through
    // `inspect_dictionary_entries`, so an unterminated `/Kids` array fails during
    // that delegated step and surfaces the array-extent rejection reason through
    // the object-dictionary channel rather than the standalone bounding call.
    let source = b"2 0 obj\n<< /Kids [ 3 0 R /Count 2 >>\nendobj\n";

    let error = inspect_page_tree_node(source, 0).expect_err("unterminated kids should reject");

    assert_eq!(
        error.reason,
        PageTreeNodeInspectionRejection::NodeDictionary {
            node_dictionary_reason:
                IndirectObjectDictionaryInspectionRejection::DictionaryEntries {
                    dictionary_entries_reason: DictionaryEntryInspectionRejection::ArrayExtent {
                        array_reason: ArrayExtentInspectionRejection::UnterminatedArray,
                    },
                },
        }
    );
}

#[test]
fn page_tree_node_rejects_missing_count() {
    let source = b"2 0 obj\n<< /Kids [ 3 0 R ] >>\nendobj\n";

    let error = inspect_page_tree_node(source, 0).expect_err("missing count should reject");

    assert_eq!(error.node_header_byte_offset, Some(0));
    assert_eq!(error.reason, PageTreeNodeInspectionRejection::MissingCount);
}

#[test]
fn page_tree_node_rejects_duplicate_count() {
    let source = b"2 0 obj\n<< /Kids [ 3 0 R ] /Count 1 /Count 2 >>\nendobj\n";

    let error = inspect_page_tree_node(source, 0).expect_err("duplicate count should reject");

    assert!(matches!(
        error.reason,
        PageTreeNodeInspectionRejection::DuplicateCount { .. }
    ));
    if let PageTreeNodeInspectionRejection::DuplicateCount {
        first_key_range,
        duplicate_key_range,
    } = error.reason
    {
        assert_eq!(
            &source[first_key_range.start..first_key_range.end],
            b"/Count"
        );
        assert_eq!(
            &source[duplicate_key_range.start..duplicate_key_range.end],
            b"/Count"
        );
        assert!(first_key_range.start < duplicate_key_range.start);
        assert_eq!(error.error_byte_offset, Some(duplicate_key_range.start));
    }
}

#[test]
fn page_tree_node_rejects_non_number_count_values() {
    for (source, expected_kind) in [
        (
            b"2 0 obj\n<< /Kids [ 3 0 R ] /Count /Two >>\nendobj\n".as_slice(),
            DictionaryValueKind::Name,
        ),
        (
            b"2 0 obj\n<< /Kids [ 3 0 R ] /Count [ 1 ] >>\nendobj\n".as_slice(),
            DictionaryValueKind::Array,
        ),
        (
            b"2 0 obj\n<< /Kids [ 3 0 R ] /Count 5 0 R >>\nendobj\n".as_slice(),
            DictionaryValueKind::IndirectReferenceLike,
        ),
    ] {
        let error = inspect_page_tree_node(source, 0).expect_err("non-number count should reject");

        assert_eq!(
            error.reason,
            PageTreeNodeInspectionRejection::NonNumberCountValue {
                value_kind: expected_kind,
            }
        );
    }
}

#[test]
fn page_tree_node_report_does_not_retain_source_bytes() {
    let source =
        b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 /Secret (not-copied) >>\nendobj\n";

    let report = inspect_page_tree_node(source, 0).expect("page tree node should inspect");

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("Secret"));
    assert!(!debug_report.contains("not-copied"));
    assert!(!debug_report.contains("/Pages"));
    assert!(!debug_report.contains("/Kids"));
    assert!(!debug_report.contains("/Count"));
}

#[test]
fn page_tree_node_composes_from_xref_trailer_root_through_catalog_pages() {
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

    let node_report =
        inspect_page_tree_node(&source, pages_offset).expect("page tree node should inspect");

    assert_eq!(
        node_report.node_dictionary.reference,
        IndirectRef {
            object_number: 2,
            generation: 0,
        }
    );
    assert_eq!(
        &source[node_report.kids_value_range.start..node_report.kids_value_range.end],
        b"[ 3 0 R 4 0 R ]"
    );
    assert_eq!(node_report.kids_array_extent.max_observed_depth, 1);
    assert_eq!(
        &source[node_report.count_value_range.start..node_report.count_value_range.end],
        b"2"
    );
}
