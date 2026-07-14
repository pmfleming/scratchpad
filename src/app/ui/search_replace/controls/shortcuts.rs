use super::super::state::{SearchStripActions, SearchStripState};
use crate::app::app_state::{SearchReplaceAvailability, SearchScope};
use crate::app::services::search::SearchMode;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TextInputKind {
    Find,
    Replace,
}

#[derive(Clone, Copy)]
enum TextInputShortcut {
    CtrlEnter,
    AltEnter,
    ShiftEnter,
    PlainEnter,
    Escape,
}

#[derive(Clone, Copy)]
enum SearchActionShortcut {
    Next,
    Previous,
    ReplaceCurrent,
    ReplaceAll,
    Undo,
    Redo,
}

#[derive(Clone, Copy)]
struct SearchActionAvailability {
    match_count: usize,
    replace_allowed: bool,
    can_undo_text_operation: bool,
    can_redo_text_operation: bool,
    text_input_focused: bool,
}

pub(super) fn consume_text_input_keys(
    ui: &mut egui::Ui,
    actions: &mut SearchStripActions,
    input_kind: TextInputKind,
) {
    if consume_key(ui, egui::Modifiers::CTRL, egui::Key::Enter) {
        apply_text_input_shortcut(actions, input_kind, TextInputShortcut::CtrlEnter);
    } else if consume_key(ui, egui::Modifiers::ALT, egui::Key::Enter) {
        apply_text_input_shortcut(actions, input_kind, TextInputShortcut::AltEnter);
    } else if input_kind == TextInputKind::Find
        && consume_key(ui, egui::Modifiers::SHIFT, egui::Key::Enter)
    {
        apply_text_input_shortcut(actions, input_kind, TextInputShortcut::ShiftEnter);
    } else if consume_key(ui, egui::Modifiers::NONE, egui::Key::Enter) {
        apply_text_input_shortcut(actions, input_kind, TextInputShortcut::PlainEnter);
    }

    if consume_key(ui, egui::Modifiers::NONE, egui::Key::Escape) {
        apply_text_input_shortcut(actions, input_kind, TextInputShortcut::Escape);
    }
}

pub(super) fn consume_search_strip_shortcuts(
    ui: &mut egui::Ui,
    state: &mut SearchStripState,
    actions: &mut SearchStripActions,
    text_input_focused: bool,
) {
    consume_search_scope_shortcuts(ui, state);
    consume_search_option_shortcuts(ui, state);
    consume_search_action_shortcuts(ui, state, actions, text_input_focused);
}

fn consume_search_scope_shortcuts(ui: &mut egui::Ui, state: &mut SearchStripState) {
    for (key, scope) in [
        (egui::Key::Num1, SearchScope::SelectionOnly),
        (egui::Key::Num2, SearchScope::ActiveBuffer),
        (egui::Key::Num3, SearchScope::ActiveWorkspaceTab),
        (egui::Key::Num4, SearchScope::AllOpenTabs),
    ] {
        if consume_key(ui, egui::Modifiers::ALT, key) {
            state.scope = scope;
            break;
        }
    }
}

fn consume_search_option_shortcuts(ui: &mut egui::Ui, state: &mut SearchStripState) {
    if consume_key(ui, egui::Modifiers::ALT, egui::Key::R) {
        state.mode = toggled_search_mode(state.mode);
    }
    if consume_key(ui, egui::Modifiers::ALT, egui::Key::C) {
        state.match_case = !state.match_case;
    }
    if consume_key(ui, egui::Modifiers::ALT, egui::Key::W) {
        state.whole_word = !state.whole_word;
    }
}

fn consume_search_action_shortcuts(
    ui: &mut egui::Ui,
    state: &SearchStripState,
    actions: &mut SearchStripActions,
    text_input_focused: bool,
) {
    let availability = SearchActionAvailability {
        match_count: state.match_count,
        replace_allowed: matches!(
            state.replace_availability,
            SearchReplaceAvailability::Allowed
        ),
        can_undo_text_operation: state.can_undo_text_operation,
        can_redo_text_operation: state.can_redo_text_operation,
        text_input_focused,
    };

    for (modifiers, key, shortcut) in [
        (
            egui::Modifiers::NONE,
            egui::Key::F3,
            SearchActionShortcut::Next,
        ),
        (
            egui::Modifiers::SHIFT,
            egui::Key::F3,
            SearchActionShortcut::Previous,
        ),
        (
            egui::Modifiers::CTRL,
            egui::Key::Enter,
            SearchActionShortcut::ReplaceCurrent,
        ),
        (
            egui::Modifiers::ALT,
            egui::Key::Enter,
            SearchActionShortcut::ReplaceAll,
        ),
        (
            egui::Modifiers::CTRL,
            egui::Key::Z,
            SearchActionShortcut::Undo,
        ),
        (
            egui::Modifiers::CTRL,
            egui::Key::Y,
            SearchActionShortcut::Redo,
        ),
    ] {
        if consume_key(ui, modifiers, key) {
            apply_search_action_shortcut(actions, availability, shortcut);
        }
    }
}

fn apply_text_input_shortcut(
    actions: &mut SearchStripActions,
    input_kind: TextInputKind,
    shortcut: TextInputShortcut,
) {
    match shortcut {
        TextInputShortcut::CtrlEnter => actions.replace_current_requested = true,
        TextInputShortcut::AltEnter => actions.replace_all_requested = true,
        TextInputShortcut::ShiftEnter if input_kind == TextInputKind::Find => {
            actions.previous_requested = true;
        }
        TextInputShortcut::PlainEnter => match input_kind {
            TextInputKind::Find => actions.next_requested = true,
            TextInputKind::Replace => actions.replace_current_requested = true,
        },
        TextInputShortcut::Escape => actions.close_requested = true,
        TextInputShortcut::ShiftEnter => {}
    }
}

fn apply_search_action_shortcut(
    actions: &mut SearchStripActions,
    availability: SearchActionAvailability,
    shortcut: SearchActionShortcut,
) {
    match shortcut {
        SearchActionShortcut::Next if availability.match_count > 0 => {
            actions.next_requested = true;
        }
        SearchActionShortcut::Previous if availability.match_count > 0 => {
            actions.previous_requested = true;
        }
        SearchActionShortcut::ReplaceCurrent if availability.replace_allowed => {
            actions.replace_current_requested = true;
        }
        SearchActionShortcut::ReplaceAll if availability.replace_allowed => {
            actions.replace_all_requested = true;
        }
        SearchActionShortcut::Undo
            if !availability.text_input_focused && availability.can_undo_text_operation =>
        {
            actions.undo_requested = true;
        }
        SearchActionShortcut::Redo
            if !availability.text_input_focused && availability.can_redo_text_operation =>
        {
            actions.redo_requested = true;
        }
        _ => {}
    }
}

fn toggled_search_mode(mode: SearchMode) -> SearchMode {
    match mode {
        SearchMode::PlainText => SearchMode::Regex,
        SearchMode::Regex => SearchMode::PlainText,
    }
}

fn consume_key(ui: &mut egui::Ui, modifiers: egui::Modifiers, key: egui::Key) -> bool {
    ui.input_mut(|input| input.consume_key(modifiers, key))
}

#[cfg(test)]
mod tests {
    use super::{
        SearchActionAvailability, SearchActionShortcut, TextInputKind, TextInputShortcut,
        apply_search_action_shortcut, apply_text_input_shortcut, toggled_search_mode,
    };
    use crate::app::services::search::SearchMode;
    use crate::app::ui::search_replace::state::SearchStripActions;

    #[test]
    fn find_input_enter_shortcuts_choose_navigation_actions() {
        let mut actions = SearchStripActions::default();
        apply_text_input_shortcut(
            &mut actions,
            TextInputKind::Find,
            TextInputShortcut::PlainEnter,
        );
        assert!(actions.next_requested);

        let mut actions = SearchStripActions::default();
        apply_text_input_shortcut(
            &mut actions,
            TextInputKind::Find,
            TextInputShortcut::ShiftEnter,
        );
        assert!(actions.previous_requested);
    }

    #[test]
    fn replace_input_plain_enter_requests_replace_current() {
        let mut actions = SearchStripActions::default();
        apply_text_input_shortcut(
            &mut actions,
            TextInputKind::Replace,
            TextInputShortcut::PlainEnter,
        );

        assert!(actions.replace_current_requested);
        assert!(!actions.next_requested);
    }

    #[test]
    fn global_navigation_shortcuts_require_matches() {
        assert!(
            !shortcut_actions(
                action_availability(0, true, false),
                SearchActionShortcut::Next
            )
            .next_requested
        );
        assert!(
            shortcut_actions(
                action_availability(1, true, false),
                SearchActionShortcut::Next
            )
            .next_requested
        );
    }

    #[test]
    fn global_replace_shortcuts_require_allowed_replace_state() {
        assert!(
            !shortcut_actions(
                action_availability(1, false, false),
                SearchActionShortcut::ReplaceAll,
            )
            .replace_all_requested
        );
        assert!(
            shortcut_actions(
                action_availability(1, true, false),
                SearchActionShortcut::ReplaceAll,
            )
            .replace_all_requested
        );
    }

    #[test]
    fn undo_redo_shortcuts_do_not_steal_text_input_focus() {
        assert!(
            !shortcut_actions(
                action_availability(1, true, true),
                SearchActionShortcut::Undo
            )
            .undo_requested
        );
        assert!(
            shortcut_actions(
                action_availability(1, true, false),
                SearchActionShortcut::Redo
            )
            .redo_requested
        );
    }

    fn shortcut_actions(
        availability: SearchActionAvailability,
        shortcut: SearchActionShortcut,
    ) -> SearchStripActions {
        let mut actions = SearchStripActions::default();
        apply_search_action_shortcut(&mut actions, availability, shortcut);
        actions
    }

    fn action_availability(
        match_count: usize,
        replace_allowed: bool,
        text_input_focused: bool,
    ) -> SearchActionAvailability {
        SearchActionAvailability {
            match_count,
            replace_allowed,
            can_undo_text_operation: true,
            can_redo_text_operation: true,
            text_input_focused,
        }
    }

    #[test]
    fn regex_mode_shortcut_toggles_between_modes() {
        assert_eq!(
            toggled_search_mode(SearchMode::PlainText),
            SearchMode::Regex
        );
        assert_eq!(
            toggled_search_mode(SearchMode::Regex),
            SearchMode::PlainText
        );
    }
}
