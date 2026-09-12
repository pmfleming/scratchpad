//! Opt-in native presentation pacing probe; no user settings/session/files are read.
//! Run on Wayland with `cargo run --release --example presentation_probe`.
//! Emits frame counts once a second and requests close after five seconds.
//! Use an external timeout if deliberately leaving its workspace hidden.
#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
fn main() -> eframe::Result<()> {
    use eframe::egui;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };
    use std::time::{Duration, Instant};

    struct Probe(Arc<AtomicU64>);
    impl eframe::App for Probe {
        fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
            let frame = self.0.fetch_add(1, Ordering::Relaxed);
            ui.heading("Scratchpad presentation pacing probe");
            ui.label("No user files or session data. Continuous redraw demand for five seconds.");
            let x = (frame % 200) as f32;
            ui.painter().rect_filled(
                egui::Rect::from_min_size(egui::pos2(20.0 + x, 100.0), egui::vec2(40.0, 40.0)),
                4.0,
                egui::Color32::LIGHT_BLUE,
            );
            ui.ctx().request_repaint();
        }
    }

    eframe::run_native(
        "Scratchpad presentation probe",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: egui::ViewportBuilder::default()
                .with_app_id("scratchpad")
                .with_inner_size([640.0, 240.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            let frames = Arc::new(AtomicU64::new(0));
            let observed = frames.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let start = Instant::now();
                let mut previous = 0;
                for _ in 0..5 {
                    std::thread::sleep(Duration::from_secs(1));
                    let count = observed.load(Ordering::Relaxed);
                    println!(
                        "{}",
                        serde_json::json!({"elapsed_seconds":start.elapsed().as_secs_f64(),
                    "frames":count,"frames_since_last_report":count-previous})
                    );
                    previous = count;
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            });
            Ok(Box::new(Probe(frames)))
        }),
    )
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("This opt-in probe targets the Linux/Wayland Glow presentation path.");
}
