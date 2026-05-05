mod app;
mod config_tab;
mod logs_tab;
mod preset_tab;
mod router_tab;
mod settings_tab;
mod status_tab;
mod token_counter_tab;
mod transformers_tab;
mod tray_manager;

fn main() {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "CCR",
        options,
        Box::new(|_cc| Ok(Box::new(app::CcrApp::new()))),
    )
    .unwrap();
}
