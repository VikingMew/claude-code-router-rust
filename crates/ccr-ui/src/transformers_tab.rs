use ccr_transformer::TransformerRegistry;
use eframe::egui;

pub struct TransformersTab {
    names: Vec<String>,
}

impl TransformersTab {
    pub fn new() -> Self {
        let reg = TransformerRegistry::new();
        let mut names: Vec<String> = reg.names().iter().map(|s| s.to_string()).collect();
        names.sort();
        Self { names }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Transformers");
        });
        ui.separator();
        ui.label("Available transformers");
        if self.names.is_empty() {
            ui.label("No transformers registered.");
            return;
        }
        ui.separator();
        for name in &self.names {
            ui.label(name);
        }
    }
}
