#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod decoder;
mod transport;
mod ui;
pub fn log(message: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(".local/smoke.log")
    {
        let _ = writeln!(file, "{message}");
    }
}
fn main() -> eframe::Result<()> {
    let smoke = std::env::args().any(|a| a == "--smoke");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("ScreenIOS")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!(concat!(
                    env!("OUT_DIR"),
                    "/screenios.png"
                )))
                .expect("Project logo"),
            )
            .with_inner_size([380., 240.])
            .with_min_inner_size([360., 240.])
            .with_decorations(false)
            .with_resizable(true),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "ScreenIOS",
        options,
        Box::new(move |cc| Ok(Box::new(ui::App::new(cc, smoke)))),
    )
}
