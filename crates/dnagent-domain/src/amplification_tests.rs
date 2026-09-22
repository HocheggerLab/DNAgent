use super::*;
use crate::DnaSeq;
use crate::primer_optimisation::Solution;

fn record(sequence: &str, topology: Topology) -> SequenceRecord {
    SequenceRecord::new(
        "synthetic",
        DnaSeq::new(sequence).unwrap(),
        topology,
        vec![],
        vec![],
    )
    .unwrap()
}
fn request() -> Request {
    Request {
        reference_input: 1,
        window_start: 0,
        window_length: 180,
        junction_offset: Some(60),
        junction_min_bases: 4,
        positive_inputs: vec![1],
        negative_inputs: vec![],
        min_product_length: 80,
        max_product_length: 140,
        screen_max_product_length: 1000,
        max_mismatches: 1,
        exact_three_prime_bases: 8,
        max_results: 3,
        constraints: PrimerConstraints {
            min_length: 20,
            max_length: 22,
            min_tm_c: 50.,
            max_tm_c: 75.,
            target_tm_c: 62.,
            max_pair_tm_difference_c: 3.,
            min_gc_fraction: 0.3,
            max_gc_fraction: 0.7,
            max_hairpin_stem: 5,
            max_dimer_run: 8,
            max_three_prime_run: 4,
            solution: Solution {
                sodium_mm: 50.,
                potassium_mm: 0.,
                tris_mm: 0.,
                magnesium_mm: 1.5,
                dntp_mm: 0.2,
                primer_nm: 250.,
            },
        },
    }
}
fn source() -> &'static str {
    include_str!("../../../fixtures/formats/fasta/primer_positive.fasta")
        .lines()
        .nth(1)
        .unwrap()
}
#[test]
fn designs_across_rotation_reverse_orientation_and_explicit_junction() {
    let s = source();
    let rotated = format!("{}{}", &s[70..], &s[..70]);
    let records = vec![
        record(s, Topology::Circular),
        record(&reverse_complement(&rotated), Topology::Circular),
    ];
    let mut r = request();
    r.window_start = 140;
    r.window_length = 180;
    r.junction_offset = Some(60);
    r.positive_inputs = vec![1, 2];
    let result = design(&records, &r).unwrap();
    assert!(!result.pairs.is_empty());
    for pair in result.pairs {
        let f = (pair.forward.reference_start + 180 - r.window_start) % 180;
        let rev = (pair.reverse.reference_start + 180 - r.window_start) % 180;
        assert!(
            (f + 4 <= 60 && 64 <= f + pair.forward.length)
                || (rev + 4 <= 60 && 64 <= rev + pair.reverse.length)
        );
        assert_eq!(pair.screens.len(), 2);
        assert!(
            pair.screens
                .iter()
                .all(|s| s.products.len() == 1 && s.products[0].mismatches == 0)
        );
    }
}
#[test]
fn identical_negative_prevents_a_design() {
    let records = vec![
        record(source(), Topology::Linear),
        record(source(), Topology::Linear),
    ];
    let mut r = request();
    r.negative_inputs = vec![2];
    assert!(matches!(design(&records, &r), Err(DesignError::NoPairs)));
}
#[test]
fn exact_three_prime_anchor_is_orientation_aware() {
    let oligo = "ACGTCAGTACCGATGCTAGC";
    for reverse in [false, true] {
        let motif = if reverse {
            reverse_complement(oligo)
        } else {
            oligo.into()
        };
        for anchor in [false, true] {
            let mut mutated = motif.as_bytes().to_vec();
            let index = if reverse == anchor { 0 } else { 19 };
            mutated[index] = if mutated[index] == b'A' { b'C' } else { b'A' };
            let sequence = String::from_utf8(mutated).unwrap();
            let found = sites(
                &record(&sequence, Topology::Linear),
                oligo,
                &request(),
                &mut 0,
            )
            .unwrap();
            assert_eq!(
                found
                    .iter()
                    .any(|s| s.reverse == reverse && s.mismatches == 1),
                !anchor
            );
        }
    }
}
#[test]
fn origin_spanning_binding_and_products_and_self_primer_products() {
    let oligo = "ACGTCAGTACCGATGCTAGC";
    let seq = format!("{}{}", &oligo[10..], &oligo[..10]);
    assert!(
        sites(&record(&seq, Topology::Circular), oligo, &request(), &mut 0)
            .unwrap()
            .iter()
            .any(|s| s.start == 10 && !s.reverse && s.mismatches == 0)
    );
    assert!(
        sites(&record(&seq, Topology::Linear), oligo, &request(), &mut 0)
            .unwrap()
            .is_empty()
    );
    let binding = vec![
        Site {
            primer: 1,
            start: 90,
            length: 5,
            reverse: false,
            mismatches: 0,
        },
        Site {
            primer: 1,
            start: 10,
            length: 5,
            reverse: true,
            mismatches: 0,
        },
    ];
    let result = products(
        &record(&"A".repeat(100), Topology::Circular),
        &binding,
        50,
        &mut 0,
    )
    .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].length, 25);
    assert_eq!(result[0].plus_primer, result[0].minus_primer);
    assert!(
        products(
            &record(&"A".repeat(100), Topology::Linear),
            &binding,
            50,
            &mut 0
        )
        .unwrap()
        .is_empty()
    );
}
#[test]
fn invalid_roles_ambiguous_templates_and_extreme_junction_are_rejected() {
    let records = vec![record(source(), Topology::Linear)];
    let mut r = request();
    r.junction_offset = Some(usize::MAX);
    assert!(r.validate(&records).is_err());
    r = request();
    r.positive_inputs = vec![1, 1];
    assert!(r.validate(&records).is_err());
    r = request();
    let ambiguous = vec![record(&"N".repeat(180), Topology::Linear)];
    assert!(r.validate(&ambiguous).is_err());
    let mut budget = 200_000_000;
    assert!(matches!(
        sites(&records[0], &source()[..20], &request(), &mut budget),
        Err(DesignError::Budget)
    ));
}
