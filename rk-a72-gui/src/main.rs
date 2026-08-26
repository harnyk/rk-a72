mod app;
mod edit_panel;
mod file_ops;
mod keyboard_view;
mod layout_grid;
mod worker;

use app::GuiApp;

fn main() -> eframe::Result<()> {
    let mut app = GuiApp::default();
    app.try_connect();

    eframe::run_native(
        "RK A72 Keymap Editor",
        eframe::NativeOptions::default(),
        Box::new(move |_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )
}
