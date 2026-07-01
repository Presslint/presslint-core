#[path = "content_stream_extent/serde_harness.rs"]
#[allow(clippy::duplicate_mod)]
mod serde_harness;

use std::fmt::Write as _;

use serde_harness::{from_serde_value, serde_value};

use crate::{
    ClassicXrefChain, ClassicXrefChainError, ClassicXrefChainRejection, ClassicXrefEntry,
    ClassicXrefEntryState, IndirectRef, ObjectLookup, ObjectLookupLocation,
    build_classic_xref_chain, locate_xref_object,
};

/// One fixed-width classic xref entry line: `(byte_offset, generation, kind)`
/// where `kind` is `'n'` (in-use) or `'f'` (free).
type EntryLine = (usize, u16, char);

/// Render a classic xref section: `xref`, the `first count` subsections and their
/// fixed-width entry lines, then a `trailer` dictionary.
fn section(subsections: &[(u32, &[EntryLine])], trailer: &str) -> String {
    let mut out = String::from("xref\n");
    for (first, entries) in subsections {
        let _ = writeln!(out, "{first} {}", entries.len());
        for (offset, generation, kind) in *entries {
            let _ = writeln!(out, "{offset:010} {generation:05} {kind} ");
        }
    }
    let _ = write!(out, "trailer\n{trailer}\n");
    out
}

fn entry(
    object_number: u32,
    generation: u16,
    byte_offset: usize,
    state: ClassicXrefEntryState,
) -> ClassicXrefEntry {
    ClassicXrefEntry {
        object_number,
        generation,
        byte_offset,
        state,
    }
}

#[test]
fn merges_two_section_chain_newest_wins_and_free_shadows_old_in_use() {
    let mut source = b"%PDF-1.7\n".to_vec();

    let older_offset = source.len();
    source.extend_from_slice(
        section(
            &[(
                0,
                &[(0, 65535, 'f'), (10, 0, 'n'), (20, 0, 'n'), (30, 0, 'n')],
            )],
            "<< /Size 4 /Root 1 0 R >>",
        )
        .as_bytes(),
    );

    let newer_offset = source.len();
    // Newest section redefines object 1 (in-use at 99) and shadows the older
    // in-use object 2 with a free entry (generation 7); object 3 is untouched.
    source.extend_from_slice(
        section(
            &[(0, &[(0, 65535, 'f'), (99, 0, 'n'), (0, 7, 'f')])],
            &format!("<< /Size 4 /Root 1 0 R /Prev {older_offset} >>"),
        )
        .as_bytes(),
    );

    let chain = build_classic_xref_chain(&source, newer_offset)
        .expect("two-section classic chain should build");

    assert_eq!(chain.startxref_byte_offset, newer_offset);
    assert_eq!(chain.section_byte_offsets, vec![newer_offset, older_offset]);
    assert_eq!(
        chain.root_reference,
        IndirectRef {
            object_number: 1,
            generation: 0
        }
    );
    assert_eq!(chain.effective_size, 4);
    assert_eq!(
        chain.entries,
        vec![
            entry(0, 65535, 0, ClassicXrefEntryState::Free),
            entry(1, 0, 99, ClassicXrefEntryState::InUse),
            entry(2, 7, 0, ClassicXrefEntryState::Free),
            entry(3, 0, 30, ClassicXrefEntryState::InUse),
        ]
    );

    assert_eq!(
        locate_xref_object(ObjectLookup::ClassicXrefChain(&chain), 1),
        ObjectLookupLocation::ClassicInUse {
            object_number: 1,
            generation: 0,
            byte_offset: 99,
        }
    );
    assert_eq!(
        locate_xref_object(ObjectLookup::ClassicXrefChain(&chain), 2),
        ObjectLookupLocation::ClassicFree {
            object_number: 2,
            generation: 7,
            next_free_object_number: 0,
        }
    );
    assert_eq!(
        locate_xref_object(ObjectLookup::ClassicXrefChain(&chain), 3),
        ObjectLookupLocation::ClassicInUse {
            object_number: 3,
            generation: 0,
            byte_offset: 30,
        }
    );
    assert_eq!(
        locate_xref_object(ObjectLookup::ClassicXrefChain(&chain), 9),
        ObjectLookupLocation::ClassicNotFound { object_number: 9 }
    );
}

#[test]
fn keeps_first_entry_for_intra_section_duplicate_object_number() {
    let mut source = b"%PDF-1.7\n".to_vec();
    let offset = source.len();
    source.extend_from_slice(
        section(
            &[
                (0, &[(0, 65535, 'f'), (11, 0, 'n')]),
                // Duplicate subsection for object 1 later in the same section.
                (1, &[(22, 0, 'n')]),
            ],
            "<< /Size 2 /Root 1 0 R >>",
        )
        .as_bytes(),
    );

    let chain =
        build_classic_xref_chain(&source, offset).expect("single-section chain should build");

    assert_eq!(
        chain.entries,
        vec![
            entry(0, 65535, 0, ClassicXrefEntryState::Free),
            entry(1, 0, 11, ClassicXrefEntryState::InUse),
        ]
    );
}

#[test]
fn detects_prev_cycle_without_looping() {
    let mut source = b"%PDF-1.7\n".to_vec();
    let offset = source.len();
    source.extend_from_slice(
        section(
            &[(0, &[(0, 65535, 'f')])],
            &format!("<< /Size 1 /Root 1 0 R /Prev {offset} >>"),
        )
        .as_bytes(),
    );

    let error = build_classic_xref_chain(&source, offset)
        .expect_err("self-referential /Prev should be a cycle");

    assert_eq!(
        error.reason,
        ClassicXrefChainRejection::Cycle {
            byte_offset: offset
        }
    );
}

#[test]
fn rejects_out_of_bounds_prev_offset() {
    let mut source = b"%PDF-1.7\n".to_vec();
    let offset = source.len();
    source.extend_from_slice(
        section(
            &[(0, &[(0, 65535, 'f')])],
            "<< /Size 1 /Root 1 0 R /Prev 999999 >>",
        )
        .as_bytes(),
    );

    let error =
        build_classic_xref_chain(&source, offset).expect_err("out-of-bounds /Prev should reject");

    assert_eq!(
        error.reason,
        ClassicXrefChainRejection::OffsetOutOfBounds {
            byte_offset: 999_999,
        }
    );
}

#[test]
fn stops_over_long_chain_at_section_bound() {
    let mut source = b"%PDF-1.7\n".to_vec();
    let mut previous: Option<usize> = None;
    let mut newest = 0;
    for _ in 0..65 {
        newest = source.len();
        let trailer = previous.map_or_else(
            || "<< /Size 1 /Root 1 0 R >>".to_string(),
            |prev| format!("<< /Size 1 /Root 1 0 R /Prev {prev} >>"),
        );
        source.extend_from_slice(section(&[(0, &[(0, 65535, 'f')])], &trailer).as_bytes());
        previous = Some(newest);
    }

    let error = build_classic_xref_chain(&source, newest)
        .expect_err("65 sections should exceed the chain bound");

    assert_eq!(
        error.reason,
        ClassicXrefChainRejection::SectionLimitExceeded { max_sections: 64 }
    );
}

#[test]
fn rejects_mixed_xref_stream_prev_target() {
    let mut source = b"%PDF-1.7\n".to_vec();
    let stream_offset = source.len();
    source.extend_from_slice(
        b"10 0 obj\n<< /Type /XRef /Size 1 /W [ 1 1 1 ] /Root 1 0 R /Length 3 >>\nstream\n\x00\x00\x00\nendstream\nendobj\n",
    );
    let table_offset = source.len();
    source.extend_from_slice(
        section(
            &[(0, &[(0, 65535, 'f')])],
            &format!("<< /Size 1 /Root 1 0 R /Prev {stream_offset} >>"),
        )
        .as_bytes(),
    );

    let error = build_classic_xref_chain(&source, table_offset)
        .expect_err("xref-stream /Prev target should reject as mixed type");

    assert_eq!(
        error.reason,
        ClassicXrefChainRejection::PrevSectionNotClassicXref {
            byte_offset: stream_offset,
        }
    );
}

#[test]
fn report_retains_no_source_bytes() {
    let mut source = b"%PDF-1.7\n".to_vec();
    let offset = source.len();
    source.extend_from_slice(
        section(
            &[(0, &[(0, 65535, 'f')])],
            "<< /Size 1 /Root 1 0 R /Secret (do-not-copy) >>",
        )
        .as_bytes(),
    );

    let chain = build_classic_xref_chain(&source, offset).expect("chain should build");
    let debug = format!("{chain:?}");
    assert!(!debug.contains("Secret"));
    assert!(!debug.contains("do-not-copy"));
}

#[test]
fn serde_round_trips_chain_report_and_error_shapes() {
    let mut source = b"%PDF-1.7\n".to_vec();
    let offset = source.len();
    source.extend_from_slice(
        section(
            &[(0, &[(0, 65535, 'f'), (10, 0, 'n')])],
            "<< /Size 2 /Root 1 0 R >>",
        )
        .as_bytes(),
    );
    let chain = build_classic_xref_chain(&source, offset).expect("chain should build");

    let value = serde_value(&chain).expect("chain report should serialize");
    let restored: ClassicXrefChain = from_serde_value(value).expect("chain should deserialize");
    assert_eq!(restored, chain);

    let cycle_source = {
        let mut cycle = b"%PDF-1.7\n".to_vec();
        let cycle_offset = cycle.len();
        cycle.extend_from_slice(
            section(
                &[(0, &[(0, 65535, 'f')])],
                &format!("<< /Size 1 /Root 1 0 R /Prev {cycle_offset} >>"),
            )
            .as_bytes(),
        );
        cycle
    };
    let error = build_classic_xref_chain(&cycle_source, 9).expect_err("cycle should reject");
    let value = serde_value(&error).expect("chain error should serialize");
    let restored: ClassicXrefChainError =
        from_serde_value(value).expect("chain error should deserialize");
    assert_eq!(restored, error);
}
