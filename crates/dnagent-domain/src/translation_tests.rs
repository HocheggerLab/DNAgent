//! Hand-derived expectations (not snapshots of the code under test).
use super::*;
use crate::{FeatureId, Location, LocationOperator, Qualifier, Region};

fn dna(s: &str) -> DnaSeq {
    DnaSeq::new(s).unwrap()
}

fn feature(parts: Vec<Region>, strand: Strand, qualifiers: &[(&str, &str)]) -> Feature {
    let operator = if parts.len() == 1 {
        LocationOperator::Contiguous
    } else {
        LocationOperator::Join
    };
    Feature::new(
        FeatureId::new("f1").unwrap(),
        "CDS",
        "test",
        Location::new(parts, strand, operator).unwrap(),
        qualifiers
            .iter()
            .map(|(k, v)| Qualifier {
                key: (*k).into(),
                value: Some((*v).into()),
            })
            .collect(),
        crate::DisplayHints::default(),
    )
}

fn positions(t: &Translation) -> Vec<[usize; 3]> {
    t.codons.iter().map(|c| c.positions).collect()
}

#[test]
fn tables_are_the_pinned_ncbi_set() {
    assert_eq!(GeneticCode::all().count(), 27);
    assert_eq!(GeneticCode::standard().id(), 1);
    assert_eq!(
        GeneticCode::ncbi(11).unwrap().name(),
        "Bacterial, Archaeal and Plant Plastid"
    );
    // Table 7 was merged into 4 and no longer exists.
    assert_eq!(
        GeneticCode::ncbi(7).unwrap_err(),
        TranslationError::UnknownTable(7)
    );
}

#[test]
fn ambiguous_codons_translate_only_when_all_expansions_agree() {
    let code = GeneticCode::standard();
    assert_eq!(code.translate_codon(*b"ATG"), 'M');
    assert_eq!(code.translate_codon(*b"CTN"), 'L');
    assert_eq!(code.translate_codon(*b"GCN"), 'A');
    assert_eq!(code.translate_codon(*b"ATH"), 'I');
    assert_eq!(code.translate_codon(*b"RAY"), 'X'); // AAC=N, GAC=D
    assert_eq!(code.translate_codon(*b"NNN"), 'X');
    assert!(code.is_stop(*b"TAR"));
    assert!(code.is_stop(*b"TRA")); // TAA, TGA
    assert!(!code.is_stop(*b"TNA")); // includes TTA=L
    // Vertebrate mitochondrial: TGA is Trp, AGA a stop.
    let mito = GeneticCode::ncbi(2).unwrap();
    assert_eq!(mito.translate_codon(*b"TGA"), 'W');
    assert!(mito.is_stop(*b"AGA"));
}

#[test]
fn starts_are_exact_and_policy_dependent() {
    let standard = GeneticCode::standard();
    let bacterial = GeneticCode::ncbi(11).unwrap();
    assert!(standard.is_start(*b"ATG", StartPolicy::AtgOnly));
    assert!(!bacterial.is_start(*b"GTG", StartPolicy::AtgOnly));
    assert!(bacterial.is_start(*b"GTG", StartPolicy::TableStarts));
    assert!(!standard.is_start(*b"GTG", StartPolicy::TableStarts));
    assert!(standard.is_start(*b"CTG", StartPolicy::TableStarts));
    assert!(!standard.is_start(*b"NTG", StartPolicy::TableStarts));
}

#[test]
fn ranges_translate_on_both_strands_with_coordinates() {
    let t = translate_range(
        &dna("ATGGCCTAA"),
        Topology::Linear,
        0,
        9,
        CodingStrand::Forward,
        0,
        1,
    )
    .unwrap();
    assert_eq!(t.protein, "MA*");
    assert!(t.terminal_stop && t.internal_stops.is_empty() && t.starts_with_atg);
    assert_eq!(positions(&t), vec![[0, 1, 2], [3, 4, 5], [6, 7, 8]]);

    let r = translate_range(
        &dna("TTAGGCCAT"),
        Topology::Linear,
        0,
        9,
        CodingStrand::Reverse,
        0,
        1,
    )
    .unwrap();
    assert_eq!(r.protein, "MA*");
    assert_eq!(r.codons[0].codon, "ATG");
    assert_eq!(positions(&r)[0], [8, 7, 6]);

    let framed = translate_range(
        &dna("AATGGCC"),
        Topology::Linear,
        0,
        7,
        CodingStrand::Forward,
        1,
        1,
    )
    .unwrap();
    assert_eq!(
        (
            framed.protein.as_str(),
            framed.trailing_bases,
            framed.codon_start
        ),
        ("MA", 0, 2)
    );
    let trailing = translate_range(
        &dna("ATGGCCA"),
        Topology::Linear,
        0,
        7,
        CodingStrand::Forward,
        0,
        1,
    )
    .unwrap();
    assert_eq!(trailing.trailing_bases, 1);
    let internal = translate_range(
        &dna("TAAATG"),
        Topology::Linear,
        0,
        6,
        CodingStrand::Forward,
        0,
        1,
    )
    .unwrap();
    assert_eq!(
        (internal.internal_stops.clone(), internal.terminal_stop),
        (vec![0], false)
    );
}

#[test]
fn range_errors_and_origin_wrap() {
    let circle = dna("GCCTAACCCATG");
    let wrapped = translate_range(
        &circle,
        Topology::Circular,
        9,
        6,
        CodingStrand::Forward,
        0,
        1,
    )
    .unwrap();
    assert_eq!(wrapped.protein, "MA*");
    assert_eq!(positions(&wrapped), vec![[9, 10, 11], [0, 1, 2], [3, 4, 5]]);
    assert!(matches!(
        translate_range(&circle, Topology::Linear, 9, 6, CodingStrand::Forward, 0, 1),
        Err(TranslationError::InvalidRange { .. })
    ));
    assert!(matches!(
        translate_range(
            &circle,
            Topology::Circular,
            3,
            3,
            CodingStrand::Forward,
            0,
            1
        ),
        Err(TranslationError::InvalidRange { .. })
    ));
    assert_eq!(
        translate_range(
            &circle,
            Topology::Circular,
            0,
            9,
            CodingStrand::Forward,
            3,
            1
        )
        .unwrap_err(),
        TranslationError::InvalidFrame(3)
    );
    assert_eq!(
        translate_range(
            &circle,
            Topology::Circular,
            0,
            2,
            CodingStrand::Forward,
            0,
            1
        )
        .unwrap_err(),
        TranslationError::TooShort
    );
    assert_eq!(
        translate_range(
            &circle,
            Topology::Circular,
            0,
            9,
            CodingStrand::Forward,
            0,
            99
        )
        .unwrap_err(),
        TranslationError::UnknownTable(99)
    );
}

#[test]
fn reverse_multipart_feature_is_the_reverse_complement_of_the_source_order_splice() {
    // Source-order splice TTACC + AGGCCAT = TTACCAGGCCAT; reverse complement ATGGCCTGGTAA.
    let sequence = dna("TTACCNNNAGGCCAT");
    let f = feature(
        vec![
            Region::linear(0, 5, 15).unwrap(),
            Region::linear(8, 15, 15).unwrap(),
        ],
        Strand::Reverse,
        &[],
    );
    let t = translate_feature(&sequence, &f, None).unwrap();
    assert_eq!(t.protein, "MAW*");
    assert_eq!(t.strand, CodingStrand::Reverse);
    // The third codon is split across the junction.
    assert_eq!(
        positions(&t),
        vec![[14, 13, 12], [11, 10, 9], [8, 4, 3], [2, 1, 0]]
    );
}

#[test]
fn origin_spanning_features_and_codon_start() {
    let circle = dna("GCCTAACCCATG");
    let f = feature(
        vec![Region::circular_arc(9, 9, 12).unwrap()],
        Strand::Forward,
        &[],
    );
    let t = translate_feature(&circle, &f, None).unwrap();
    assert_eq!(t.protein, "MA*");
    assert_eq!(positions(&t)[1], [0, 1, 2]);

    let shifted = feature(
        vec![Region::linear(0, 10, 10).unwrap()],
        Strand::Forward,
        &[("codon_start", "2")],
    );
    assert_eq!(
        translate_feature(&dna("CATGGCCTAA"), &shifted, None)
            .unwrap()
            .protein,
        "MA*"
    );
    let bad = feature(
        vec![Region::linear(0, 9, 9).unwrap()],
        Strand::Forward,
        &[("codon_start", "4")],
    );
    assert_eq!(
        translate_feature(&dna("ATGGCCTAA"), &bad, None).unwrap_err(),
        TranslationError::InvalidCodonStart("4".into())
    );
    let unknown = feature(vec![Region::linear(0, 9, 9).unwrap()], Strand::Unknown, &[]);
    assert!(matches!(
        translate_feature(&dna("ATGGCCTAA"), &unknown, None),
        Err(TranslationError::UnknownStrand(_))
    ));
}

#[test]
fn complete_cds_with_alternative_start_uses_initiator_methionine() {
    let gtg = feature(
        vec![Region::linear(0, 9, 9).unwrap()],
        Strand::Forward,
        &[("transl_table", "11")],
    );
    let t = translate_feature(&dna("GTGAAATAA"), &gtg, None).unwrap();
    assert_eq!(
        (t.protein.as_str(), t.initiator_as_methionine, t.table),
        ("MK*", true, 11)
    );
    // Not complete (no stop): translated literally.
    let open = feature(
        vec![Region::linear(0, 6, 6).unwrap()],
        Strand::Forward,
        &[("transl_table", "11")],
    );
    assert_eq!(
        translate_feature(&dna("GTGAAA"), &open, None)
            .unwrap()
            .protein,
        "VK"
    );
    // GTG is not a start in table 1; an explicit table overrides the qualifier.
    assert_eq!(
        translate_feature(&dna("GTGAAATAA"), &gtg, Some(1))
            .unwrap()
            .protein,
        "VK*"
    );
}

#[test]
fn imported_translations_are_compared_without_commas_or_terminal_stop() {
    let f = feature(
        vec![Region::linear(0, 9, 9).unwrap()],
        Strand::Forward,
        &[("translation", "M,A")],
    );
    let t = translate_feature(&dna("ATGGCCTAA"), &f, None).unwrap();
    let imported = imported_translation(&f).unwrap();
    assert_eq!(imported, "MA");
    assert_eq!(
        compare_imported(&t, &imported),
        ImportedComparison {
            imported_length: 2,
            matches: true,
            first_difference: None
        }
    );
    assert_eq!(compare_imported(&t, "MV").first_difference, Some(1));
    assert_eq!(compare_imported(&t, "M").first_difference, Some(1));
}

#[test]
fn orfs_are_complete_longest_per_stop_on_both_strands() {
    let scan = find_orfs(
        &dna("ATGAAATAA"),
        Topology::Linear,
        1,
        2,
        StartPolicy::AtgOnly,
    )
    .unwrap();
    assert_eq!(scan.orfs.len(), 1);
    let orf = &scan.orfs[0];
    assert_eq!(
        (
            orf.id.as_str(),
            orf.start,
            orf.length,
            orf.codons,
            orf.protein.as_str(),
            orf.frame
        ),
        ("orf-0001", 0, 9, 2, "MK", 0)
    );
    assert_eq!(
        (
            orf.start_codon.as_str(),
            orf.stop_codon.as_str(),
            orf.wraps_origin
        ),
        ("ATG", "TAA", false)
    );
    assert_eq!(
        find_orfs(
            &dna("ATGAAATAA"),
            Topology::Linear,
            1,
            3,
            StartPolicy::AtgOnly
        )
        .unwrap()
        .orfs,
        []
    );

    let nested = find_orfs(
        &dna("ATGATGAAATAA"),
        Topology::Linear,
        1,
        1,
        StartPolicy::AtgOnly,
    )
    .unwrap();
    assert_eq!(nested.orfs.len(), 1);
    assert_eq!(nested.orfs[0].protein, "MMK");

    let reverse = find_orfs(
        &dna("TTATTTCAT"),
        Topology::Linear,
        1,
        2,
        StartPolicy::AtgOnly,
    )
    .unwrap();
    assert_eq!(reverse.orfs.len(), 1);
    assert_eq!(
        (
            reverse.orfs[0].strand,
            reverse.orfs[0].start,
            reverse.orfs[0].length,
            reverse.orfs[0].frame
        ),
        (CodingStrand::Reverse, 0, 9, 0)
    );

    assert_eq!(
        find_orfs(
            &dna("ATGAAATAA"),
            Topology::Linear,
            1,
            0,
            StartPolicy::AtgOnly
        )
        .unwrap_err(),
        TranslationError::InvalidMinimum
    );
}

#[test]
fn circular_orfs_wrap_the_origin_once() {
    let sequence = dna("AAATAAGGGATG");
    let scan = find_orfs(&sequence, Topology::Circular, 1, 1, StartPolicy::AtgOnly).unwrap();
    assert_eq!(scan.orfs.len(), 1);
    let orf = &scan.orfs[0];
    assert_eq!(
        (
            orf.start,
            orf.length,
            orf.protein.as_str(),
            orf.wraps_origin
        ),
        (9, 9, "MK", true)
    );
    assert_eq!(
        find_orfs(&sequence, Topology::Linear, 1, 1, StartPolicy::AtgOnly)
            .unwrap()
            .orfs,
        []
    );
    // A frame with no stop at all is not an ORF.
    assert_eq!(
        find_orfs(
            &dna("ATGAAAAAAAAA"),
            Topology::Circular,
            1,
            1,
            StartPolicy::AtgOnly
        )
        .unwrap()
        .orfs,
        []
    );
}

#[test]
fn six_frames_follow_the_documented_coordinate_rule() {
    let frames = six_frames(&dna("ATGGCCTAA"), 1).unwrap();
    let proteins: Vec<&str> = frames.iter().map(|f| f.protein.as_str()).collect();
    assert_eq!(proteins, vec!["MA*", "WP", "GL", "LGH", "*A", "RP"]);
    assert_eq!(
        frames.iter().map(|f| f.first).collect::<Vec<_>>(),
        vec![0, 1, 2, 8, 7, 6]
    );
}
