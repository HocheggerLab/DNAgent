//! Biological domain types shared by every DNAagent adapter.

pub mod restriction;

use serde::Serialize;
use std::{collections::HashSet, fmt};
use thiserror::Error;

const IUPAC_DNA: &str = "ACGTRYSWKMBDHVN";

/// Errors raised while constructing checked domain values.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// A sequence was empty.
    #[error("DNA sequence must not be empty")]
    EmptySequence,
    /// A sequence contained a non-IUPAC DNA symbol.
    #[error("invalid IUPAC DNA symbol {symbol:?} at byte {index}")]
    InvalidSymbol { symbol: char, index: usize },
    /// A stable identifier was empty.
    #[error("{kind} identifier must not be empty")]
    EmptyId { kind: &'static str },
    /// A location had no regions.
    #[error("a feature location must contain at least one region")]
    EmptyLocation,
    /// The operator did not match the number of regions.
    #[error("location operator {operator:?} is incompatible with {part_count} regions")]
    InvalidLocationOperator {
        operator: LocationOperator,
        part_count: usize,
    },
    /// A region did not fit the molecule.
    #[error("invalid region for molecule length {molecule_length}: {reason}")]
    InvalidRegion {
        molecule_length: usize,
        reason: String,
    },
    /// Two features used the same record-local identifier.
    #[error("duplicate feature identifier {id:?}")]
    DuplicateFeatureId { id: String },
}

/// Validated, canonical uppercase IUPAC DNA.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct DnaSeq(String);

impl DnaSeq {
    /// Validate and canonicalize an IUPAC DNA sequence.
    pub fn new(sequence: impl AsRef<str>) -> Result<Self, DomainError> {
        let sequence = sequence.as_ref();
        if sequence.is_empty() {
            return Err(DomainError::EmptySequence);
        }

        let mut canonical = String::with_capacity(sequence.len());
        for (index, symbol) in sequence.char_indices() {
            let symbol = symbol.to_ascii_uppercase();
            if !symbol.is_ascii() || !IUPAC_DNA.contains(symbol) {
                return Err(DomainError::InvalidSymbol { symbol, index });
            }
            canonical.push(symbol);
        }
        Ok(Self(canonical))
    }

    /// Return the canonical sequence.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Return the number of bases.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// DNAagent sequences are never empty after construction.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Return a checked zero-based, half-open sequence slice.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> Option<&str> {
        if start <= end {
            self.0.get(start..end)
        } else {
            None
        }
    }
}

impl fmt::Display for DnaSeq {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Physical topology of an imported DNA record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Topology {
    /// A molecule with two ends.
    Linear,
    /// A covalently closed or conceptually circular record.
    Circular,
}

/// Zero-based base-boundary position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Position(usize);

impl Position {
    /// Create a position. Bounds are checked by the containing region.
    #[must_use]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Return the numeric boundary position.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Number of bases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Length(usize);

impl Length {
    /// Create a non-zero length.
    #[must_use]
    pub const fn new(value: usize) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// Return the number of bases.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Strand relative to the record's forward sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Strand {
    /// Forward/reference strand.
    Forward,
    /// Reverse/complementary strand.
    Reverse,
    /// Strand is absent or unknown.
    Unknown,
}

/// How multiple regions compose a biological location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationOperator {
    /// One contiguous region.
    Contiguous,
    /// Ordered joined regions.
    Join,
    /// Ordered regions whose precise relationship is unknown.
    Order,
}

/// A checked zero-based, half-open region. Construction is only through checked methods.
/// JSON retains the tagged `linear` / `circular_arc` representation.
///
/// ```compile_fail
/// use dnagent_domain::{Position, Region};
/// let invalid = Region::Linear { start: Position::new(9), end: Position::new(2) };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Region(RegionValue);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RegionValue {
    /// A conventional non-wrapping interval.
    Linear { start: Position, end: Position },
    /// A contiguous interval on a circle, represented by start and length.
    CircularArc { start: Position, length: Length },
}

impl Region {
    /// Construct a checked linear interval.
    pub fn linear(start: usize, end: usize, molecule_length: usize) -> Result<Self, DomainError> {
        if start >= end || end > molecule_length {
            return Err(invalid_region(
                molecule_length,
                format!("linear interval [{start}, {end})"),
            ));
        }
        Ok(Self(RegionValue::Linear {
            start: Position::new(start),
            end: Position::new(end),
        }))
    }

    /// Construct a checked circular arc.
    pub fn circular_arc(
        start: usize,
        length: usize,
        molecule_length: usize,
    ) -> Result<Self, DomainError> {
        if start >= molecule_length || length == 0 || length > molecule_length {
            return Err(invalid_region(
                molecule_length,
                format!("circular arc starting at {start} with length {length}"),
            ));
        }
        Ok(Self(RegionValue::CircularArc {
            start: Position::new(start),
            length: Length::new(length).expect("length was checked as non-zero"),
        }))
    }

    /// Return the first boundary on the stored forward record.
    #[must_use]
    pub const fn start(&self) -> Position {
        match &self.0 {
            RegionValue::Linear { start, .. } | RegionValue::CircularArc { start, .. } => *start,
        }
    }

    /// Return the number of bases represented by this region.
    #[must_use]
    pub const fn length(&self) -> Length {
        match &self.0 {
            RegionValue::Linear { start, end } => Length(end.0 - start.0),
            RegionValue::CircularArc { length, .. } => *length,
        }
    }

    /// Whether this region requires circular topology.
    #[must_use]
    pub const fn is_circular_arc(&self) -> bool {
        matches!(self.0, RegionValue::CircularArc { .. })
    }
}

fn invalid_region(molecule_length: usize, reason: String) -> DomainError {
    DomainError::InvalidRegion {
        molecule_length,
        reason,
    }
}

/// A feature location preserving biological identity across one or more regions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Location {
    parts: Vec<Region>,
    strand: Strand,
    operator: LocationOperator,
}

impl Location {
    /// Construct a non-empty location.
    pub fn new(
        parts: Vec<Region>,
        strand: Strand,
        operator: LocationOperator,
    ) -> Result<Self, DomainError> {
        if parts.is_empty() {
            return Err(DomainError::EmptyLocation);
        }
        if (parts.len() == 1) != (operator == LocationOperator::Contiguous) {
            return Err(DomainError::InvalidLocationOperator {
                operator,
                part_count: parts.len(),
            });
        }
        Ok(Self {
            parts,
            strand,
            operator,
        })
    }

    /// Return the ordered regions.
    #[must_use]
    pub fn parts(&self) -> &[Region] {
        &self.parts
    }

    /// Return the strand.
    #[must_use]
    pub const fn strand(&self) -> Strand {
        self.strand
    }

    /// Return the composition operator.
    #[must_use]
    pub const fn operator(&self) -> LocationOperator {
        self.operator
    }
}

/// Stable feature identifier within an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct FeatureId(String);

impl FeatureId {
    /// Construct a non-empty feature identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        if value.is_empty() {
            return Err(DomainError::EmptyId { kind: "feature" });
        }
        Ok(Self(value))
    }

    /// Return the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Ordered, repeatable feature qualifier. A `None` value represents a valueless qualifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Qualifier {
    pub key: String,
    pub value: Option<String>,
}

/// Non-biological presentation hints imported from a format.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DisplayHints {
    pub color: Option<String>,
}

/// An annotated biological feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Feature {
    id: FeatureId,
    kind: String,
    label: String,
    location: Location,
    qualifiers: Vec<Qualifier>,
    display: DisplayHints,
}

impl Feature {
    /// Construct a feature.
    #[must_use]
    pub fn new(
        id: FeatureId,
        kind: impl Into<String>,
        label: impl Into<String>,
        location: Location,
        qualifiers: Vec<Qualifier>,
        display: DisplayHints,
    ) -> Self {
        Self {
            id,
            kind: kind.into(),
            label: label.into(),
            location,
            qualifiers,
            display,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &FeatureId {
        &self.id
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn location(&self) -> &Location {
        &self.location
    }

    #[must_use]
    pub fn qualifiers(&self) -> &[Qualifier] {
        &self.qualifiers
    }

    #[must_use]
    pub const fn display(&self) -> &DisplayHints {
        &self.display
    }
}

/// Primer content retained during migration import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportedPrimer {
    pub name: String,
    pub sequence: DnaSeq,
    pub description: Option<String>,
}

/// Imported annotated DNA artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SequenceRecord {
    name: String,
    sequence: DnaSeq,
    topology: Topology,
    features: Vec<Feature>,
    primers: Vec<ImportedPrimer>,
}

impl SequenceRecord {
    /// Construct a normalized record and verify record-level invariants.
    pub fn new(
        name: impl Into<String>,
        sequence: DnaSeq,
        topology: Topology,
        features: Vec<Feature>,
        primers: Vec<ImportedPrimer>,
    ) -> Result<Self, DomainError> {
        let molecule_length = sequence.len();
        let mut feature_ids = HashSet::with_capacity(features.len());
        for feature in &features {
            if !feature_ids.insert(feature.id().as_str()) {
                return Err(DomainError::DuplicateFeatureId {
                    id: feature.id().as_str().to_owned(),
                });
            }
            for region in feature.location().parts() {
                match &region.0 {
                    RegionValue::Linear { end, .. } if end.get() > molecule_length => {
                        return Err(invalid_region(
                            molecule_length,
                            format!("feature {} extends to {}", feature.id().as_str(), end.get()),
                        ));
                    }
                    RegionValue::CircularArc { start, length }
                        if topology != Topology::Circular
                            || start.get() >= molecule_length
                            || length.get() > molecule_length =>
                    {
                        return Err(invalid_region(
                            molecule_length,
                            format!(
                                "feature {} has an incompatible circular arc",
                                feature.id().as_str()
                            ),
                        ));
                    }
                    _ => {}
                }
            }
        }
        Ok(Self {
            name: name.into(),
            sequence,
            topology,
            features,
            primers,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn sequence(&self) -> &DnaSeq {
        &self.sequence
    }

    #[must_use]
    pub const fn topology(&self) -> Topology {
        self.topology
    }

    #[must_use]
    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    #[must_use]
    pub fn primers(&self) -> &[ImportedPrimer] {
        &self.primers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_validated_and_canonicalized() {
        let sequence = DnaSeq::new("acgtryn").unwrap();
        assert_eq!(sequence.as_str(), "ACGTRYN");
        assert!(matches!(
            DnaSeq::new("ACGU"),
            Err(DomainError::InvalidSymbol {
                symbol: 'U',
                index: 3
            })
        ));
    }

    #[test]
    fn circular_arc_can_cross_the_origin() {
        let region = Region::circular_arc(8, 4, 10).unwrap();
        assert_eq!(region.start().get(), 8);
        assert_eq!(region.length().get(), 4);
    }

    #[test]
    fn region_boundaries_are_checked_exhaustively_on_small_molecules() {
        for size in 0..8 {
            for start in 0..10 {
                for end in 0..10 {
                    let region = Region::linear(start, end, size);
                    assert_eq!(region.is_ok(), start < end && end <= size);
                    if let Ok(region) = region {
                        assert_eq!(region.length().get(), end - start);
                    }
                    let arc = Region::circular_arc(start, end, size);
                    assert_eq!(arc.is_ok(), start < size && end > 0 && end <= size);
                }
            }
        }
        assert_eq!(
            Region::linear(0, usize::MAX, usize::MAX)
                .unwrap()
                .length()
                .get(),
            usize::MAX
        );
        assert!(Region::linear(usize::MAX, 0, usize::MAX).is_err());
    }

    #[test]
    fn location_operator_must_match_part_count() {
        let part = Region::linear(0, 2, 10).unwrap();
        for operator in [
            LocationOperator::Contiguous,
            LocationOperator::Join,
            LocationOperator::Order,
        ] {
            assert!(Location::new(vec![], Strand::Unknown, operator).is_err());
            assert_eq!(
                Location::new(vec![part.clone()], Strand::Forward, operator).is_ok(),
                operator == LocationOperator::Contiguous
            );
            assert_eq!(
                Location::new(
                    vec![part.clone(), Region::linear(4, 6, 10).unwrap()],
                    Strand::Reverse,
                    operator
                )
                .is_ok(),
                operator != LocationOperator::Contiguous
            );
        }
    }

    #[test]
    fn record_rechecks_regions_against_actual_length_and_topology() {
        let make_record = |region, topology| {
            let feature = Feature::new(
                FeatureId::new("feature").unwrap(),
                "misc_feature",
                "test",
                Location::new(vec![region], Strand::Forward, LocationOperator::Contiguous).unwrap(),
                vec![],
                DisplayHints::default(),
            );
            SequenceRecord::new(
                "test",
                DnaSeq::new("ACGT").unwrap(),
                topology,
                vec![feature],
                vec![],
            )
        };
        assert!(make_record(Region::linear(3, 5, 10).unwrap(), Topology::Linear).is_err());
        assert!(make_record(Region::circular_arc(0, 5, 10).unwrap(), Topology::Circular).is_err());
        assert!(make_record(Region::circular_arc(4, 1, 10).unwrap(), Topology::Circular).is_err());
        assert!(make_record(Region::circular_arc(3, 2, 4).unwrap(), Topology::Linear).is_err());
        assert!(make_record(Region::circular_arc(3, 2, 4).unwrap(), Topology::Circular).is_ok());
        assert!(make_record(Region::linear(0, 4, 4).unwrap(), Topology::Linear).is_ok());
    }

    #[test]
    fn invalid_regions_are_rejected() {
        assert!(Region::linear(5, 5, 10).is_err());
        assert!(Region::circular_arc(10, 1, 10).is_err());
        assert!(Region::circular_arc(0, 11, 10).is_err());
    }
}
