//! Conservative GenBank selected-strand views, not cohesive-duplex round trips.
use dnagent_domain::fragment_annotations::{AnnotatedDigest, FeatureMapping};
use dnagent_domain::{Strand, Topology};
use std::fmt::Write;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportStrand {
    Top,
    Bottom,
}

#[derive(Debug, Error)]
pub enum GenbankError {
    #[error("cannot export GenBank: {0}")]
    InvalidProjection(&'static str),
}

/// Export one record per fragment. Each mapped piece is a separate misc_feature;
/// no CDS translations, inferred joins or unsupported source qualifiers are emitted.
/// This serializes domain-produced projections; it does not certify their biology.
pub fn export(report: &AnnotatedDigest, selection: ExportStrand) -> Result<String, GenbankError> {
    if report.digest.fragments.len() != report.annotations.len() {
        return Err(GenbankError::InvalidProjection(
            "fragment/annotation count mismatch",
        ));
    }
    let mut out = String::new();
    for (index, (fragment, annotations)) in report
        .digest
        .fragments
        .iter()
        .zip(&report.annotations)
        .enumerate()
    {
        if fragment.id != annotations.fragment_id {
            return Err(GenbankError::InvalidProjection(
                "fragment identity mismatch",
            ));
        }
        let (side, sequence, mappings) = match selection {
            ExportStrand::Top => ("top", &fragment.top, &annotations.top),
            ExportStrand::Bottom => ("bottom", &fragment.bottom, &annotations.bottom),
        };
        if sequence.length != sequence.sequence_5to3.len()
            || sequence.length == 0
            || !sequence.sequence_5to3.bytes().all(|b| b"ACGT".contains(&b))
        {
            return Err(GenbankError::InvalidProjection("invalid strand sequence"));
        }
        let topology = match fragment.topology {
            Topology::Linear => "linear",
            Topology::Circular => "circular",
        };
        let locus = format!("frag{}_{side}", index + 1);
        writeln!(
            out,
            "LOCUS       {locus:<16} {:>11} bp    DNA     {topology:<8} UNK 01-JAN-1980",
            sequence.length
        )
        .expect("String write");
        writeln!(
            out,
            "DEFINITION  DNAagent selected {side} strand, fragment {}.",
            index + 1
        )
        .expect("String write");
        out.push_str("ACCESSION   .\nVERSION     .\nKEYWORDS    .\nSOURCE      .\n  ORGANISM  .\n            .\n");
        out.push_str("COMMENT     Selected 5-prime-to-3-prime strand view, NOT a duplex product.\n            End chemistry and other-strand geometry require the JSON report.\n            Each mapped piece is an independent misc_feature, not an inferred\n            biological join or validated CDS. Qualifiers, operators, primers,\n            modelled metadata and original locations remain in JSON.\n            Feature/part indices below are zero-based in that JSON report.\n            LOCUS date is a fixed export placeholder, not an experiment date.\n");
        writeln!(
            out,
            "            Source forward-axis interval start: {}.",
            sequence.source_start
        )
        .expect("String write");
        out.push_str("FEATURES             Location/Qualifiers\n");
        write_features(&mut out, report, mappings, sequence.length)?;
        out.push_str("ORIGIN\n");
        for (line, bases) in sequence.sequence_5to3.as_bytes().chunks(60).enumerate() {
            write!(out, "{:>9}", line * 60 + 1).expect("String write");
            for group in bases.chunks(10) {
                out.push(' ');
                for base in group {
                    out.push(char::from(base.to_ascii_lowercase()));
                }
            }
            out.push('\n');
        }
        out.push_str("//\n");
    }
    Ok(out)
}

fn write_features(
    out: &mut String,
    report: &AnnotatedDigest,
    mappings: &[FeatureMapping],
    length: usize,
) -> Result<(), GenbankError> {
    for mapping in mappings {
        let (source_index, source) = report
            .source_features
            .iter()
            .enumerate()
            .find(|(_, f)| f.id().as_str() == mapping.source_feature_id)
            .ok_or(GenbankError::InvalidProjection("unknown source feature"))?;
        for part in &mapping.parts {
            let start = part.fragment_region.start().get();
            let end = start
                .checked_add(part.fragment_region.length().get())
                .ok_or(GenbankError::InvalidProjection(
                    "mapped coordinates overflow",
                ))?;
            if part.fragment_region.is_circular_arc()
                || end > length
                || part.source_part >= source.location().parts().len()
            {
                return Err(GenbankError::InvalidProjection("invalid mapped part"));
            }
            let span = format!("{}..{end}", start + 1);
            let location = if mapping.fragment_strand == Strand::Reverse {
                format!("complement({span})")
            } else {
                span
            };
            writeln!(out, "     misc_feature    {location}").expect("String write");
            // Only safe short display labels enter native qualifier syntax.
            let label = source.label();
            if !label.is_empty()
                && label.len() <= 40
                && label.trim() == label
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b" ._-".contains(&b))
            {
                qualifier(out, "label", label);
            } else {
                qualifier(
                    out,
                    "label",
                    &format!("source_feature_{}", source_index + 1),
                );
                qualifier(out, "note", "original label retained in JSON");
            }
            qualifier(out, "note", &format!("source_feature_index={source_index}"));
            qualifier(
                out,
                "note",
                &format!(
                    "source_part={} source_offset={}",
                    part.source_part, part.source_offset
                ),
            );
            qualifier(
                out,
                "note",
                if mapping.complete {
                    "source base coverage=complete; function not inferred"
                } else {
                    "source base coverage=partial; clipped annotation"
                },
            );
            if mapping.split_source_parts.contains(&part.source_part) {
                qualifier(out, "note", "source part split across linear fragment ends");
            }
            if mapping.fragment_strand == Strand::Unknown {
                qualifier(
                    out,
                    "note",
                    "source strand unknown; no orientation inferred",
                );
            }
        }
    }
    Ok(())
}

fn qualifier(out: &mut String, key: &str, value: &str) {
    // All callers supply generated ASCII or a validated short label. Preserve
    // label whitespace exactly; generated note words may wrap at spaces.
    if key == "label" {
        writeln!(out, "                     /label=\"{value}\"").expect("String write");
        return;
    }
    write!(out, "                     /{key}=\"").expect("String write");
    let mut column = 21 + key.len() + 3;
    for (index, word) in value.split_whitespace().enumerate() {
        if index > 0 {
            if column + 1 + word.len() + 1 > 79 {
                out.push_str("\n                     ");
                column = 21;
            } else {
                out.push(' ');
                column += 1;
            }
        }
        out.push_str(word);
        column += word.len();
    }
    out.push_str("\"\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnagent_domain::fragment_annotations::annotated_digest;
    use dnagent_domain::{
        DisplayHints, DnaSeq, Feature, FeatureId, Location, LocationOperator, Qualifier, Region,
        SequenceRecord,
    };

    fn report(label: &str) -> AnnotatedDigest {
        let feature = Feature::new(
            FeatureId::new("f").unwrap(),
            "CDS",
            label,
            Location::new(
                vec![Region::linear(2, 10, 12).unwrap()],
                Strand::Reverse,
                LocationOperator::Contiguous,
            )
            .unwrap(),
            vec![Qualifier {
                key: "translation".into(),
                value: Some("DO_NOT_EXPORT".into()),
            }],
            DisplayHints::default(),
        );
        let record = SequenceRecord::new(
            "private label",
            DnaSeq::new("AAAGAATTCTTT").unwrap(),
            Topology::Linear,
            vec![feature],
            vec![],
        )
        .unwrap();
        annotated_digest(&record, &["EcoRI".into()]).unwrap()
    }

    #[test]
    fn selected_strands_preserve_orientation_without_stale_cds_claims() {
        let report = report("safe label");
        let top = export(&report, ExportStrand::Top).unwrap();
        let bottom = export(&report, ExportStrand::Bottom).unwrap();
        assert!(top.contains("complement(3..4)"));
        assert!(bottom.contains("misc_feature    1..6"));
        assert!(!top.contains("/translation="));
        assert!(!top.contains("DO_NOT_EXPORT"));
        assert!(top.contains("clipped annotation"));
        assert_eq!(
            top.lines()
                .filter(|line| line.starts_with("LOCUS "))
                .count(),
            2
        );
        assert_eq!(top, export(&report, ExportStrand::Top).unwrap());
    }

    #[test]
    fn hostile_or_unicode_labels_cannot_inject_genbank_syntax() {
        let text = export(&report("\"\n//\nLOCUS injected €"), ExportStrand::Top).unwrap();
        assert!(!text.contains("injected"));
        assert!(text.contains("original label retained in JSON"));
        assert_eq!(text.matches("\n//\n").count(), 2);
    }

    #[test]
    fn malformed_public_projections_fail_without_panicking() {
        let mut report = report("label");
        report.annotations[0].top[0].source_feature_id = "absent".into();
        assert!(export(&report, ExportStrand::Top).is_err());
        report.annotations[0].bottom[0].parts[0].fragment_region =
            Region::circular_arc(usize::MAX - 1, usize::MAX, usize::MAX).unwrap();
        assert!(export(&report, ExportStrand::Bottom).is_err());
        report.annotations.clear();
        assert!(export(&report, ExportStrand::Bottom).is_err());
    }
}
