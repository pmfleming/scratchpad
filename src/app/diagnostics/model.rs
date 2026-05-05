use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppDiagnosticKind {
    SessionStarted,
    EguiIdConflict,
    EguiWarning,
    Panic,
    Io,
    Other,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct AppDiagnostic {
    timestamp: String,
    pub(super) kind: AppDiagnosticKind,
    message: String,
    source: Option<String>,
    operation: Option<String>,
    path: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(super) details: BTreeMap<String, String>,
    widget_id: Option<String>,
    rect: Option<String>,
    frame: Option<u64>,
}

impl AppDiagnostic {
    pub(super) fn new(kind: AppDiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            timestamp: super::timestamp_now(),
            kind,
            message: message.into(),
            source: None,
            operation: None,
            path: None,
            details: BTreeMap::new(),
            widget_id: None,
            rect: None,
            frame: None,
        }
    }

    pub(super) fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub(super) fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    pub(super) fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.display().to_string());
        self
    }

    pub(super) fn with_details(
        mut self,
        details: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Self {
        for (key, value) in details {
            self.details.insert(key.to_owned(), value);
        }
        self
    }

    pub(super) fn with_widget_id(mut self, widget_id: impl Into<String>) -> Self {
        self.widget_id = Some(widget_id.into());
        self
    }

    pub(super) fn with_rect(mut self, rect: impl Into<String>) -> Self {
        self.rect = Some(rect.into());
        self
    }

    pub(super) fn with_frame(mut self, frame: u64) -> Self {
        self.frame = Some(frame);
        self
    }
}

#[derive(Clone, Debug)]
pub(super) struct TrackedWidget {
    pub(super) kind: String,
    pub(super) rect: String,
    pub(super) location: String,
    pub(super) pass_index: usize,
    pub(super) parent_ui_id: Option<String>,
}
