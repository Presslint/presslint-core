#[path = "content_stream_extent/serde_harness.rs"]
#[allow(clippy::duplicate_mod)]
mod serde_harness;

use serde_harness::{from_serde_value, serde_value};

use crate::{
    ContentStreamDataExtentInspection, DocumentAccess, DocumentAccessBackend, DocumentAccessError,
    DocumentAccessRejection, DocumentPageContentExtentResult, IndirectRef, ObjectLookup,
    ObjectLookupLocation, PageContentExtentInspection, inspect_document_access,
    inspect_document_page_content_extents_with_lookup, locate_xref_object,
};

/// Push a fixed-width in-use classic xref entry line for `byte_offset`.
fn in_use_entry(source: &mut Vec<u8>, byte_offset: usize) {
    source.extend_from_slice(format!("{byte_offset:010} 00000 n \n").as_bytes());
}

/// Assemble a two-section incrementally-updated classic-xref document where
/// object 4 (`/Contents`) is present in both sections. The newest section
/// redefines object 4, so navigation through the classic `/Prev` chain must land
/// on the newer content stream. Returns the source and the newer content byte
/// offset.
fn two_section_classic_document() -> (Vec<u8>, usize) {
    let prefix = b"%PDF-1.7\n";
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
    let page = b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>\nendobj\n";
    let old_content = b"4 0 obj\n<< /Length 3 >>\nstream\nold\nendstream\nendobj\n";

    let catalog_offset = prefix.len();
    let pages_offset = catalog_offset + catalog.len();
    let page_offset = pages_offset + pages.len();
    let old_content_offset = page_offset + page.len();
    let old_xref_offset = old_content_offset + old_content.len();

    let mut source = prefix.to_vec();
    source.extend_from_slice(catalog);
    source.extend_from_slice(pages);
    source.extend_from_slice(page);
    source.extend_from_slice(old_content);

    source.extend_from_slice(b"xref\n0 5\n");
    source.extend_from_slice(b"0000000000 65535 f \n");
    in_use_entry(&mut source, catalog_offset);
    in_use_entry(&mut source, pages_offset);
    in_use_entry(&mut source, page_offset);
    in_use_entry(&mut source, old_content_offset);
    source.extend_from_slice(b"trailer\n<< /Size 5 /Root 1 0 R >>\n");

    let new_content_offset = source.len();
    source.extend_from_slice(b"4 0 obj\n<< /Length 3 >>\nstream\nnew\nendstream\nendobj\n");

    let new_xref_offset = source.len();
    source.extend_from_slice(b"xref\n4 1\n");
    in_use_entry(&mut source, new_content_offset);
    source.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R /Prev {old_xref_offset} >>\n").as_bytes(),
    );
    source.extend_from_slice(format!("startxref\n{new_xref_offset}\n%%EOF\n").as_bytes());

    (source, new_content_offset)
}

/// A single-section classic document whose trailer carries a `/Prev` pointing at
/// `prev_offset`, so navigation selects the classic-chain backend.
fn classic_document_with_prev(prev_offset: usize) -> Vec<u8> {
    let prefix = b"%PDF-1.7\n";
    let catalog = b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let pages = b"2 0 obj\n<< /Type /Pages /Kids [ 3 0 R ] /Count 1 >>\nendobj\n";
    let page = b"3 0 obj\n<< /Type /Page /Parent 2 0 R >>\nendobj\n";

    let catalog_offset = prefix.len();
    let pages_offset = catalog_offset + catalog.len();
    let page_offset = pages_offset + pages.len();

    let mut source = prefix.to_vec();
    source.extend_from_slice(catalog);
    source.extend_from_slice(pages);
    source.extend_from_slice(page);

    let xref_offset = source.len();
    source.extend_from_slice(b"xref\n0 4\n");
    source.extend_from_slice(b"0000000000 65535 f \n");
    in_use_entry(&mut source, catalog_offset);
    in_use_entry(&mut source, pages_offset);
    in_use_entry(&mut source, page_offset);
    source.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R /Prev {prev_offset} >>\n").as_bytes(),
    );
    source.extend_from_slice(format!("startxref\n{xref_offset}\n%%EOF\n").as_bytes());

    source
}

#[test]
fn neutral_spine_navigates_two_section_classic_chain_to_newest_content_object() {
    let (source, new_content_offset) = two_section_classic_document();

    let access = inspect_document_access(&source)
        .expect("neutral spine should navigate a two-section classic chain");

    assert!(matches!(
        access.backend,
        DocumentAccessBackend::ClassicXrefChain { .. }
    ));
    assert_eq!(
        access.root_reference,
        IndirectRef {
            object_number: 1,
            generation: 0
        }
    );
    assert_eq!(access.page_leaves.leaf_count(), 1);

    let DocumentAccessBackend::ClassicXrefChain { chain } = &access.backend else {
        unreachable!("backend shape already asserted");
    };
    assert_eq!(chain.section_byte_offsets.len(), 2);
    assert_eq!(
        locate_xref_object(ObjectLookup::ClassicXrefChain(chain), 4),
        ObjectLookupLocation::ClassicInUse {
            object_number: 4,
            generation: 0,
            byte_offset: new_content_offset,
        }
    );

    let extents = inspect_document_page_content_extents_with_lookup(
        &source,
        ObjectLookup::ClassicXrefChain(chain),
        access.page_tree_root.object_byte_offset,
    )
    .expect("page content extents should resolve through the classic chain");
    assert_eq!(extents.located_page_count(), 1);
    let DocumentPageContentExtentResult::Inspected { extents, .. } = &extents.pages[0].result
    else {
        unreachable!("the single page should have inspected extents");
    };
    let PageContentExtentInspection::Located {
        object_byte_offset,
        extent: ContentStreamDataExtentInspection::DirectLength(extent),
        ..
    } = &extents.entries[0]
    else {
        unreachable!("the single content stream should be located with a direct length");
    };
    assert_eq!(*object_byte_offset, new_content_offset);
    assert_eq!(
        &source[extent.stream_data_start_byte_offset..extent.stream_data_end_byte_offset],
        b"new"
    );
}

#[test]
fn neutral_spine_reports_classic_chain_build_failure() {
    let source = classic_document_with_prev(999_999);

    let error = inspect_document_access(&source)
        .expect_err("an out-of-bounds classic /Prev must fail the chain");

    assert!(matches!(
        error.reason,
        DocumentAccessRejection::ClassicXrefChain { .. }
    ));
}

#[test]
fn classic_chain_report_retains_no_source_bytes() {
    let (source, _) = two_section_classic_document();

    let access = inspect_document_access(&source).expect("classic chain spine should compose");
    let debug = format!("{access:?}");

    assert!(!debug.contains("old"));
    assert!(!debug.contains("new"));
}

#[test]
fn serde_round_trips_classic_chain_report() {
    let (source, _) = two_section_classic_document();
    let access = inspect_document_access(&source).expect("classic chain spine should compose");

    assert!(matches!(
        access.backend,
        DocumentAccessBackend::ClassicXrefChain { .. }
    ));
    let value = serde_value(&access).expect("classic chain report should serialize");
    let restored: DocumentAccess =
        from_serde_value(value).expect("classic chain report should deserialize");
    assert_eq!(restored, access);
}

#[test]
fn serde_round_trips_classic_chain_rejection() {
    let source = classic_document_with_prev(999_999);
    let error =
        inspect_document_access(&source).expect_err("out-of-bounds classic /Prev should reject");

    let value = serde_value(&error).expect("rejection should serialize");
    let restored: DocumentAccessError =
        from_serde_value(value).expect("rejection should deserialize");
    assert_eq!(restored, error);
}
