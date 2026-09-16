//! Conservative sequence exports for materialised exact-overlap assemblies.
use dnagent_domain::Topology;
use dnagent_domain::existing_overlaps::ExistingAssembly;
use std::fmt::Write;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssemblyExportError {
    #[error("cannot export assembled product: {0}")]
    Invalid(&'static str),
}

/// Sequence-only FASTA. JSON remains authoritative for source associations.
pub fn fasta(assembly: &ExistingAssembly, name: &str) -> Result<String, AssemblyExportError> {
    validate(assembly)?;
    let safe_name = safe_identifier(name);
    let topology = match assembly.topology {
        Topology::Linear => "linear",
        Topology::Circular => "circular",
    };
    let mut out = format!(
        ">{safe_name} topology={topology} length={} derived_by=dnagent_exact_overlap_assembly\n",
        assembly.product_sequence_5to3.len()
    );
    for chunk in assembly.product_sequence_5to3.as_bytes().chunks(80) {
        out.push_str(std::str::from_utf8(chunk).expect("validated ASCII DNA"));
        out.push('\n');
    }
    Ok(out)
}

/// Conservative GenBank view with one `misc_feature` per component association.
/// It does not infer genes, CDSs, joins between source features or translations.
pub fn genbank(assembly: &ExistingAssembly, name: &str) -> Result<String, AssemblyExportError> {
    validate(assembly)?;
    let length = assembly.product_sequence_5to3.len();
    let safe_name = safe_identifier(name);
    let topology = match assembly.topology {
        Topology::Linear => "linear",
        Topology::Circular => "circular",
    };
    let mut out = String::new();
    writeln!(
        out,
        "LOCUS       {safe_name:<16} {length:>11} bp    DNA     {topology:<8} UNK 01-JAN-1980"
    )
    .expect("String write");
    out.push_str("DEFINITION  DNAagent derived exact-overlap assembly.\nACCESSION   .\nVERSION     .\nKEYWORDS    .\nSOURCE      .\n  ORGANISM  .\n            .\n");
    out.push_str("COMMENT     Computationally derived product, not experimental validation.\n            Components are provenance misc_features; genes, CDSs, translations\n            and biological function are not reconstructed. JSON is authoritative\n            for source records, selections, mappings and overlap junctions.\n            LOCUS date is a fixed export placeholder.\n");
    out.push_str("FEATURES             Location/Qualifiers\n");
    for (index, component) in assembly.components.iter().enumerate() {
        let component_length = component.sequence_5to3.len();
        let end = component
            .product_start
            .checked_add(component_length)
            .ok_or(AssemblyExportError::Invalid(
                "component coordinate overflow",
            ))?;
        let location = if end <= length {
            format!("{}..{end}", component.product_start + 1)
        } else if assembly.topology == Topology::Circular && end - length <= length {
            format!(
                "join({}..{length},1..{})",
                component.product_start + 1,
                end - length
            )
        } else {
            return Err(AssemblyExportError::Invalid(
                "component placement is outside the product",
            ));
        };
        writeln!(out, "     misc_feature    {location}").expect("String write");
        writeln!(
            out,
            "                     /label=\"component_{}\"",
            index + 1
        )
        .expect("String write");
        writeln!(
            out,
            "                     /note=\"input={} source_interval=[{},{}), orientation={:?}; shared overlap associations are retained in JSON\"",
            component.selection.input,
            component.selection.start,
            component.selection.start + component.selection.length,
            component.selection.orientation
        )
        .expect("String write");
    }
    out.push_str("ORIGIN\n");
    for (line, bases) in assembly
        .product_sequence_5to3
        .as_bytes()
        .chunks(60)
        .enumerate()
    {
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
    Ok(out)
}

fn validate(assembly: &ExistingAssembly) -> Result<(), AssemblyExportError> {
    if assembly.product_sequence_5to3.is_empty()
        || !assembly
            .product_sequence_5to3
            .bytes()
            .all(|base| b"ACGT".contains(&base))
        || assembly.components.is_empty()
    {
        return Err(AssemblyExportError::Invalid(
            "product must contain components and unambiguous ACGT sequence",
        ));
    }
    Ok(())
}

fn safe_identifier(name: &str) -> String {
    let filtered: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(16)
        .collect();
    if filtered.is_empty() {
        "dnagent_product".into()
    } else {
        filtered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnagent_domain::existing_overlaps;
    use dnagent_domain::gibson::CoreSelection;
    use dnagent_domain::ligation::Orientation;
    use dnagent_domain::{DnaSeq, SequenceRecord};

    fn assembly(topology: Topology) -> ExistingAssembly {
        let sequence = "ACGT".repeat(30);
        let record = SequenceRecord::new(
            "source",
            DnaSeq::new(&sequence).unwrap(),
            Topology::Circular,
            vec![],
            vec![],
        )
        .unwrap();
        let fragments = vec![CoreSelection {
            input: 1,
            start: 0,
            length: 120,
            orientation: Orientation::Forward,
        }];
        let overlaps = if topology == Topology::Circular {
            vec![20]
        } else {
            vec![]
        };
        if topology == Topology::Circular {
            // A non-repetitive closure is needed by the domain uniqueness policy.
            let sequence = "ACGTTGCAAGTCCTAGGATC".repeat(6);
            let record = SequenceRecord::new(
                "source",
                DnaSeq::new(sequence).unwrap(),
                Topology::Circular,
                vec![],
                vec![],
            )
            .unwrap();
            return existing_overlaps::assemble(&[record], &fragments, topology, &overlaps)
                .unwrap();
        }
        existing_overlaps::assemble(&[record], &fragments, topology, &overlaps).unwrap()
    }

    #[test]
    fn exports_parseable_sequence_views_without_cds_claims() {
        let product = assembly(Topology::Linear);
        let fasta = fasta(&product, "test product").unwrap();
        assert!(fasta.starts_with(">testproduct topology=linear"));
        assert_eq!(fasta.lines().skip(1).collect::<String>().len(), 120);
        let gb = genbank(&product, "test product").unwrap();
        assert!(gb.contains("misc_feature    1..120"));
        assert!(!gb.contains("     CDS"));
        assert!(gb.ends_with("//\n"));
    }
}
