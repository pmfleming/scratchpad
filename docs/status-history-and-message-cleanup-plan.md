# Status History And Message Cleanup Plan

This plan covers two related goals:

- Create a history of transient status messages that users can open from the status bar. (this history is per session - it is clear everytime we startup the app)
- Clean up inconsistent status-bar language so current and historical messages are user-focused, concise, and coherent.

## Current Problem

The app currently stores transient status text as `ScratchpadApp::status_message: Option<String>`.

The call sites use separate methods:

- `set_info_status`
- `set_warning_status`
- `set_error_status`

But all three methods store only a plain string. By the time the status bar renders, severity, domain, source, and actionability have been lost.

The result is that unrelated subsystems write messages in different styles, and the status bar can show permanent active-file state beside stale transient prose.

## Desired User Experience

The status bar should remain calm and compact.

The current transient message should answer:

- What happened?
- Does the user need to do anything?

The status history should answer:

- What happened recently?
- Which messages were warnings or errors?
- What file, setting, or operation was involved?
- What technical detail is available if the user needs it?

The tab attention dot is only a visual state marker. It does not get a status or error tooltip. The tab tooltip remains the full file/display name.

## Phase 1: Structured Status Messages

Replace the plain transient message source with a structured model.

Suggested shape:

```rust
pub(crate) struct StatusMessage {
    pub id: u64,
    pub created_at: std::time::SystemTime,
    pub severity: StatusSeverity,
    pub domain: StatusDomain,
    pub text: String,
    pub detail: Option<String>,
    pub action: Option<StatusAction>,
}

pub(crate) enum StatusSeverity {
    Info,
    Warning,
    Error,
}

pub(crate) enum StatusDomain {
    File,
    Disk,
    Search,
    Settings,
    Session,
    Encoding,
    History,
    Layout,
    App,
}
```

DO NOT Keep the current setters as compatibility wrappers:

- `set_info_status(message)`
- `set_warning_status(message)`
- `set_error_status(message)`

Add a bounded history collection:

```rust
current_status: Option<StatusMessage>,
status_history: VecDeque<StatusMessage>,
next_status_message_id: u64,
```

Initial cap: 100 messages.

Clearing the current message should not erase history.

## Phase 2: Status History UI

Add a status-history entry point in the status bar.

Recommended UI:

- A small status/history icon near the existing status-bar controls.
- Current transient message remains text, not a button.
- Clicking the icon opens a status-history popover or dialog.

Avoid using the text-history icon, because that already means document edit history.

History view content:

- Newest messages first.
- Severity marker.
- Short message text.
- Relative or compact timestamp.
- Optional file/domain label.
- Expandable detail for raw errors.

The primary row text should be user-facing. Raw OS, parser, or internal errors belong in detail text.

## Phase 3: User-Focused Message Vocabulary

Add central helpers or constructors for common domains instead of writing ad hoc strings at call sites.

Examples:

| Current | User-focused primary text | Detail |
| --- | --- | --- |
| `Session save failed: {error}` | `Could not save your session.` | `{error}` |
| `Settings TOML parse failed: {error}` | `Could not apply settings.toml.` | `{error}` |
| `Search replace-all was blocked because results are stale.` | `Search results changed. Run search again before replacing.` | none |
| `Open Here failed to create a balanced tile layout.` | `Could not add those files to this tab layout.` | none |
| `Reopen With Encoding is available only for files on disk.` | `Save this file before reopening it with another encoding.` | none |

The visible status-bar text should be short. Longer explanations should go to status history detail.

## Phase 4: Message Style Rules

Use these rules for status messages:

- Say what happened from the user's perspective.
- Use sentence case.
- Avoid programmer plurals such as `file(s)`, `tab(s)`, and `conflict(s)`.
- Avoid raw internal terms in primary text.
- Avoid duplicate prefixes such as `Control characters detected: Control characters detected`.
- End complete sentences with a period.
- Keep summary fragments short when they sit among status-bar facts.
- Put raw error strings in `detail`, not the primary message.
- Use consistent disk words: `Saved`, `Unsaved edits`, `Changed on disk`, `Conflict`, `Missing`.

## Phase 5: Migration Order

Migrate messages by domain to keep patches reviewable.

1. Disk, save, reload, missing-file, and conflict messages.
2. Open and Open Here summaries.
3. Settings and session messages.
4. Search and replace messages.
5. Text history, rename, close, reveal, and layout messages.
6. Startup restore conflict messages.

Each migration should preserve behavior first, then improve text.

## Phase 6: Tests

Add focused tests for:

- Info, warning, and error setters preserve severity.
- Setting a message pushes to history.
- History is capped.
- Clearing current status does not erase history.
- Status-history rows show primary text and hide detail until expanded.
- Message constructors avoid `file(s)`, `tab(s)`, and duplicated prefixes.
- Current tab tooltips remain full file/display names only, even when an attention dot is present.

## Open Decisions

- Whether status history should be a popover, modal dialog, or settings-adjacent panel.
- Whether routine info messages should expire after a short timeout.
- Whether repeated identical messages should be coalesced in history.
- Whether status actions should be included in the first implementation or added after the history UI lands.

## Suggested First Patch

The first patch should be intentionally small:

1. Add `StatusMessage`, `StatusSeverity`, and `StatusDomain`.
2. Replace `status_message: Option<String>` with `current_status: Option<StatusMessage>`.
3. Add bounded `status_history`.
4. Keep existing setter names and call sites working.
5. Render `current_status.text` exactly where `status_message` renders today.
6. Add tests for severity preservation, history push, cap, and clear behavior.

After that lands, add the status-history UI and begin domain-by-domain message cleanup.
