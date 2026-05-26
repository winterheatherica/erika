mod app;
mod curve;
mod export;
mod image_ref;
mod persist;
mod svg_export;
mod ui_canvas;
mod ui_sidepanel;
mod ui_topbar;

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
