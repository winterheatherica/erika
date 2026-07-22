mod app;
mod export;
mod model;
mod trace;
mod ui;

use app::App;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Erika"),
        ..Default::default()
    };
    eframe::run_native(
        "Erika",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}
