#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use scratchpad::ScratchpadApp;
use scratchpad::app::app_paths;
use scratchpad::app::app_state::prepare_context_before_first_frame;
use scratchpad::app::platform;
use scratchpad::app::services::instance_broker::{ElectionResult, LaunchRequest, PrimaryInstance};
use scratchpad::app::services::session_store::SessionStore;
use scratchpad::app::services::settings_store::{
    DEFAULT_WINDOW_INNER_SIZE, MIN_WINDOW_INNER_SIZE, SettingsStore, WindowState,
};
use scratchpad::app::startup::StartupOptions;

fn main() -> eframe::Result<()> {
    let startup_action = scratchpad::app::startup::parse_startup_action_from_env();
    match &startup_action {
        scratchpad::app::startup::StartupAction::Help => {
            println!("{}", scratchpad::app::startup::USAGE_TEXT);
            return Ok(());
        }
        scratchpad::app::startup::StartupAction::Version => {
            println!("scratchpad {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    let startup_options = match &startup_action {
        scratchpad::app::startup::StartupAction::Run(options) => options.clone(),
        scratchpad::app::startup::StartupAction::Help
        | scratchpad::app::startup::StartupAction::Version => StartupOptions::default(),
    };
    let launch_request = LaunchRequest::from_startup_options(
        new_invocation_id().map_err(eframe::Error::AppCreation)?,
        startup_options.clone(),
    )
    .map_err(app_creation_error)?;
    let primary = match PrimaryInstance::elect(&app_paths::runtime_session_root(), &launch_request)
        .map_err(app_creation_error)?
    {
        ElectionResult::Primary(primary) => primary,
        ElectionResult::Forwarded => return Ok(()),
        ElectionResult::Rejected(reason) => {
            eprintln!("Scratchpad: {reason}");
            return Ok(());
        }
    };

    let legacy_root = app_paths::legacy_temp_root();
    let session_store =
        SessionStore::with_fallback(app_paths::runtime_session_root(), legacy_root.clone());
    let settings_store =
        SettingsStore::with_fallback(app_paths::runtime_settings_root(), legacy_root);
    let startup_settings = settings_store.load().ok().flatten().unwrap_or_default();

    let options = eframe::NativeOptions {
        viewport: viewport_builder_from_window_state(
            &startup_settings.ui.window_state,
            platform::capabilities(startup_settings.platform.profile),
        ),
        renderer: renderer_from_env(),
        persist_window: false,
        persistence_path: Some(session_store.root().join("eframe-state.ron")),
        ..Default::default()
    };

    eframe::run_native(
        "Scratchpad",
        options,
        Box::new(move |cc| {
            let inbox = primary.start(&cc.egui_ctx)?;
            let mut app = ScratchpadApp::with_stores_and_runtime_startup(
                session_store,
                settings_store,
                startup_options,
            );
            app.attach_instance_broker(inbox);
            prepare_context_before_first_frame(&mut app, &cc.egui_ctx);
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            Ok(Box::new(app))
        }),
    )
}

fn app_creation_error(error: std::io::Error) -> eframe::Error {
    eframe::Error::AppCreation(Box::new(error))
}

fn new_invocation_id() -> Result<u128, Box<dyn std::error::Error + Send + Sync>> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)?;
    Ok(u128::from_le_bytes(bytes))
}

fn renderer_from_env() -> eframe::Renderer {
    match std::env::var("SCRATCHPAD_RENDERER") {
        #[cfg(target_os = "linux")]
        Ok(value) if value.eq_ignore_ascii_case("glow") => eframe::Renderer::Glow,
        Ok(value) if value.eq_ignore_ascii_case("wgpu") => eframe::Renderer::Wgpu,
        _ => eframe::Renderer::default(),
    }
}

fn viewport_builder_from_window_state(
    window_state: &WindowState,
    capabilities: platform::PlatformCapabilities,
) -> egui::ViewportBuilder {
    let mut viewport = egui::ViewportBuilder::default()
        // Hyprland matches this Wayland app ID as its window class.
        .with_app_id("scratchpad")
        .with_decorations(capabilities.use_native_decorations)
        .with_visible(false)
        .with_inner_size(window_state.inner_size.unwrap_or(DEFAULT_WINDOW_INNER_SIZE))
        .with_min_inner_size(MIN_WINDOW_INNER_SIZE);

    if let Some(icon) = scratchpad_window_icon() {
        viewport = viewport.with_icon(icon);
    }

    if let Some(position) = window_state.position {
        viewport = viewport.with_position(egui::pos2(position[0], position[1]));
    }

    viewport
}

fn scratchpad_window_icon() -> Option<egui::IconData> {
    let png_bytes = best_png_from_ico(include_bytes!("assets/Scratchpad.ico"))?;
    eframe::icon_data::from_png_bytes(png_bytes).ok()
}

fn best_png_from_ico(ico: &[u8]) -> Option<&[u8]> {
    const ICO_HEADER_LEN: usize = 6;
    const ICO_DIRECTORY_ENTRY_LEN: usize = 16;
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

    if ico.len() < ICO_HEADER_LEN
        || u16::from_le_bytes([ico[0], ico[1]]) != 0
        || u16::from_le_bytes([ico[2], ico[3]]) != 1
    {
        return None;
    }

    let image_count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let directory_len =
        ICO_HEADER_LEN.checked_add(image_count.checked_mul(ICO_DIRECTORY_ENTRY_LEN)?)?;
    if ico.len() < directory_len {
        return None;
    }

    let mut best_image: Option<(u32, &[u8])> = None;
    for index in 0..image_count {
        let entry_start = ICO_HEADER_LEN + index * ICO_DIRECTORY_ENTRY_LEN;
        let entry = &ico[entry_start..entry_start + ICO_DIRECTORY_ENTRY_LEN];
        let width = if entry[0] == 0 { 256 } else { entry[0] as u32 };
        let height = if entry[1] == 0 { 256 } else { entry[1] as u32 };
        let image_size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let image_offset =
            u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let image_end = image_offset.checked_add(image_size)?;
        let image = ico.get(image_offset..image_end)?;

        if !image.starts_with(PNG_SIGNATURE) {
            continue;
        }

        let area = width * height;
        if best_image.is_none_or(|(best_area, _)| area > best_area) {
            best_image = Some((area, image));
        }
    }

    best_image.map(|(_, image)| image)
}
