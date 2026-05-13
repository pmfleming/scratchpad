mod egui_warning;
mod model;

use egui_warning::{
    extract_hexes, is_egui_target, is_egui_warning_message, should_capture_log_record,
    widget_rect_changed_fingerprint,
};
pub use model::AppDiagnosticKind;
use model::{AppDiagnostic, TrackedWidget};

use eframe::egui;
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const ERROR_LOG_NAME: &str = "error.log";
const RECENT_CONFLICT_LIMIT: usize = 256;

static DIAGNOSTICS: OnceLock<Mutex<DiagnosticsState>> = OnceLock::new();
static LOGGER: AppDiagnosticsLogger = AppDiagnosticsLogger;
static LOGGER_INSTALLED: OnceLock<()> = OnceLock::new();
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

#[derive(Debug)]
struct DiagnosticsState {
    log_path: PathBuf,
    frame: u64,
    pass_index: usize,
    seen_ids: HashMap<String, TrackedWidget>,
    prev_pass: HashMap<String, TrackedWidget>,
    current_pass: HashMap<String, TrackedWidget>,
    recent_conflicts: HashSet<String>,
    recent_conflict_order: VecDeque<String>,
    last_write_error: Option<String>,
}

impl DiagnosticsState {
    fn new(log_path: PathBuf) -> Self {
        Self {
            log_path,
            frame: 0,
            pass_index: 0,
            seen_ids: HashMap::new(),
            prev_pass: HashMap::new(),
            current_pass: HashMap::new(),
            recent_conflicts: HashSet::new(),
            recent_conflict_order: VecDeque::new(),
            last_write_error: None,
        }
    }

    fn reconfigure(&mut self, log_path: PathBuf) {
        if self.log_path != log_path {
            self.log_path = log_path;
            self.seen_ids.clear();
            self.prev_pass.clear();
            self.current_pass.clear();
            self.recent_conflicts.clear();
            self.recent_conflict_order.clear();
            self.last_write_error = None;
        }
    }

    fn begin_frame(&mut self) {
        self.frame = self.frame.saturating_add(1);
        self.pass_index = 0;
        self.seen_ids.clear();
        self.prev_pass.clear();
        self.current_pass.clear();
    }

    fn begin_pass(&mut self, pass_index: usize) {
        self.pass_index = pass_index;
        if pass_index == 0 {
            self.begin_frame();
            return;
        }

        self.prev_pass = std::mem::take(&mut self.current_pass);
        self.seen_ids.clear();
    }

    fn track_widget(
        &mut self,
        id: String,
        short_hex: String,
        rect: String,
        kind: &'static str,
        location: String,
        parent_ui_id: Option<String>,
    ) {
        let current = TrackedWidget {
            kind: kind.to_owned(),
            rect: rect.clone(),
            location: location.clone(),
            pass_index: self.pass_index,
            parent_ui_id,
        };

        self.current_pass.insert(short_hex, current.clone());

        if let Some(previous) = self.seen_ids.get(&id).cloned() {
            let fingerprint = format!(
                "{}|{}|{}|{}|{}",
                id, previous.kind, previous.rect, current.kind, current.rect
            );
            if self.mark_conflict_reported(fingerprint) {
                let diagnostic = AppDiagnostic::new(
                    AppDiagnosticKind::EguiIdConflict,
                    format!(
                        "duplicate egui id tracked: previous_kind={} current_kind={}",
                        previous.kind, current.kind
                    ),
                )
                .with_source("widget_ids::track")
                .with_widget_id(id.clone())
                .with_rect(format!(
                    "previous={} current={}",
                    previous.rect, current.rect
                ))
                .with_details([
                    (
                        "previous_parent_ui_id",
                        detail_or_unknown(previous.parent_ui_id.as_deref()),
                    ),
                    (
                        "current_parent_ui_id",
                        detail_or_unknown(current.parent_ui_id.as_deref()),
                    ),
                    ("previous_pass", previous.pass_index.to_string()),
                    ("current_pass", current.pass_index.to_string()),
                ])
                .with_frame(self.frame);
                self.append_diagnostic(&diagnostic);
            }
        } else {
            self.seen_ids.insert(id, current);
        }
    }

    fn log_record(&mut self, level: Level, target: &str, message: String) {
        let kind = if is_egui_target(target) || is_egui_warning_message(&message) {
            AppDiagnosticKind::EguiWarning
        } else {
            AppDiagnosticKind::Other
        };
        let mut diagnostic = AppDiagnostic::new(kind, message.clone())
            .with_source(format!("log::{level}:{target}"))
            .with_frame(self.frame);

        if diagnostic.kind == AppDiagnosticKind::EguiWarning
            && message.starts_with("Widget rect ")
            && message.contains("changed id between passes")
        {
            let prev_sites =
                self.resolve_sites(extract_hexes(&message, "prev ids: [", "]"), &self.prev_pass);
            if !prev_sites.is_empty() {
                diagnostic
                    .details
                    .insert("prev_site".to_owned(), prev_sites.join(" | "));
            }
            let new_sites = self.resolve_sites(
                extract_hexes(&message, "new ids: [", "]"),
                &self.current_pass,
            );
            if !new_sites.is_empty() {
                diagnostic
                    .details
                    .insert("new_site".to_owned(), new_sites.join(" | "));
            }

            let fingerprint = widget_rect_changed_fingerprint(&message, &prev_sites, &new_sites);
            if !self.mark_conflict_reported(fingerprint) {
                return;
            }
        }

        self.append_diagnostic(&diagnostic);
    }

    fn resolve_sites(
        &self,
        hexes: Vec<String>,
        pass: &HashMap<String, TrackedWidget>,
    ) -> Vec<String> {
        hexes
            .into_iter()
            .filter_map(|hex| {
                pass.get(&hex).map(|widget| {
                    let parent = widget.parent_ui_id.as_deref().unwrap_or("<unknown_parent>");
                    format!(
                        "{} {} {} pass={} parent_ui_id={}",
                        hex, widget.location, widget.kind, widget.pass_index, parent
                    )
                })
            })
            .collect()
    }

    fn log_panic(&mut self, message: String) {
        let diagnostic = AppDiagnostic::new(AppDiagnosticKind::Panic, message)
            .with_source("panic_hook")
            .with_frame(self.frame);
        self.append_diagnostic(&diagnostic);
    }

    fn append_session_header(&mut self) {
        let diagnostic = AppDiagnostic::new(
            AppDiagnosticKind::SessionStarted,
            format!(
                "Scratchpad session started; version={}; profile={}; os={}; eframe={}",
                env!("CARGO_PKG_VERSION"),
                if cfg!(debug_assertions) {
                    "debug"
                } else {
                    "release"
                },
                std::env::consts::OS,
                option_env!("SCRATCHPAD_EFRAME_VERSION").unwrap_or("unknown")
            ),
        );
        self.append_diagnostic(&diagnostic);
    }

    fn append_diagnostic(&mut self, diagnostic: &AppDiagnostic) {
        match append_diagnostic_to_path(&self.log_path, diagnostic) {
            Ok(()) => self.last_write_error = None,
            Err(error) => self.last_write_error = Some(error.to_string()),
        }
    }

    fn mark_conflict_reported(&mut self, fingerprint: String) -> bool {
        if self.recent_conflicts.contains(&fingerprint) {
            return false;
        }
        self.recent_conflicts.insert(fingerprint.clone());
        self.recent_conflict_order.push_back(fingerprint);
        while self.recent_conflict_order.len() > RECENT_CONFLICT_LIMIT {
            if let Some(oldest) = self.recent_conflict_order.pop_front() {
                self.recent_conflicts.remove(&oldest);
            }
        }
        true
    }
}

fn detail_or_unknown(value: Option<&str>) -> String {
    value.unwrap_or("<unknown>").to_owned()
}

pub fn default_error_log_path() -> PathBuf {
    std::env::temp_dir().join("scratchpad").join(ERROR_LOG_NAME)
}

pub(crate) fn error_log_path(root: &Path) -> PathBuf {
    root.join(ERROR_LOG_NAME)
}

pub fn initialize_default() {
    initialize(default_error_log_path());
}

pub(crate) fn initialize(log_path: PathBuf) {
    let diagnostics =
        DIAGNOSTICS.get_or_init(|| Mutex::new(DiagnosticsState::new(log_path.clone())));
    if let Ok(mut state) = diagnostics.lock() {
        state.reconfigure(log_path);
        state.append_session_header();
    }
    install_logger();
    install_panic_hook();
}

pub(crate) fn begin_pass(pass_index: usize) {
    with_state(|state| state.begin_pass(pass_index));
}

pub(crate) fn track_widget_id(
    id: egui::Id,
    rect: egui::Rect,
    kind: &'static str,
    location: &'static std::panic::Location<'static>,
    parent_ui_id: Option<egui::Id>,
) {
    let id_str = format!("{:016X}", id.value());
    let short_hex = id.short_debug_format();
    let rect_str = format!("{rect:?}");
    let loc_str = format!("{}:{}", location.file(), location.line());
    let parent_ui_id = parent_ui_id.map(|id| format!("{:016X}", id.value()));
    with_state(|state| {
        state.track_widget(id_str, short_hex, rect_str, kind, loc_str, parent_ui_id)
    });
}

pub(crate) fn record_io_error(
    operation: &'static str,
    path: Option<&Path>,
    source: &'static str,
    error: &dyn std::fmt::Display,
) {
    record_diagnostic(build_io_diagnostic(
        operation,
        path,
        source,
        error.to_string(),
        std::iter::empty(),
    ));
}

pub(crate) fn record_io_error_with_details(
    operation: &'static str,
    path: Option<&Path>,
    source: &'static str,
    error: &dyn std::fmt::Display,
    details: impl IntoIterator<Item = (&'static str, String)>,
) {
    record_diagnostic(build_io_diagnostic(
        operation,
        path,
        source,
        error.to_string(),
        details,
    ));
}

pub(crate) fn record_warning(
    operation: &'static str,
    path: Option<&Path>,
    source: &'static str,
    message: impl Into<String>,
) {
    let mut diagnostic = AppDiagnostic::new(AppDiagnosticKind::Other, message)
        .with_source(source)
        .with_operation(operation);
    if let Some(path) = path {
        diagnostic = diagnostic.with_path(path);
    }
    record_diagnostic(diagnostic);
}

pub(crate) fn record_background_failure(
    operation: &'static str,
    source: &'static str,
    message: impl Into<String>,
    details: impl IntoIterator<Item = (&'static str, String)>,
) {
    record_diagnostic(
        AppDiagnostic::new(AppDiagnosticKind::Other, message)
            .with_source(source)
            .with_operation(operation)
            .with_details(details),
    );
}

fn build_io_diagnostic(
    operation: &'static str,
    path: Option<&Path>,
    source: &'static str,
    message: String,
    details: impl IntoIterator<Item = (&'static str, String)>,
) -> AppDiagnostic {
    let mut diagnostic = AppDiagnostic::new(AppDiagnosticKind::Io, message)
        .with_source(source)
        .with_operation(operation)
        .with_details(details);
    if let Some(path) = path {
        diagnostic = diagnostic.with_path(path);
    }
    diagnostic
}

fn record_diagnostic(diagnostic: AppDiagnostic) {
    with_state(|state| {
        let diagnostic = diagnostic.with_frame(state.frame);
        state.append_diagnostic(&diagnostic);
    });
}

fn install_logger() {
    LOGGER_INSTALLED.get_or_init(|| {
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(LevelFilter::Warn);
        }
    });
}

fn install_panic_hook() {
    PANIC_HOOK_INSTALLED.get_or_init(|| {
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            let message = panic_message(info);
            with_state(|state| state.log_panic(message));
            previous(info);
        }));
    });
}

fn with_state(action: impl FnOnce(&mut DiagnosticsState)) {
    if let Some(diagnostics) = DIAGNOSTICS.get()
        && let Ok(mut state) = diagnostics.lock()
    {
        action(&mut state);
    }
}

fn append_diagnostic_to_path(path: &Path, diagnostic: &AppDiagnostic) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(diagnostic).map_err(io::Error::other)?;
    writeln!(file, "{line}")
}

fn timestamp_now() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => format!("{}.{:03}Z", duration.as_secs(), duration.subsec_millis()),
        Err(_) => "0.000Z".to_owned(),
    }
}

fn panic_message(info: &panic::PanicHookInfo<'_>) -> String {
    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic payload is not a string".to_owned());
    let location = info
        .location()
        .map(|location| format!("{}:{}", location.file(), location.line()))
        .unwrap_or_else(|| "unknown location".to_owned());
    let thread = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_owned();
    format!("thread={thread}; location={location}; message={payload}")
}

struct AppDiagnosticsLogger;

impl Log for AppDiagnosticsLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= Level::Warn
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        if !should_capture_log_record(record.metadata(), &message) {
            return;
        }
        with_state(|state| state.log_record(record.level(), record.target(), message));
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::{
        AppDiagnostic, AppDiagnosticKind, DiagnosticsState, ERROR_LOG_NAME,
        append_diagnostic_to_path, build_io_diagnostic, extract_hexes, should_capture_log_record,
    };
    use log::{Level, Metadata};
    use std::fs;

    #[test]
    fn append_creates_error_log() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join(ERROR_LOG_NAME);
        let diagnostic = AppDiagnostic::new(AppDiagnosticKind::Io, "created");

        append_diagnostic_to_path(&path, &diagnostic).unwrap();

        let contents = fs::read_to_string(path).unwrap();
        assert!(contents.contains("created"));
    }

    #[test]
    fn append_does_not_overwrite_existing_log() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(ERROR_LOG_NAME);

        append_diagnostic_to_path(&path, &AppDiagnostic::new(AppDiagnosticKind::Io, "first"))
            .unwrap();
        append_diagnostic_to_path(&path, &AppDiagnostic::new(AppDiagnosticKind::Io, "second"))
            .unwrap();

        let contents = fs::read_to_string(path).unwrap();
        assert!(contents.contains("first"));
        assert!(contents.contains("second"));
    }

    #[test]
    fn structured_io_diagnostic_includes_operation_path_and_details() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(ERROR_LOG_NAME);
        let target_path = directory.path().join("target.txt");
        let diagnostic = build_io_diagnostic(
            "save_file",
            Some(&target_path),
            "test_source",
            "disk full".to_owned(),
            [("encoding", "UTF-8".to_owned())],
        );

        append_diagnostic_to_path(&path, &diagnostic).unwrap();

        let contents = fs::read_to_string(path).unwrap();
        let payload: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(payload["kind"], "io");
        assert_eq!(payload["operation"], "save_file");
        assert_eq!(payload["source"], "test_source");
        assert_eq!(payload["path"], target_path.display().to_string());
        assert_eq!(payload["details"]["encoding"], "UTF-8");
    }

    #[test]
    fn unavailable_log_path_does_not_panic() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().to_path_buf();
        let mut state = DiagnosticsState::new(path);

        state.append_diagnostic(&AppDiagnostic::new(AppDiagnosticKind::Io, "blocked"));

        assert!(state.last_write_error.is_some());
    }

    #[test]
    fn duplicate_widget_ids_are_deduplicated() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(ERROR_LOG_NAME);
        let mut state = DiagnosticsState::new(path.clone());
        state.begin_frame();

        state.track_widget(
            "id".to_owned(),
            "hex".to_owned(),
            "rect-a".to_owned(),
            "first",
            "loc".to_owned(),
            None,
        );
        state.track_widget(
            "id".to_owned(),
            "hex".to_owned(),
            "rect-b".to_owned(),
            "second",
            "loc".to_owned(),
            None,
        );
        state.begin_frame();
        state.track_widget(
            "id".to_owned(),
            "hex".to_owned(),
            "rect-a".to_owned(),
            "first",
            "loc".to_owned(),
            None,
        );
        state.track_widget(
            "id".to_owned(),
            "hex".to_owned(),
            "rect-b".to_owned(),
            "second",
            "loc".to_owned(),
            None,
        );

        let contents = fs::read_to_string(path).unwrap();
        assert_eq!(contents.matches("egui_id_conflict").count(), 1);
    }

    #[test]
    fn extract_hexes_reads_all_warning_ids() {
        let message = "Widget rect [[0.0 0.0] - [1.0 1.0]] changed id between passes: prev ids: [\"AAAA\", \"BBBB\", \"CCCC\"], new ids: [\"DDDD\"]";

        assert_eq!(
            extract_hexes(message, "prev ids: [", "]"),
            vec!["AAAA", "BBBB", "CCCC"]
        );
        assert_eq!(extract_hexes(message, "new ids: [", "]"), vec!["DDDD"]);
    }

    #[test]
    fn changed_id_warning_attributes_all_matching_pass_sites() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(ERROR_LOG_NAME);
        let mut state = DiagnosticsState::new(path.clone());

        state.begin_pass(0);
        state.track_widget(
            "prev-id".to_owned(),
            "BBBB".to_owned(),
            "prev-rect".to_owned(),
            "prev_kind",
            "prev.rs:10".to_owned(),
            Some("prev-parent".to_owned()),
        );
        state.begin_pass(1);
        state.track_widget(
            "new-id".to_owned(),
            "DDDD".to_owned(),
            "new-rect".to_owned(),
            "new_kind",
            "new.rs:20".to_owned(),
            Some("new-parent".to_owned()),
        );

        state.log_record(
            Level::Warn,
            "egui::context",
            "Widget rect [[0.0 0.0] - [1.0 1.0]] changed id between passes: prev ids: [\"AAAA\", \"BBBB\"], new ids: [\"CCCC\", \"DDDD\"]".to_owned(),
        );

        let contents = fs::read_to_string(path).unwrap();
        let payload: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(
            payload["details"]["prev_site"],
            "BBBB prev.rs:10 prev_kind pass=0 parent_ui_id=prev-parent"
        );
        assert_eq!(
            payload["details"]["new_site"],
            "DDDD new.rs:20 new_kind pass=1 parent_ui_id=new-parent"
        );
    }

    #[test]
    fn logger_filter_captures_egui_id_warnings() {
        let metadata = Metadata::builder()
            .level(Level::Warn)
            .target("egui")
            .build();

        assert!(should_capture_log_record(&metadata, "id clash detected"));
    }

    #[test]
    fn logger_filter_captures_app_warnings() {
        let metadata = Metadata::builder()
            .level(Level::Warn)
            .target("scratchpad")
            .build();

        assert!(should_capture_log_record(&metadata, "ordinary warning"));
    }

    #[test]
    fn logger_filter_ignores_dependency_warnings() {
        let metadata = Metadata::builder()
            .level(Level::Warn)
            .target("some_dependency")
            .build();

        assert!(!should_capture_log_record(&metadata, "ordinary warning"));
    }
}
