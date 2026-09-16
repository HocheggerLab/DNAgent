//! Source-linked annotation projections onto each digest strand's 5′→3′ frame.
use crate::digest::{Digest, DigestError, DigestFragment, FragmentStrand, simulate_digest};
use crate::{DomainError, Feature, ImportedPrimer, Region, SequenceRecord, Strand, Topology};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnnotationError {
    #[error(transparent)]
    Digest(#[from] DigestError),
    #[error(transparent)]
    Coordinates(#[from] DomainError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MappedPart {
    /// Index into the original feature's ordered location parts, zero-based.
    pub source_part: usize,
    /// Offset within that original part, in its forward-coordinate traversal.
    pub source_offset: usize,
    pub source_region: Region,
    /// Coordinates on THIS exported strand's 5′→3′ sequence, not a duplex axis.
    pub fragment_region: Region,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureMapping {
    pub source_feature_id: String,
    pub fragment_strand: Strand,
    pub source_bases: usize,
    pub retained_bases: usize,
    /// Coverage only, NOT a statement that a CDS remains functional or in frame.
    pub complete: bool,
    /// Original contiguous parts split across opposite ends of a linear product.
    pub split_source_parts: Vec<usize>,
    /// Original part order, then offset within each part; never genomic re-sorting.
    pub parts: Vec<MappedPart>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FragmentAnnotations {
    pub fragment_id: String,
    pub top: Vec<FeatureMapping>,
    pub bottom: Vec<FeatureMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnnotatedDigest {
    pub source_name: String,
    /// Unmodified originals, including qualifiers, operators and display hints.
    pub source_features: Vec<Feature>,
    /// Retained only as source metadata; binding sites are not inferred.
    pub source_primers: Vec<ImportedPrimer>,
    pub digest: Digest,
    pub annotations: Vec<FragmentAnnotations>,
    pub assumptions: Vec<&'static str>,
}

pub fn annotated_digest(
    record: &SequenceRecord,
    names: &[String],
) -> Result<AnnotatedDigest, AnnotationError> {
    let digest = simulate_digest(record.sequence(), record.topology(), names)?;
    let annotations = digest
        .fragments
        .iter()
        .map(|fragment| {
            Ok(FragmentAnnotations {
                fragment_id: fragment.id.clone(),
                top: map_features(record, fragment, false)?,
                bottom: map_features(record, fragment, true)?,
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    Ok(AnnotatedDigest {
        source_name: record.name().to_owned(),
        source_features: record.features().to_vec(),
        source_primers: record.primers().to_vec(),
        digest,
        annotations,
        assumptions: vec![
            "Annotations projected separately onto each strand's 5-prime-to-3-prime sequence; bottom projection reverses coordinates and feature strand",
            "complete means base coverage only; split_source_parts flags original contiguous parts broken across linear fragment ends",
            "Source feature operators, part order and qualifiers are retained without translation, frame, splicing or biological-function inference",
            "Source qualifiers may no longer apply to clipped products; they are provenance, not new product annotations",
            "Source primers retained without binding-site inference; opaque import metadata is not exported",
            "FASTA is sequence-only: use the JSON report for annotations, source coordinates and duplex end geometry",
        ],
    })
}

fn map_features(
    record: &SequenceRecord,
    fragment: &DigestFragment,
    bottom: bool,
) -> Result<Vec<FeatureMapping>, DomainError> {
    let strand = if bottom {
        &fragment.bottom
    } else {
        &fragment.top
    };
    record
        .features()
        .iter()
        .filter_map(|feature| {
            match map_feature(
                feature,
                strand,
                fragment.topology,
                record.sequence().len(),
                bottom,
            ) {
                Ok(mapping) if mapping.parts.is_empty() => None,
                result => Some(result),
            }
        })
        .collect()
}

fn map_feature(
    feature: &Feature,
    strand: &FragmentStrand,
    topology: Topology,
    n: usize,
    bottom: bool,
) -> Result<FeatureMapping, DomainError> {
    let mut parts = Vec::new();
    let mut split_source_parts = Vec::new();
    let mut source_bases = 0;
    let begin = strand.source_start as i128;
    let finish = begin + strand.length as i128;
    for (index, region) in feature.location().parts().iter().enumerate() {
        source_bases += region.length().get();
        let mut pieces = Vec::new();
        // Both source and fragment spans are <= n; these three lifts cover all
        // possible intersections, including origin-spanning source annotations.
        for shift in [-(n as i128), 0, n as i128] {
            let start = region.start().get() as i128 + shift;
            let end = start + region.length().get() as i128;
            let lo = start.max(begin);
            let hi = end.min(finish);
            if lo >= hi {
                continue;
            }
            let source_start = lo.rem_euclid(n as i128) as usize;
            let length = (hi - lo) as usize;
            let source_region = if source_start + length <= n {
                Region::linear(source_start, source_start + length, n)?
            } else {
                Region::circular_arc(source_start, length, n)?
            };
            let (a, b) = if bottom {
                (finish - hi, finish - lo)
            } else {
                (lo - begin, hi - begin)
            };
            pieces.push(MappedPart {
                source_part: index,
                source_offset: (lo - start) as usize,
                source_region,
                fragment_region: Region::linear(a as usize, b as usize, strand.length)?,
            });
        }
        pieces.sort_by_key(|p| p.source_offset);
        if topology == Topology::Linear && pieces.len() > 1 {
            split_source_parts.push(index);
        }
        parts.extend(pieces);
    }
    let retained_bases = parts.iter().map(|p| p.fragment_region.length().get()).sum();
    let fragment_strand = match (feature.location().strand(), bottom) {
        (Strand::Forward, true) => Strand::Reverse,
        (Strand::Reverse, true) => Strand::Forward,
        (strand, _) => strand,
    };
    Ok(FeatureMapping {
        source_feature_id: feature.id().as_str().to_owned(),
        fragment_strand,
        source_bases,
        retained_bases,
        complete: retained_bases == source_bases,
        split_source_parts,
        parts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DisplayHints, DnaSeq, FeatureId, Location, LocationOperator, Qualifier};
    fn record(
        seq: &str,
        topology: Topology,
        parts: Vec<Region>,
        strand: Strand,
        operator: LocationOperator,
    ) -> SequenceRecord {
        SequenceRecord::new(
            "synthetic",
            DnaSeq::new(seq).unwrap(),
            topology,
            vec![Feature::new(
                FeatureId::new("f").unwrap(),
                "CDS",
                "synthetic feature",
                Location::new(parts, strand, operator).unwrap(),
                vec![Qualifier {
                    key: "note".into(),
                    value: None,
                }],
                DisplayHints::default(),
            )],
            vec![],
        )
        .unwrap()
    }
    #[test]
    fn staggered_cuts_project_distinct_strands_and_preserve_provenance() {
        let r = record(
            "AAAGAATTCTTT",
            Topology::Linear,
            vec![Region::linear(2, 10, 12).unwrap()],
            Strand::Forward,
            LocationOperator::Contiguous,
        );
        let d = annotated_digest(&r, &["EcoRI".into()]).unwrap();
        assert_eq!(d.source_features, r.features());
        assert_eq!(d.annotations[0].top[0].retained_bases, 2);
        assert_eq!(d.annotations[0].bottom[0].retained_bases, 6);
        assert!(!d.annotations[0].top[0].complete);
        assert_eq!(d.annotations[0].bottom[0].fragment_strand, Strand::Reverse);
        assert_eq!(
            d.annotations[0].bottom[0].parts[0].fragment_region,
            Region::linear(0, 6, 8).unwrap()
        );
    }
    #[test]
    fn complete_coverage_does_not_hide_a_feature_split_at_linear_ends() {
        let r = record(
            "AAAGAATTCTTT",
            Topology::Circular,
            vec![Region::linear(2, 10, 12).unwrap()],
            Strand::Reverse,
            LocationOperator::Contiguous,
        );
        let d = annotated_digest(&r, &["EcoRI".into()]).unwrap();
        let mapping = &d.annotations[0].top[0];
        assert!(mapping.complete);
        assert_eq!(mapping.split_source_parts, [0]);
        assert_eq!(
            mapping
                .parts
                .iter()
                .map(|p| p.source_offset)
                .collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(d.annotations[0].bottom[0].fragment_strand, Strand::Forward);
    }
    #[test]
    fn order_operator_and_unknown_strand_are_preserved() {
        let r = record(
            "AAAGAATTCTTT",
            Topology::Linear,
            vec![
                Region::linear(8, 11, 12).unwrap(),
                Region::linear(1, 3, 12).unwrap(),
            ],
            Strand::Unknown,
            LocationOperator::Order,
        );
        let d = annotated_digest(&r, &["BamHI".into()]).unwrap();
        assert_eq!(
            d.source_features[0].location().operator(),
            LocationOperator::Order
        );
        assert_eq!(d.annotations[0].bottom[0].fragment_strand, Strand::Unknown);
        assert_eq!(
            d.annotations[0].top[0]
                .parts
                .iter()
                .map(|p| p.source_part)
                .collect::<Vec<_>>(),
            [0, 1]
        );
    }
    #[test]
    fn origin_spanning_uncut_circle_is_not_flagged_as_disrupted() {
        let r = record(
            "AAAGAATTCTTT",
            Topology::Circular,
            vec![Region::circular_arc(10, 4, 12).unwrap()],
            Strand::Forward,
            LocationOperator::Contiguous,
        );
        let d = annotated_digest(&r, &["BamHI".into()]).unwrap();
        assert!(d.annotations[0].top[0].complete);
        assert!(d.annotations[0].top[0].split_source_parts.is_empty());
        assert_eq!(d.annotations[0].top[0].parts.len(), 2);
    }
}
