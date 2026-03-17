use codexbar_rs::gui::CodexBarGuiApp;
use tokio::runtime::Runtime;

fn main() -> eframe::Result<()> {
    let runtime = Runtime::new().expect("failed to create tokio runtime for GUI");
    let options = eframe::NativeOptions::default();

    eframe::run_native(
        "codexbar-rs",
        options,
        Box::new(move |_cc| Ok(Box::new(CodexBarGuiApp::new(runtime)))),
    )
}
