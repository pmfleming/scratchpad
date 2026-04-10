use super::OpenHerePathOutcome;
use crate::app::utils::file_count_label;

#[derive(Default)]
pub(super) struct OpenHereBatchSummary {
    pub(super) opened_count: usize,
    pub(super) migrated_count: usize,
    already_here_count: usize,
    pub(super) failure_count: usize,
    pub(super) artifact_count: usize,
    last_artifact_warning: Option<String>,
}

impl OpenHereBatchSummary {
    pub(super) fn record(mut self, outcome: OpenHerePathOutcome) -> Self {
        match outcome {
            OpenHerePathOutcome::Opened { artifact_warning } => {
                self.opened_count += 1;
                if let Some(warning) = artifact_warning {
                    self.artifact_count += 1;
                    self.last_artifact_warning = Some(warning);
                }
            }
            OpenHerePathOutcome::Migrated => {
                self.migrated_count += 1;
            }
            OpenHerePathOutcome::AlreadyInCurrentTab => {
                self.already_here_count += 1;
            }
            OpenHerePathOutcome::Queued => {}
            OpenHerePathOutcome::Failed => {
                self.failure_count += 1;
            }
        }

        self
    }

    pub(super) fn status_message(&self) -> Option<String> {
        if self.opened_count == 1
            && self.migrated_count == 0
            && self.already_here_count == 0
            && self.failure_count == 0
        {
            return self
                .last_artifact_warning
                .clone()
                .or_else(|| Some("Opened 1 file in the current tab.".to_owned()));
        }

        let parts = [
            self.opened_message(),
            self.migrated_message(),
            self.already_here_message(),
            self.failure_message(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

        (!parts.is_empty()).then(|| parts.join("; "))
    }

    pub(super) fn log_message(&self) -> String {
        format!(
            "Open Here batch completed: opened={}, migrated={}, already_here={}, failed={}, artifacts={}",
            self.opened_count,
            self.migrated_count,
            self.already_here_count,
            self.failure_count,
            self.artifact_count
        )
    }

    fn opened_message(&self) -> Option<String> {
        if self.opened_count == 0 {
            None
        } else if self.artifact_count > 0 {
            Some(format!(
                "Opened {} here ({} with formatting artifacts)",
                file_count_label(self.opened_count),
                file_count_label(self.artifact_count)
            ))
        } else {
            Some(format!(
                "Opened {} here",
                file_count_label(self.opened_count)
            ))
        }
    }

    fn migrated_message(&self) -> Option<String> {
        (self.migrated_count > 0).then(|| {
            format!(
                "Migrated {} into the current tab",
                file_count_label(self.migrated_count)
            )
        })
    }

    fn already_here_message(&self) -> Option<String> {
        (self.already_here_count > 0).then(|| {
            format!(
                "{} already in the current tab",
                file_count_label(self.already_here_count)
            )
        })
    }

    fn failure_message(&self) -> Option<String> {
        (self.failure_count > 0).then(|| {
            format!(
                "{} failed to open here",
                file_count_label(self.failure_count)
            )
        })
    }
}
