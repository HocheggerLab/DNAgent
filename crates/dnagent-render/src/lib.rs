//! Renderer-independent map scene and deterministic SVG backend.

use dnagent_domain::{SequenceRecord, Strand, Topology};
use serde::Serialize;
use std::fmt::Write;

/// Geometry-neutral map scene shared by renderers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapScene {
    pub name: String,
    pub length: usize,
    pub topology: Topology,
    pub features: Vec<MapFeature>,
}

/// One biological feature in the map scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MapFeature {
    pub id: String,
    pub label: String,
    pub strand: Strand,
    pub color: String,
    pub segments: Vec<MapSegment>,
}

/// A contiguous segment represented as start plus length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MapSegment {
    pub start: usize,
    pub length: usize,
}

impl MapScene {
    /// Build a stable map scene from a normalized record.
    #[must_use]
    pub fn from_record(record: &SequenceRecord) -> Self {
        let features = record
            .features()
            .iter()
            .map(|feature| MapFeature {
                id: feature.id().as_str().to_owned(),
                label: feature.label().to_owned(),
                strand: feature.location().strand(),
                color: feature
                    .display()
                    .color
                    .clone()
                    .unwrap_or_else(|| "#4c78a8".to_owned()),
                segments: feature
                    .location()
                    .parts()
                    .iter()
                    .map(|region| MapSegment {
                        start: region.start().get(),
                        length: region.length().get(),
                    })
                    .collect(),
            })
            .collect();
        Self {
            name: record.name().to_owned(),
            length: record.sequence().len(),
            topology: record.topology(),
            features,
        }
    }

    /// Render a deterministic standalone SVG.
    #[must_use]
    pub fn to_svg(&self) -> String {
        match self.topology {
            Topology::Circular => self.circular_svg(),
            Topology::Linear => self.linear_svg(),
        }
    }

    fn circular_svg(&self) -> String {
        let mut svg = svg_header(&self.name);
        svg.push_str("  <circle cx=\"250\" cy=\"250\" r=\"150\" fill=\"none\" stroke=\"#333\" stroke-width=\"2\"/>\n");
        let _ = writeln!(
            svg,
            "  <text x=\"250\" y=\"244\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"18\">{}</text>",
            escape_xml(&self.name)
        );
        let _ = writeln!(
            svg,
            "  <text x=\"250\" y=\"270\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"14\">{} bp</text>",
            self.length
        );

        for (feature_index, feature) in self.features.iter().enumerate() {
            let radius = 160.0 + f64::from(u16::try_from(feature_index % 4).unwrap()) * 10.0;
            for (segment_index, segment) in feature.segments.iter().enumerate() {
                let element_id = format!("{}-segment-{}", feature.id, segment_index + 1);
                if segment.length >= self.length {
                    let _ = writeln!(
                        svg,
                        "  <circle id=\"{}\" cx=\"250\" cy=\"250\" r=\"{radius:.1}\" fill=\"none\" stroke=\"{}\" stroke-width=\"8\"/>",
                        escape_xml(&element_id),
                        safe_color(&feature.color)
                    );
                    continue;
                }
                let start_angle = fraction(segment.start, self.length) * std::f64::consts::TAU
                    - std::f64::consts::FRAC_PI_2;
                let end_angle = fraction(segment.start + segment.length, self.length)
                    * std::f64::consts::TAU
                    - std::f64::consts::FRAC_PI_2;
                let (start_x, start_y) = point_on_circle(radius, start_angle);
                let (end_x, end_y) = point_on_circle(radius, end_angle);
                let large_arc = u8::from(segment.length * 2 > self.length);
                let _ = writeln!(
                    svg,
                    "  <path id=\"{}\" d=\"M {start_x:.3} {start_y:.3} A {radius:.1} {radius:.1} 0 {large_arc} 1 {end_x:.3} {end_y:.3}\" fill=\"none\" stroke=\"{}\" stroke-width=\"8\"><title>{}</title></path>",
                    escape_xml(&element_id),
                    safe_color(&feature.color),
                    escape_xml(&feature.label)
                );
            }
        }
        svg.push_str("</svg>\n");
        svg
    }

    fn linear_svg(&self) -> String {
        let mut svg = svg_header(&self.name);
        svg.push_str("  <line x1=\"50\" y1=\"250\" x2=\"450\" y2=\"250\" stroke=\"#333\" stroke-width=\"2\"/>\n");
        for (feature_index, feature) in self.features.iter().enumerate() {
            let y = 225 + i32::try_from(feature_index % 5).unwrap() * 12;
            for (segment_index, segment) in feature.segments.iter().enumerate() {
                let x = 50.0 + fraction(segment.start, self.length) * 400.0;
                let width = fraction(segment.length, self.length) * 400.0;
                let _ = writeln!(
                    svg,
                    "  <rect id=\"{}-segment-{}\" x=\"{x:.3}\" y=\"{y}\" width=\"{width:.3}\" height=\"8\" fill=\"{}\"><title>{}</title></rect>",
                    escape_xml(&feature.id),
                    segment_index + 1,
                    safe_color(&feature.color),
                    escape_xml(&feature.label)
                );
            }
        }
        let _ = writeln!(
            svg,
            "  <text x=\"250\" y=\"200\" text-anchor=\"middle\" font-family=\"sans-serif\" font-size=\"18\">{} — {} bp</text>",
            escape_xml(&self.name),
            self.length
        );
        svg.push_str("</svg>\n");
        svg
    }
}

fn svg_header(name: &str) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 500 500\" role=\"img\" aria-label=\"{} plasmid map\">\n",
        escape_xml(name)
    )
}

fn fraction(value: usize, total: usize) -> f64 {
    value as f64 / total as f64
}

fn point_on_circle(radius: f64, angle: f64) -> (f64, f64) {
    (250.0 + radius * angle.cos(), 250.0 + radius * angle.sin())
}

fn safe_color(color: &str) -> &str {
    if color.len() == 7
        && color.starts_with('#')
        && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        color
    } else {
        "#4c78a8"
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dnagent_domain::DnaSeq;

    #[test]
    fn empty_circular_record_has_stable_svg() {
        let record = SequenceRecord::new(
            "pTest",
            DnaSeq::new("ACGT").unwrap(),
            Topology::Circular,
            vec![],
            vec![],
        )
        .unwrap();
        let svg = MapScene::from_record(&record).to_svg();
        assert!(svg.contains("<circle"));
        assert!(svg.contains("pTest"));
        assert!(svg.ends_with("</svg>\n"));
    }

    #[test]
    fn unsafe_colors_fall_back() {
        assert_eq!(safe_color("red"), "#4c78a8");
        assert_eq!(safe_color("#12aBcF"), "#12aBcF");
    }
}
