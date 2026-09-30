//! The predicted Gibson product as an annotated record (for DNAgent GenBank): source
//! features carried over from the engine's core projections, primers at their binding
//! sites and junction overlaps. Features cut by a core boundary are left out with a warning.
use crate::AppError;
use dnagent_domain::feature_match::reverse_complement;
use dnagent_domain::fragment_annotations::FeatureMapping;
use dnagent_domain::gibson::{GibsonReport, PrimerCandidate};
use dnagent_domain::{
    DisplayHints, DnaSeq, Feature, FeatureId, ImportedPrimer, Location, LocationOperator,
    Qualifier, Region, SequenceRecord, Strand, Topology,
};
use dnagent_formats::{FormatExtensions, ImportReport, ImportWarning};

/// A feature before ids are assigned: sorted by first part start, then source order.
struct Pending {
    start: usize,
    kind: String,
    label: String,
    location: Location,
    qualifiers: Vec<Qualifier>,
    color: Option<String>,
}

fn region(start: usize, length: usize, total: usize, circular: bool) -> Result<Region, AppError> {
    let start = start % total;
    Ok(if start + length > total {
        if !circular {
            return Err(AppError::InvalidRange {
                start,
                end: start + length,
                length: total,
            });
        }
        Region::circular_arc(start, length, total)?
    } else {
        Region::linear(start, start + length, total)?
    })
}

/// Bases `[start, start + length)` of a (circular) product.
fn slice(product: &str, start: usize, length: usize) -> String {
    let n = product.len();
    (0..length)
        .map(|i| char::from(product.as_bytes()[(start + i) % n]))
        .collect()
}

/// Product parts of a complete core projection, adjacent pieces of one source part merged.
fn mapped_parts(mapping: &FeatureMapping, offset: usize) -> Vec<(usize, usize)> {
    let mut parts: Vec<(usize, usize, usize)> = Vec::new(); // (source part, start, length)
    for part in &mapping.parts {
        let start = offset + part.fragment_region.start().get();
        let length = part.fragment_region.length().get();
        match parts.last_mut() {
            Some((source, s, l)) if *source == part.source_part && *s + *l == start => *l += length,
            Some((source, s, l)) if *source == part.source_part && start + length == *s => {
                *s = start;
                *l += length;
            }
            _ => parts.push((part.source_part, start, length)),
        }
    }
    parts.into_iter().map(|(_, s, l)| (s, l)).collect()
}

/// Features carried from one component's source, and warnings for those cut by the core.
fn carried_features(
    component: &dnagent_domain::gibson::GibsonComponent,
    source: &SequenceRecord,
    number: usize,
    total: usize,
    circular: bool,
    pending: &mut Vec<Pending>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), AppError> {
    let core = format!(
        "[{}, {}) of {}",
        component.selection.start,
        component.selection.start + component.selection.length,
        source.name()
    );
    for mapping in &component.annotations {
        let Some(feature) = source
            .features()
            .iter()
            .find(|f| f.id().as_str() == mapping.source_feature_id)
        else {
            continue;
        };
        let label = if feature.label().is_empty() {
            feature.kind()
        } else {
            feature.label()
        };
        if !mapping.complete {
            warnings.push(ImportWarning::new(
                "gibson_feature_clipped",
                format!(
                    "{label} ({}) from {} is cut by core {number} {core} ({} of {} bases); not carried into the product",
                    mapping.source_feature_id,
                    source.name(),
                    mapping.retained_bases,
                    mapping.source_bases
                ),
            ));
            continue;
        }
        let parts = mapped_parts(mapping, component.product_start);
        let regions = parts
            .iter()
            .map(|&(s, l)| region(s, l, total, circular))
            .collect::<Result<Vec<_>, _>>()?;
        let operator = if regions.len() == 1 {
            LocationOperator::Contiguous
        } else {
            feature.location().operator()
        };
        let operator = if regions.len() > 1 && operator == LocationOperator::Contiguous {
            LocationOperator::Join
        } else {
            operator
        };
        let mut qualifiers = feature.qualifiers().to_vec();
        qualifiers.push(Qualifier {
            key: "note".into(),
            value: Some(format!(
                "Gibson product: {} from {} (core {number})",
                mapping.source_feature_id,
                source.name()
            )),
        });
        pending.push(Pending {
            start: parts.first().map_or(0, |p| p.0),
            kind: feature.kind().to_owned(),
            label: label.to_owned(),
            location: Location::new(regions, mapping.fragment_strand, operator)?,
            qualifiers,
            color: feature.display().color.clone(),
        });
    }
    Ok(())
}

/// Primer-binding features for one component; a primer that does not read as one
/// stretch of the product is listed in the record but not annotated.
fn primer_features(
    component: &dnagent_domain::gibson::GibsonComponent,
    pair: &(PrimerCandidate, PrimerCandidate),
    number: usize,
    product: &str,
    circular: bool,
    pending: &mut Vec<Pending>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<(), AppError> {
    let total = product.len();
    // Primers: where each full oligo (tail included) matches the product.
    let core_end = component.product_start + component.core_sequence_5to3.len();
    let (forward, reverse) = pair;
    let placements = [
        (
            "F",
            forward,
            (component.product_start + total - forward.tail_sequence_5to3.len()) % total,
            Strand::Forward,
        ),
        (
            "R",
            reverse,
            core_end + total - reverse.annealing_sequence_5to3.len(),
            Strand::Reverse,
        ),
    ];
    for (letter, primer, start, strand) in placements {
        let length = primer.sequence_5to3.len();
        let on_product = if strand == Strand::Forward {
            Some(primer.sequence_5to3.clone())
        } else {
            reverse_complement(&primer.sequence_5to3)
        };
        let fits = circular || start % total + length <= total;
        if !fits || on_product.as_deref() != Some(slice(product, start % total, length).as_str()) {
            warnings.push(ImportWarning::new(
                "gibson_primer_not_placed",
                format!("primer {letter}{number} does not match the product as one stretch; listed but not annotated"),
            ));
            continue;
        }
        let tail = primer.tail_sequence_5to3.len();
        pending.push(Pending {
            start: start % total,
            kind: "primer_bind".into(),
            label: format!("Gibson {letter}{number}"),
            location: Location::new(
                vec![region(start, length, total, circular)?],
                strand,
                LocationOperator::Contiguous,
            )?,
            qualifiers: vec![Qualifier {
                key: "note".into(),
                value: Some(format!(
                    "{} nt: {} nt annealing to core {number}{}",
                    length,
                    primer.annealing_sequence_5to3.len(),
                    if tail > 0 {
                        format!(" + {tail} nt tail from the next core")
                    } else {
                        String::new()
                    }
                )),
            }],
            color: None,
        });
    }
    Ok(())
}

/// One feature per junction overlap.
fn junction_features(
    report: &GibsonReport,
    total: usize,
    circular: bool,
    pending: &mut Vec<Pending>,
) -> Result<(), AppError> {
    for junction in &report.junctions {
        let closure = if junction.closure {
            " (closes the circle)"
        } else {
            ""
        };
        pending.push(Pending {
            start: junction.product_start,
            kind: "misc_feature".into(),
            label: format!(
                "Gibson overlap {}-{}",
                junction.after_component, junction.before_component
            ),
            location: Location::new(
                vec![region(
                    junction.product_start,
                    junction.overlap_sequence_5to3.len(),
                    total,
                    circular,
                )?],
                Strand::Unknown,
                LocationOperator::Contiguous,
            )?,
            qualifiers: vec![Qualifier {
                key: "note".into(),
                value: Some(format!(
                    "{} nt overlap joining core {} to core {}{closure}",
                    junction.overlap_sequence_5to3.len(),
                    junction.after_component,
                    junction.before_component,
                )),
            }],
            color: Some("#b0b0b0".into()),
        });
    }
    Ok(())
}

/// The designed oligos, as the record's (unplaced) primer list.
fn primer_list(
    primers: &[(PrimerCandidate, PrimerCandidate)],
) -> Result<Vec<ImportedPrimer>, AppError> {
    primers
        .iter()
        .enumerate()
        .flat_map(|(i, (f, r))| {
            [
                (format!("Gibson F{}", i + 1), f),
                (format!("Gibson R{}", i + 1), r),
            ]
        })
        .map(|(name, primer)| {
            let tail = if primer.tail_sequence_5to3.is_empty() {
                String::new()
            } else {
                format!(", tail {}", primer.tail_sequence_5to3)
            };
            Ok(ImportedPrimer {
                name,
                sequence: DnaSeq::new(&primer.sequence_5to3)?,
                description: Some(format!(
                    "Gibson PCR primer (candidate, not validated): annealing {}{tail}",
                    primer.annealing_sequence_5to3,
                )),
            })
        })
        .collect()
}

/// Build the product record. `primers` are (forward, reverse) per component, in order
/// (the fixed-length candidates, or the optimiser's choices).
pub fn product_record(
    report: &GibsonReport,
    primers: &[(PrimerCandidate, PrimerCandidate)],
    name: &str,
) -> Result<(ImportReport, Vec<ImportWarning>), AppError> {
    let product = &report.product_sequence_5to3;
    let total = product.len();
    let circular = report.topology == Topology::Circular;
    let mut warnings = Vec::new();
    let mut pending = Vec::new();

    for (index, component) in report.components.iter().enumerate() {
        let source = &report.inputs[component.selection.input - 1];
        carried_features(
            component,
            source,
            index + 1,
            total,
            circular,
            &mut pending,
            &mut warnings,
        )?;
        primer_features(
            component,
            &primers[index],
            index + 1,
            product,
            circular,
            &mut pending,
            &mut warnings,
        )?;
    }
    junction_features(report, total, circular, &mut pending)?;
    pending.sort_by_key(|p| p.start);
    let features = pending
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            Ok(Feature::new(
                FeatureId::new(format!("feature-{:04}", i + 1))?,
                p.kind,
                p.label,
                p.location,
                p.qualifiers,
                DisplayHints { color: p.color },
            ))
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let oligos = primer_list(primers)?;
    let record = SequenceRecord::new(
        name,
        DnaSeq::new(product)?,
        report.topology,
        features,
        oligos,
    )?;
    Ok((
        ImportReport {
            record,
            warnings: Vec::new(),
            preserved_metadata: FormatExtensions::default(),
        },
        warnings,
    ))
}
