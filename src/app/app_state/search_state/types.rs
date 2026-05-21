use crate::app::domain::{BufferId, ViewId};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchScope {
    SelectionOnly,
    #[default]
    ActiveBuffer,
    ActiveWorkspaceTab,
    AllOpenTabs,
}

impl SearchScope {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SelectionOnly => "Selection",
            Self::ActiveBuffer => "Active File",
            Self::ActiveWorkspaceTab => "Current Tab",
            Self::AllOpenTabs => "All Open Tabs",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchScopeOrigin {
    Manual,
    SelectionDefault,
    #[default]
    ActiveContextDefault,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchFocusTarget {
    FindInput,
    ReplaceInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchStatus {
    Idle,
    Searching {
        scanned_targets: usize,
        total_targets: usize,
    },
    Ready,
    NoMatches,
    InvalidQuery(String),
    Error(String),
}

impl SearchStatus {
    pub(crate) fn message(&self) -> Option<&str> {
        match self {
            Self::InvalidQuery(message) | Self::Error(message) => Some(message.as_str()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SearchFreshness {
    #[default]
    Fresh,
    Stale,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchReplaceAvailability {
    Allowed,
    Disabled,
    Blocked(String),
}

impl SearchReplaceAvailability {
    pub(crate) fn allows_actions(&self) -> bool {
        matches!(self, Self::Allowed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchMatch {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) target_revision: u64,
    pub(crate) range: Range<usize>,
    pub(crate) matched_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResultEntry {
    pub(crate) match_index: usize,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) line_number: usize,
    pub(crate) column_number: usize,
    pub(crate) preview: String,
    pub(crate) active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchResultGroup {
    pub(crate) tab_index: usize,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) tab_label: String,
    pub(crate) first_match_index: usize,
    pub(crate) total_match_count: usize,
    pub(crate) active: bool,
}

#[derive(Clone)]
pub(crate) struct SearchProgress {
    pub(crate) scanned_targets: usize,
    pub(crate) target_count: usize,
    pub(crate) displayed_match_count: usize,
    pub(crate) total_match_count: usize,
    pub(crate) status: SearchStatus,
    pub(crate) freshness: SearchFreshness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplacementTargetPlan {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) buffer_id: BufferId,
    pub(crate) buffer_label: String,
    pub(crate) target_revision: u64,
    pub(crate) expected_matches: Vec<(Range<usize>, String)>,
    pub(crate) replacements: Vec<(Range<usize>, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplacementPlan {
    pub(crate) scope: SearchScope,
    pub(crate) targets: Vec<ReplacementTargetPlan>,
    pub(crate) total_match_count: usize,
}

impl ReplacementPlan {
    pub(crate) fn affected_buffer_count(&self) -> usize {
        self.targets.len()
    }

    pub(super) fn requires_confirmation(&self) -> bool {
        const HIGH_REPLACE_ALL_MATCH_COUNT: usize = 100;
        self.affected_buffer_count() > 1 || self.total_match_count > HIGH_REPLACE_ALL_MATCH_COUNT
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplaceAllConfirmation {
    pub(crate) scope: SearchScope,
    pub(crate) affected_buffer_count: usize,
    pub(crate) total_match_count: usize,
    pub(crate) replacement: String,
    requested_generation: u64,
}

impl ReplaceAllConfirmation {
    pub(super) fn from_plan(
        plan: &ReplacementPlan,
        replacement: &str,
        requested_generation: u64,
    ) -> Self {
        Self {
            scope: plan.scope,
            affected_buffer_count: plan.affected_buffer_count(),
            total_match_count: plan.total_match_count,
            replacement: replacement.to_owned(),
            requested_generation,
        }
    }

    pub(super) fn matches_plan(
        &self,
        plan: &ReplacementPlan,
        replacement: &str,
        requested_generation: u64,
    ) -> bool {
        self.scope == plan.scope
            && self.affected_buffer_count == plan.affected_buffer_count()
            && self.total_match_count == plan.total_match_count
            && self.replacement == replacement
            && self.requested_generation == requested_generation
    }
}
