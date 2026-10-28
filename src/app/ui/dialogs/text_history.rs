mod list;
mod model;
mod navigation;
mod persistence;
mod snapshots;

use self::model::{
    HistoryAction, TextHistoryFileGroup, TextHistoryRow, file_groups_from_entries,
    timeline_rows_from_entries,
};
use self::persistence::{read_active_tab, read_follow_focus, write_active_tab, write_follow_focus};
use super::common::{history_dialog_card, history_dialog_header};
use crate::app::app_state::{ScratchpadApp, workspace::mutation as workspace_mutation};
use crate::app::theme::{action_bg, border, tab_selected_accent, tab_selected_bg};
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;
use egui_phosphor::regular::{CLOCK_COUNTER_CLOCKWISE, CROSSHAIR, FILES, TRASH};

pub(super) const TEXT_HISTORY_SIZE: egui::Vec2 = egui::vec2(
    crate::app::ui::search_replace::SEARCH_DIALOG_WIDTH - 20.0,
    520.0,
);
const TEXT_HISTORY_TITLE_SIZE: f32 = 24.0;
pub(super) const TEXT_HISTORY_LIST_MIN_HEIGHT: f32 = 330.0;
pub(super) const HISTORY_PILL_CORNER_RADIUS: u8 = 8;
pub(super) const HISTORY_PILL_INNER_MARGIN: i8 = 10;
pub(super) const HISTORY_PILL_SPACING: f32 = 6.0;
const TAB_BUTTON_HEIGHT: f32 = 30.0;
const HISTORY_CARD_CORNER_RADIUS: u8 = 12;
const UNDONE_OPACITY: f32 = 0.55;
const HISTORY_TIMELINE_SHORTCUT: &str = "CTRL+1: Timeline";
const HISTORY_BY_FILE_SHORTCUT: &str = "CTRL+2: By file";
const HISTORY_FOLLOW_FOCUS_SHORTCUT: &str = "CTRL+SHIFT+F: Follow restored buffer";
const HISTORY_STOP_FOLLOWING_FOCUS_SHORTCUT: &str = "CTRL+SHIFT+F: Keep current buffer focused";
const HISTORY_CLEAR_SHORTCUT: &str = "CTRL+SHIFT+DELETE: Clear history";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryTab {
    Timeline,
    ByFile,
}

struct TextHistoryWindowInputs<'a> {
    timeline_rows: &'a [TextHistoryRow],
    file_groups: &'a [TextHistoryFileGroup],
    active_tab: HistoryTab,
    follow_focus: bool,
    step_targets: [Option<HistoryAction>; 2],
    snapshots: &'a snapshots::Catalog,
    active_buffer: Option<u64>,
}

struct TextHistoryWindowState<'a> {
    next_tab: &'a mut HistoryTab,
    next_follow_focus: &'a mut bool,
    action: &'a mut Option<HistoryAction>,
    close_requested: &'a mut bool,
    clear_requested: &'a mut bool,
}

/// Consume navigation before the editor sees arrow keys. Browsing moves the real
/// undo boundary, but never writes the resulting text to its source file.
pub(crate) fn handle_text_history_navigation(ctx: &egui::Context, app: &mut ScratchpadApp) {
    if !app.state.dialogs.text_history.is_open()
        || app.state.dialogs.encoding.is_open()
        || app.state.dialogs.status_history.is_open()
        || crate::app::app_state::workspace::accessors::pending_action(app).is_some()
    {
        return;
    }
    for (key, undo) in [(egui::Key::ArrowUp, true), (egui::Key::ArrowDown, false)] {
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key))
            && let Some(action) = snapshots::step(ctx, app, undo)
        {
            snapshots::execute(ctx, app, action);
        }
    }
}

pub(crate) fn show_text_history_window(ctx: &egui::Context, app: &mut ScratchpadApp) {
    snapshots::cleanup(ctx, app);
    if !app.state.dialogs.text_history.is_open() {
        snapshots::cancel_pending(ctx);
        snapshots::cancel_coverage(ctx, app);
        navigation::clear(ctx);
        return;
    }

    let mut action: Option<HistoryAction> = None;
    let mut close_requested = false;
    let mut clear_requested = false;
    let active_tab = read_active_tab(ctx);
    let mut next_tab = active_tab;
    let follow_focus = read_follow_focus(ctx);
    let mut next_follow_focus = follow_focus;

    let screen = ctx.content_rect();
    callout::show_floating(
        ctx,
        "text_history_window",
        "Text history",
        egui::pos2(
            (screen.right() - TEXT_HISTORY_SIZE.x - 24.0).max(screen.left()),
            screen.top() + 72.0,
        ),
        TEXT_HISTORY_SIZE.x,
        |ui| {
            widget_ids::feature_scope(ui, "text_history_dialog", |ui| {
                snapshots::poll(ui, app);
                snapshots::hydrate(ui, app);
                let entries = snapshots::entries(ctx, app);
                let timeline_rows = timeline_rows_from_entries(entries.iter());
                let mut file_groups = file_groups_from_entries(entries.iter());
                snapshots::add_empty_groups(app, &mut file_groups);
                let catalog = snapshots::collect(ui, app);
                let step_targets = [
                    snapshots::step(ctx, app, true),
                    snapshots::step(ctx, app, false),
                ];
                let active_buffer = app
                    .tab_manager
                    .active_tab()
                    .map(|tab| tab.active_buffer().id);
                render_text_history_window(
                    ui,
                    TextHistoryWindowInputs {
                        timeline_rows: &timeline_rows,
                        file_groups: &file_groups,
                        active_tab,
                        follow_focus,
                        step_targets,
                        snapshots: &catalog,
                        active_buffer,
                    },
                    TextHistoryWindowState {
                        next_tab: &mut next_tab,
                        next_follow_focus: &mut next_follow_focus,
                        action: &mut action,
                        close_requested: &mut close_requested,
                        clear_requested: &mut clear_requested,
                    },
                );
            });
        },
    );

    if next_tab != active_tab {
        write_active_tab(ctx, next_tab);
    }
    if next_follow_focus != follow_focus {
        write_follow_focus(ctx, next_follow_focus);
    }
    if close_requested {
        crate::app::commands::close_text_history(&mut app.state.dialogs);
        snapshots::cancel_pending(ctx);
        snapshots::cancel_coverage(ctx, app);
        navigation::clear(ctx);
    }
    if clear_requested {
        let saved = snapshots::clear_history(ctx, app);
        let live = workspace_mutation::clear_text_history(app);
        if saved && !live {
            app.tab_manager.mark_session_dirty();
            app.state.status.set_info_status_in_domain(
                crate::app::app_state::StatusDomain::History,
                "Cleared text history.",
            );
        }
    }
    if let Some(action) = action {
        snapshots::execute(ctx, app, action);
    }
}

fn render_text_history_window(
    ui: &mut egui::Ui,
    inputs: TextHistoryWindowInputs<'_>,
    mut state: TextHistoryWindowState<'_>,
) {
    settings::apply_dialog_typography(ui);
    callout::apply_spacing(ui);
    ui.spacing_mut().item_spacing = egui::vec2(8.0, 12.0);
    handle_history_shortcuts(ui, &inputs, &mut state);
    if render_header(ui) {
        *state.close_requested = true;
    }
    ui.add_space(4.0);
    history_dialog_card(ui, HISTORY_CARD_CORNER_RADIUS, |ui| {
        render_controls(ui, &inputs, &mut state);
    });
    match inputs.active_tab {
        HistoryTab::Timeline => {
            list::render_timeline(ui, inputs.timeline_rows, inputs.snapshots, state.action);
        }
        HistoryTab::ByFile => list::render_by_file(
            ui,
            inputs.file_groups,
            inputs.snapshots,
            inputs.active_buffer,
            state.action,
        ),
    }
}

fn render_header(ui: &mut egui::Ui) -> bool {
    history_dialog_header(
        ui,
        "text_history.header",
        "Close history",
        "History",
        TEXT_HISTORY_TITLE_SIZE,
    )
}

fn render_controls(
    ui: &mut egui::Ui,
    inputs: &TextHistoryWindowInputs<'_>,
    state: &mut TextHistoryWindowState<'_>,
) {
    let active = inputs.active_tab;
    let follow_focus = inputs.follow_focus;
    let tabs = [
        (
            "timeline",
            CLOCK_COUNTER_CLOCKWISE,
            HISTORY_TIMELINE_SHORTCUT,
            HistoryTab::Timeline,
        ),
        (
            "by_file",
            FILES,
            HISTORY_BY_FILE_SHORTCUT,
            HistoryTab::ByFile,
        ),
    ];

    ui.horizontal(|ui| {
        for (id_source, icon, tooltip, tab) in tabs {
            if control_icon_button(ui, id_source, icon, tooltip, active == tab, true).clicked() {
                *state.next_tab = tab;
            }
        }
        if control_icon_button(
            ui,
            "follow_focus",
            CROSSHAIR,
            if follow_focus {
                HISTORY_STOP_FOLLOWING_FOCUS_SHORTCUT
            } else {
                HISTORY_FOLLOW_FOCUS_SHORTCUT
            },
            follow_focus,
            true,
        )
        .clicked()
        {
            *state.next_follow_focus = !follow_focus;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if control_icon_button(
                ui,
                "clear_history",
                TRASH,
                HISTORY_CLEAR_SHORTCUT,
                false,
                !inputs.timeline_rows.is_empty(),
            )
            .clicked()
            {
                *state.clear_requested = true;
            }
            if let Some(action) = navigation::controls(ui, inputs.step_targets) {
                *state.action = Some(action);
            }
        });
    });
}

fn handle_history_shortcuts(
    ui: &mut egui::Ui,
    inputs: &TextHistoryWindowInputs<'_>,
    state: &mut TextHistoryWindowState<'_>,
) {
    if consume_history_shortcut(ui, egui::Modifiers::CTRL, egui::Key::Num1) {
        *state.next_tab = HistoryTab::Timeline;
    }
    if consume_history_shortcut(ui, egui::Modifiers::CTRL, egui::Key::Num2) {
        *state.next_tab = HistoryTab::ByFile;
    }
    if consume_history_shortcut(ui, ctrl_shift_modifiers(), egui::Key::F) {
        *state.next_follow_focus = !inputs.follow_focus;
    }
    if !inputs.timeline_rows.is_empty()
        && consume_history_shortcut(ui, ctrl_shift_modifiers(), egui::Key::Delete)
    {
        *state.clear_requested = true;
    }
}

fn consume_history_shortcut(ui: &mut egui::Ui, modifiers: egui::Modifiers, key: egui::Key) -> bool {
    ui.input_mut(|input| input.consume_key(modifiers, key))
}

fn ctrl_shift_modifiers() -> egui::Modifiers {
    egui::Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    }
}

fn control_icon_button(
    ui: &mut egui::Ui,
    id_source: &'static str,
    icon: &str,
    tooltip: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let (fill, stroke_color, text_color) = if selected {
        (
            tab_selected_bg(ui),
            tab_selected_accent(ui),
            callout::text(ui),
        )
    } else {
        (action_bg(ui), border(ui), callout::muted_text(ui))
    };
    let button = egui::Button::new(
        egui::RichText::new(icon)
            .font(egui_phosphor::font_id(16.0))
            .color(if enabled {
                text_color
            } else {
                text_color.gamma_multiply(UNDONE_OPACITY)
            }),
    )
    .min_size(egui::vec2(36.0, TAB_BUTTON_HEIGHT))
    .fill(fill)
    .stroke(egui::Stroke::new(1.0, stroke_color))
    .corner_radius(egui::CornerRadius::same(8));
    widget_ids::scope(ui, ("text_history.control", id_source), |ui| {
        ui.add_enabled(enabled, button)
    })
    .inner
    .accessible_name(tooltip)
    .on_hover_text(tooltip)
}

pub(super) fn dim_if(undone: bool, color: egui::Color32) -> egui::Color32 {
    if undone {
        color.gamma_multiply(UNDONE_OPACITY)
    } else {
        color
    }
}

pub(super) fn truncated_label(
    ui: &mut egui::Ui,
    text: &str,
    width: f32,
    size: f32,
    color: egui::Color32,
    sense: egui::Sense,
) -> egui::Response {
    ui.add_sized(
        egui::vec2(width, 0.0),
        egui::Label::new(egui::RichText::new(text).size(size).color(color))
            .truncate()
            .show_tooltip_when_elided(false)
            .sense(sense),
    )
}

#[cfg(test)]
mod tests;
