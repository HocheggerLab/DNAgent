//! Thin eframe/egui desktop adapter for DNAgent.

use dnagent_domain::Topology;
use dnagent_formats::ImportReport;
use dnagent_render::MapScene;
use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use std::path::{Path, PathBuf};

/// Launch the desktop adapter, optionally opening one `.dna` file.
pub fn run(initial_path: Option<&Path>) -> Result<(), eframe::Error> {
    let app = DnaAgentApp::new(initial_path);
    eframe::run_native(
        "DNAgent",
        eframe::NativeOptions::default(),
        Box::new(move |_context| Ok(Box::new(app))),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Map,
    Features,
    Sequence,
}

struct DnaAgentApp {
    report: Option<ImportReport>,
    opened_path: Option<PathBuf>,
    error: Option<String>,
    tab: Tab,
    selected_feature: Option<String>,
}

impl DnaAgentApp {
    fn new(initial_path: Option<&Path>) -> Self {
        let mut app = Self {
            report: None,
            opened_path: None,
            error: None,
            tab: Tab::Map,
            selected_feature: None,
        };
        if let Some(path) = initial_path {
            app.open(path);
        }
        app
    }

    fn open(&mut self, path: &Path) {
        match dnagent_app::open_path(path) {
            Ok(report) => {
                self.report = Some(report);
                self.opened_path = Some(path.to_owned());
                self.error = None;
                self.selected_feature = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("DNAgent");
            if let Some(report) = &self.report {
                ui.separator();
                ui.label(format!(
                    "{} · {} bp · {:?}",
                    report.record.name(),
                    report.record.sequence().len(),
                    report.record.topology()
                ));
                if !report.warnings.is_empty() {
                    ui.colored_label(
                        Color32::YELLOW,
                        format!("{} import warning(s)", report.warnings.len()),
                    );
                }
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tab, Tab::Map, "Map");
            ui.selectable_value(&mut self.tab, Tab::Features, "Features");
            ui.selectable_value(&mut self.tab, Tab::Sequence, "Sequence");
            if let Some(path) = &self.opened_path {
                ui.separator();
                ui.small(path.display().to_string());
            }
        });
        ui.separator();
    }

    fn map_view(&mut self, ui: &mut egui::Ui) {
        let Some(report) = &self.report else {
            empty_state(ui);
            return;
        };
        let scene = MapScene::from_record(&report.record);
        let available = ui.available_size();
        let side = available.x.min(available.y - 80.0).clamp(200.0, 620.0);
        let (response, painter) = ui.allocate_painter(Vec2::splat(side), Sense::hover());
        let center = response.rect.center();
        painter.text(
            Pos2::new(center.x, response.rect.top() + 30.0),
            egui::Align2::CENTER_CENTER,
            format!("{} · {} bp", scene.name, scene.length),
            egui::FontId::proportional(18.0),
            Color32::WHITE,
        );

        match scene.topology {
            Topology::Circular => {
                let radius = side * 0.32;
                painter.circle_stroke(center, radius, Stroke::new(2.0, Color32::GRAY));
                for (feature_index, feature) in scene.features.iter().enumerate() {
                    let lane = f32::from(u16::try_from(feature_index % 4).unwrap());
                    let feature_radius = radius + 12.0 + lane * 7.0;
                    let width = feature_width(feature, self.selected_feature.as_deref());
                    for segment in &feature.segments {
                        let points = arc_points(center, feature_radius, *segment, scene.length);
                        painter.add(egui::Shape::line(
                            points,
                            Stroke::new(width, parse_color(&feature.color)),
                        ));
                    }
                }
            }
            Topology::Linear => {
                let start_x = response.rect.left() + side * 0.1;
                let track_width = side * 0.8;
                painter.line_segment(
                    [
                        Pos2::new(start_x, center.y),
                        Pos2::new(start_x + track_width, center.y),
                    ],
                    Stroke::new(2.0, Color32::GRAY),
                );
                for (feature_index, feature) in scene.features.iter().enumerate() {
                    let lane = f32::from(u16::try_from(feature_index % 5).unwrap());
                    let y = center.y - 24.0 + lane * 12.0;
                    let width = feature_width(feature, self.selected_feature.as_deref());
                    for segment in &feature.segments {
                        let x1 = start_x + segment.start as f32 / scene.length as f32 * track_width;
                        let x2 = x1 + segment.length as f32 / scene.length as f32 * track_width;
                        painter.line_segment(
                            [Pos2::new(x1, y), Pos2::new(x2, y)],
                            Stroke::new(width, parse_color(&feature.color)),
                        );
                    }
                }
            }
        }

        ui.horizontal_wrapped(|ui| {
            for feature in &scene.features {
                let selected = self.selected_feature.as_deref() == Some(feature.id.as_str());
                if ui.selectable_label(selected, &feature.label).clicked() {
                    self.selected_feature = Some(feature.id.clone());
                }
            }
        });
    }

    fn features_view(&mut self, ui: &mut egui::Ui) {
        let Some(report) = &self.report else {
            empty_state(ui);
            return;
        };
        let rows = dnagent_app::feature_views(&report.record);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for row in rows {
                let selected = self.selected_feature.as_deref() == Some(row.id.as_str());
                let title = if row.label.is_empty() {
                    format!("{} ({})", row.id, row.kind)
                } else {
                    format!("{} — {}", row.label, row.kind)
                };
                if ui.selectable_label(selected, title).clicked() {
                    self.selected_feature = Some(row.id);
                }
                ui.indent("feature-location", |ui| {
                    ui.small(format!("{:?} · {:?}", row.strand, row.location.parts()));
                });
                ui.separator();
            }
        });
    }

    fn sequence_view(&self, ui: &mut egui::Ui) {
        let Some(report) = &self.report else {
            empty_state(ui);
            return;
        };
        let sequence = report.record.sequence().as_str();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (line, chunk) in sequence.as_bytes().chunks(60).enumerate() {
                let sequence = std::str::from_utf8(chunk).expect("validated DNA is ASCII");
                ui.monospace(format!("{:>8}  {sequence}", line * 60));
            }
        });
    }
}

impl eframe::App for DnaAgentApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dropped_path = ui.ctx().input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .find_map(|file| file.path.clone())
        });
        if let Some(path) = dropped_path {
            self.open(&path);
        }

        self.header(ui);
        if let Some(error) = &self.error {
            ui.colored_label(Color32::RED, error);
            ui.separator();
        }
        match self.tab {
            Tab::Map => self.map_view(ui),
            Tab::Features => self.features_view(ui),
            Tab::Sequence => self.sequence_view(ui),
        }
    }
}

fn empty_state(ui: &mut egui::Ui) {
    ui.centered_and_justified(|ui| {
        ui.label("Drop a SnapGene .dna file here or run `dnagent gui FILE.dna`.");
    });
}

fn feature_width(feature: &dnagent_render::MapFeature, selected_id: Option<&str>) -> f32 {
    if selected_id == Some(feature.id.as_str()) {
        8.0
    } else {
        5.0
    }
}

fn arc_points(
    center: Pos2,
    radius: f32,
    segment: dnagent_render::MapSegment,
    molecule_length: usize,
) -> Vec<Pos2> {
    let steps = (segment.length.saturating_mul(64) / molecule_length).clamp(2, 64);
    (0..=steps)
        .map(|step| {
            let offset = segment.length as f32 * step as f32 / steps as f32;
            let fraction = (segment.start as f32 + offset) / molecule_length as f32;
            let angle = fraction * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

fn parse_color(value: &str) -> Color32 {
    let Some(hex) = value.strip_prefix('#') else {
        return Color32::from_rgb(76, 120, 168);
    };
    if hex.len() != 6 {
        return Color32::from_rgb(76, 120, 168);
    }
    let parse = |range| u8::from_str_radix(&hex[range], 16).ok();
    match (parse(0..2), parse(2..4), parse(4..6)) {
        (Some(red), Some(green), Some(blue)) => Color32::from_rgb(red, green, blue),
        _ => Color32::from_rgb(76, 120, 168),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors_without_panicking() {
        assert_eq!(parse_color("#ff0000"), Color32::RED);
        assert_eq!(parse_color("not-a-color"), Color32::from_rgb(76, 120, 168));
    }
}
