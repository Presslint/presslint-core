use super::{
    IndirectObjectEditDisposition, IndirectObjectOwnership, IndirectRef,
    decide_indirect_object_edit,
};

fn indirect_ref(object_number: u32, generation: u16) -> IndirectRef {
    IndirectRef {
        object_number,
        generation,
    }
}

#[test]
fn one_proven_consumer_allows_in_place_mutation() {
    let target = indirect_ref(10, 0);
    let owner = indirect_ref(2, 0);

    let decision = decide_indirect_object_edit(target, [owner]);

    assert_eq!(decision.target, target);
    assert_eq!(
        decision.ownership,
        IndirectObjectOwnership::ProvenSingleUse { owner }
    );
    assert_eq!(
        decision.disposition,
        IndirectObjectEditDisposition::InPlaceMutation
    );
}

#[test]
fn multiple_proven_consumers_require_private_copy() {
    let target = indirect_ref(10, 0);
    let first = indirect_ref(2, 0);
    let second = indirect_ref(3, 0);

    let decision = decide_indirect_object_edit(target, [first, second]);

    assert_eq!(
        decision.ownership,
        IndirectObjectOwnership::Shared {
            consumers: vec![first, second],
        }
    );
    assert_eq!(
        decision.disposition,
        IndirectObjectEditDisposition::PrivateCopy
    );
}

#[test]
fn no_proven_consumers_require_private_copy() {
    let target = indirect_ref(10, 0);

    let decision = decide_indirect_object_edit(target, []);

    assert_eq!(decision.ownership, IndirectObjectOwnership::Unproven);
    assert_eq!(
        decision.disposition,
        IndirectObjectEditDisposition::PrivateCopy
    );
}

#[test]
fn shared_consumer_refs_are_reported_deterministically() {
    let target = indirect_ref(10, 0);
    let high_generation = indirect_ref(2, 1);
    let lowest = indirect_ref(1, 0);
    let low_generation = indirect_ref(2, 0);

    let decision =
        decide_indirect_object_edit(target, [high_generation, lowest, low_generation, lowest]);

    assert_eq!(
        decision.ownership,
        IndirectObjectOwnership::Shared {
            consumers: vec![lowest, low_generation, high_generation],
        }
    );
}
