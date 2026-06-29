use crate::{
    ClassicXrefObjectLocation, DictionaryValueKind, IndirectRef, PageTreeNodeType,
    PageTreeNodeTypeInspectionRejection, inspect_catalog_pages, inspect_classic_xref_table,
    inspect_classic_xref_trailer_root, inspect_page_tree_node_type, resolve_classic_xref_object,
};

#[test]
fn page_tree_node_type_classifies_intermediate_pages_node() {
    let source = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R 4 0 R ] /Count 2 >>\nendobj\n";

    let report =
        inspect_page_tree_node_type(source, 0).expect("page-tree node type should classify");

    assert_eq!(
        report.object_dictionary.reference,
        IndirectRef {
            object_number: 2,
            generation: 0,
        }
    );
    assert_eq!(
        &source[report.type_key_range.start..report.type_key_range.end],
        b"/Type"
    );
    assert_eq!(
        &source[report.type_value_range.start..report.type_value_range.end],
        b"/Pages"
    );
    assert_eq!(report.node_type, PageTreeNodeType::Pages);
}

#[test]
fn page_tree_node_type_classifies_leaf_page_object() {
    let source = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [ 0 0 612 792 ] >>\nendobj\n";

    let report =
        inspect_page_tree_node_type(source, 0).expect("page-tree node type should classify");

    assert_eq!(
        &source[report.type_value_range.start..report.type_value_range.end],
        b"/Page"
    );
    assert_eq!(report.node_type, PageTreeNodeType::Page);
}

#[test]
fn page_tree_node_type_classifies_other_name_value() {
    let source = b"5 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";

    let report =
        inspect_page_tree_node_type(source, 0).expect("page-tree node type should classify");

    assert_eq!(
        &source[report.type_value_range.start..report.type_value_range.end],
        b"/Catalog"
    );
    assert_eq!(report.node_type, PageTreeNodeType::Other);
}

#[test]
fn page_tree_node_type_does_not_decode_name_escapes() {
    // An escaped name that decodes to `/Page` must stay `Other` because the
    // classifier compares only the exact raw `/Type` value bytes.
    let source = b"3 0 obj\n<< /Type /Page#73 >>\nendobj\n";

    let report =
        inspect_page_tree_node_type(source, 0).expect("page-tree node type should classify");

    assert_eq!(
        &source[report.type_value_range.start..report.type_value_range.end],
        b"/Page#73"
    );
    assert_eq!(report.node_type, PageTreeNodeType::Other);
}

#[test]
fn page_tree_node_type_rejects_missing_type() {
    let source = b"2 0 obj\n<< /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";

    let error = inspect_page_tree_node_type(source, 0).expect_err("missing type should reject");

    assert_eq!(error.byte_offset, 0);
    assert_eq!(error.byte_len, source.len());
    assert_eq!(error.object_header_byte_offset, Some(0));
    assert_eq!(
        error.reason,
        PageTreeNodeTypeInspectionRejection::MissingType
    );

    let object_dictionary =
        crate::inspect_indirect_object_dictionary(source, 0).expect("object dictionary inspects");
    assert_eq!(
        error.error_byte_offset,
        Some(object_dictionary.dictionary_close_byte_offset)
    );
}

#[test]
fn page_tree_node_type_rejects_duplicate_type() {
    let source = b"2 0 obj\n<< /Type /Pages /Type /Page >>\nendobj\n";

    let error = inspect_page_tree_node_type(source, 0).expect_err("duplicate type should reject");

    assert!(matches!(
        error.reason,
        PageTreeNodeTypeInspectionRejection::DuplicateType { .. }
    ));
    if let PageTreeNodeTypeInspectionRejection::DuplicateType {
        first_key_range,
        duplicate_key_range,
    } = error.reason
    {
        assert_eq!(
            &source[first_key_range.start..first_key_range.end],
            b"/Type"
        );
        assert_eq!(
            &source[duplicate_key_range.start..duplicate_key_range.end],
            b"/Type"
        );
        assert!(first_key_range.start < duplicate_key_range.start);
        assert_eq!(error.error_byte_offset, Some(duplicate_key_range.start));
    }
}

#[test]
fn page_tree_node_type_rejects_non_name_type_values() {
    for (source, expected_kind) in [
        (
            b"2 0 obj\n<< /Type << /Nested /Pages >> >>\nendobj\n".as_slice(),
            DictionaryValueKind::Dictionary,
        ),
        (
            b"2 0 obj\n<< /Type [ /Pages ] >>\nendobj\n".as_slice(),
            DictionaryValueKind::Array,
        ),
        (
            b"2 0 obj\n<< /Type 1 >>\nendobj\n".as_slice(),
            DictionaryValueKind::NumberLike,
        ),
        (
            b"2 0 obj\n<< /Type 2 0 R >>\nendobj\n".as_slice(),
            DictionaryValueKind::IndirectReferenceLike,
        ),
    ] {
        let error =
            inspect_page_tree_node_type(source, 0).expect_err("non-name type should reject");

        assert_eq!(
            error.reason,
            PageTreeNodeTypeInspectionRejection::NonNameTypeValue {
                value_kind: expected_kind,
            }
        );
    }
}

#[test]
fn page_tree_node_type_report_does_not_retain_source_bytes() {
    let source =
        b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 /Secret (not-copied) >>\nendobj\n";

    let report =
        inspect_page_tree_node_type(source, 0).expect("page-tree node type should classify");

    let debug_report = format!("{report:?}");
    assert!(!debug_report.contains("Secret"));
    assert!(!debug_report.contains("not-copied"));
    assert!(!debug_report.contains("/Pages"));
    assert!(!debug_report.contains("/Type"));
}

#[test]
fn page_tree_node_type_composes_from_xref_catalog_and_page_tree_root() {
    let prefix = b"%PDF-1.7\n";
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
    let page_three = b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n";
    let catalog_offset = prefix.len();
    let pages_offset = prefix.len() + catalog.len();
    let page_three_offset = prefix.len() + catalog.len() + pages.len();
    let xref_offset = prefix.len() + catalog.len() + pages.len() + page_three.len();
    let source = format!(
        "{}{}{}{}xref\n0 4\n0000000000 65535 f \n{catalog_offset:010} 00000 n \n{pages_offset:010} 00000 n \n{page_three_offset:010} 00000 n \ntrailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        String::from_utf8_lossy(prefix),
        String::from_utf8_lossy(catalog),
        String::from_utf8_lossy(pages),
        String::from_utf8_lossy(page_three),
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

    let node_type_report = inspect_page_tree_node_type(&source, pages_offset)
        .expect("page-tree root type should classify");

    assert_eq!(
        node_type_report.object_dictionary.reference,
        IndirectRef {
            object_number: 2,
            generation: 0,
        }
    );
    assert_eq!(
        &source[node_type_report.type_value_range.start..node_type_report.type_value_range.end],
        b"/Pages"
    );
    assert_eq!(node_type_report.node_type, PageTreeNodeType::Pages);
}
