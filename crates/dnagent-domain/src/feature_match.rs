//! Sequence identity for the feature library: a feature's bases in its own direction,
//! a strand-independent key, and exact occurrences on both strands (wrapping on circles).
use crate::{Feature, SequenceRecord, Strand, Topology};

/// Watson–Crick complement of an upper-case ACGT sequence, read 5′→3′; `None` for any other base.
#[must_use]
pub fn reverse_complement(sequence: &str) -> Option<String> {
    sequence
        .bytes()
        .rev()
        .map(|base| match base {
            b'A' => Some('T'),
            b'C' => Some('G'),
            b'G' => Some('C'),
            b'T' => Some('A'),
            _ => None,
        })
        .collect()
}

/// The feature's bases 5′→3′ in its own direction: parts joined in source order, each
/// wrapping through the origin if it does; reverse features are reverse-complemented.
/// `None` if any base is not A, C, G or T.
#[must_use]
pub fn feature_sequence(record: &SequenceRecord, feature: &Feature) -> Option<String> {
    let sequence = record.sequence().as_str();
    let length = sequence.len();
    let mut joined = String::new();
    for part in feature.location().parts() {
        let start = part.start().get();
        let size = part.length().get();
        let end = start + size;
        if end <= length {
            joined.push_str(&sequence[start..end]);
        } else {
            joined.push_str(&sequence[start..]);
            joined.push_str(&sequence[..end - length]);
        }
    }
    if !joined
        .bytes()
        .all(|b| matches!(b, b'A' | b'C' | b'G' | b'T'))
    {
        return None;
    }
    if feature.location().strand() == Strand::Reverse {
        reverse_complement(&joined)
    } else {
        Some(joined)
    }
}

/// Strand-independent identity: the lexicographically smaller of a sequence and its
/// reverse complement. `None` for non-ACGT input.
#[must_use]
pub fn canonical(sequence: &str) -> Option<String> {
    let reverse = reverse_complement(sequence)?;
    Some(if reverse.as_str() < sequence {
        reverse
    } else {
        sequence.to_owned()
    })
}

/// One exact occurrence of a query on a molecule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Occurrence {
    /// Start on the forward strand (zero-based); with `length`, may wrap on a circle.
    pub start: usize,
    pub length: usize,
    /// `Forward`: the query reads 5′→3′ on the top strand; `Reverse`: on the bottom strand.
    pub strand: Strand,
}

/// Every exact occurrence of `query` (ACGT, 5′→3′) on both strands, ordered by start then
/// strand. A palindromic query is reported once, on the forward strand. On a circle,
/// occurrences may run through the origin; a query longer than the molecule never matches.
#[must_use]
pub fn find_exact(sequence: &str, topology: Topology, query: &str) -> Vec<Occurrence> {
    Searcher::new(sequence, topology, query.len()).find(query)
}

/// Repeated [`find_exact`] searches on one molecule, preparing the origin-wrapped text once.
pub struct Searcher {
    text: String,
    length: usize,
    circular: bool,
}

impl Searcher {
    /// Prepare for queries up to `longest` bases.
    #[must_use]
    pub fn new(sequence: &str, topology: Topology, longest: usize) -> Self {
        let circular = topology == Topology::Circular;
        let length = sequence.len();
        let wrap = if circular {
            longest.saturating_sub(1).min(length)
        } else {
            0
        };
        Self {
            text: format!("{sequence}{}", &sequence[..wrap]),
            length,
            circular,
        }
    }

    /// See [`find_exact`]. Queries longer than the `longest` given to [`Searcher::new`]
    /// miss origin-spanning occurrences, so size it for the longest query.
    #[must_use]
    pub fn find(&self, query: &str) -> Vec<Occurrence> {
        let (n, m) = (self.length, query.len());
        let Some(reverse) = reverse_complement(query) else {
            return Vec::new();
        };
        if m == 0 || m > n {
            return Vec::new();
        }
        let text = if self.circular {
            &self.text[..(n + m - 1).min(self.text.len())]
        } else {
            &self.text[..n]
        };
        let mut found = Vec::new();
        let mut scan = |needle: &str, strand: Strand| {
            let mut from = 0;
            while let Some(offset) = text[from..].find(needle) {
                let start = from + offset;
                if start >= n {
                    break;
                }
                found.push(Occurrence {
                    start,
                    length: m,
                    strand,
                });
                from = start + 1;
            }
        };
        scan(query, Strand::Forward);
        if reverse != query {
            scan(&reverse, Strand::Reverse);
        }
        found.sort_by_key(|o| (o.start, o.strand == Strand::Reverse));
        found
    }
}

/// Fewest substitutions, insertions and deletions that align all of `short` to some
/// stretch of `long` (end gaps in `long` are free), or `None` if more than `max`.
#[must_use]
pub fn semiglobal_edits(short: &[u8], long: &[u8], max: usize) -> Option<usize> {
    let m = long.len();
    let mut previous = vec![0usize; m + 1]; // aligning nothing costs nothing anywhere in `long`
    let mut current = vec![0usize; m + 1];
    for (i, &a) in short.iter().enumerate() {
        current[0] = i + 1;
        let mut best = current[0];
        for j in 1..=m {
            let substitution = previous[j - 1] + usize::from(a != long[j - 1]);
            current[j] = substitution.min(previous[j] + 1).min(current[j - 1] + 1);
            best = best.min(current[j]);
        }
        if best > max {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous.iter().copied().min().filter(|&edits| edits <= max)
}

/// Standard-code translation of whole codons from the first base (stops as `*`).
#[must_use]
pub fn translate_standard(sequence: &str) -> String {
    let code = crate::translation::GeneticCode::standard();
    sequence
        .as_bytes()
        .chunks_exact(3)
        .map(|c| code.translate_codon([c[0], c[1], c[2]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_strands_origin_and_palindromes() {
        // GAATTC is palindromic: forward only. ACCG forward at 1; its reverse CGGT at 6.
        let hits = find_exact("TACCGAACGGT", Topology::Linear, "ACCG");
        assert_eq!(
            hits,
            vec![
                Occurrence {
                    start: 1,
                    length: 4,
                    strand: Strand::Forward
                },
                Occurrence {
                    start: 7,
                    length: 4,
                    strand: Strand::Reverse
                },
            ]
        );
        assert_eq!(
            find_exact("GAATTCGAATTC", Topology::Linear, "GAATTC").len(),
            2
        );
        // Across the origin of a circle, not on a linear molecule.
        assert_eq!(
            find_exact("CGTTTTTAC", Topology::Circular, "ACCG"),
            vec![Occurrence {
                start: 7,
                length: 4,
                strand: Strand::Forward
            }]
        );
        assert!(find_exact("CGTTTTTAC", Topology::Linear, "ACCG").is_empty());
        assert!(find_exact("ACG", Topology::Circular, "ACGT").is_empty());
    }

    #[test]
    fn semiglobal_edits_align_the_short_one_fully() {
        assert_eq!(
            semiglobal_edits(b"ACGT", b"TTACGTTT", 0),
            Some(0),
            "exact containment"
        );
        assert_eq!(
            semiglobal_edits(b"ACCT", b"TTACGTTT", 1),
            Some(1),
            "one substitution"
        );
        assert_eq!(
            semiglobal_edits(b"ACGGT", b"TTACGTTT", 1),
            Some(1),
            "one insertion in the short one"
        );
        assert_eq!(
            semiglobal_edits(b"AGT", b"TTACGTTT", 1),
            Some(1),
            "one deletion"
        );
        assert_eq!(semiglobal_edits(b"GGGG", b"TTACGTTT", 2), None, "bounded");
        assert_eq!(translate_standard("ATGAAATAGC"), "MK*");
    }

    #[test]
    fn canonical_is_strand_independent() {
        assert_eq!(canonical("TTGCA"), canonical("TGCAA"));
        assert_eq!(canonical("ACGN"), None);
        assert_eq!(reverse_complement("AACG").as_deref(), Some("CGTT"));
    }
}
