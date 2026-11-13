use crate::app::app_state::ScratchpadApp;
use crate::app::app_state::workspace::{accessors as workspace_accessors, display_tabs};
use crate::app::chrome::{
    TabButtonOptions, TabRenameEditorOptions, tab_button_with_actions, tab_rename_editor_sized,
};
use crate::app::domain::buffer::StorageAttention;
use crate::app::domain::{TabAttentionState, WorkspaceTab};
use crate::app::ui::tab_drag;
use crate::app::ui::tab_strip::context_menu::attach_tab_context_menu;
use crate::app::ui::widget_ids;
use eframe::egui;
use std::collections::HashMap;
use std::fmt::Write as _;

pub(crate) struct TabPresentation {
    pub display_name: String,
    pub tooltip: String,
    pub can_promote_all_files: bool,
    pub attention_state: Option<TabAttentionState>,
}

pub(crate) fn tab_presentation(
    app: &ScratchpadApp,
    slot_index: usize,
    duplicate_name_counts: &HashMap<String, usize>,
) -> Option<TabPresentation> {
    if display_tabs::tab_slot_is_settings(app, slot_index) {
        return Some(TabPresentation {
            display_name: "Settings".to_owned(),
            tooltip: "Settings".to_owned(),
            can_promote_all_files: false,
            attention_state: None,
        });
    }

    let workspace_index = display_tabs::workspace_index_for_slot(app, slot_index)?;
    let tab = app.tab_manager.tabs.as_slice().get(workspace_index)?;
    let has_duplicate = duplicate_name_counts
        .get(&tab.buffers.buffer.name)
        .copied()
        .unwrap_or(0)
        > 1;
    let display_name = crate::app::domain::tab::summary::full_display_name(tab, has_duplicate);
    Some(TabPresentation {
        tooltip: storage_attention_tooltip(&display_name, tab),
        display_name,
        can_promote_all_files: crate::app::domain::tab::summary::can_promote_all_files(tab),
        attention_state: crate::app::domain::tab::summary::attention_state(tab),
    })
}

/// The tab name followed by one line per buffer whose storage needs attention.
fn storage_attention_tooltip(display_name: &str, tab: &WorkspaceTab) -> String {
    let mut tooltip = display_name.to_owned();
    for buffer in tab.buffers() {
        let cause = match &buffer.storage_attention {
            Some(StorageAttention::Unsaved) => {
                "Changes are in memory only; session is not saving.".to_owned()
            }
            Some(StorageAttention::Unreadable(cause)) => format!("Text is unreadable: {cause}"),
            None => continue,
        };
        let _ = write!(tooltip, "\n{}: {cause}", buffer.display_name());
    }
    tooltip
}

pub(crate) struct TabCellProps<'a> {
    pub display_name: &'a str,
    pub tooltip: Option<String>,
    pub can_promote_all_files: bool,
    pub attention_state: Option<TabAttentionState>,
    pub is_active: bool,
    pub is_selected: bool,
    pub pending_scroll_to_active: bool,
    pub width: f32,
    pub label_font_id: egui::FontId,
    pub row_height: f32,
}

pub(crate) struct TabCellOutcome {
    pub interaction: TabInteraction,
    pub rect: egui::Rect,
}

#[derive(Clone, Copy)]
pub(crate) enum TabInteraction {
    None,
    Activate(usize),
    BeginRename(usize),
    PromoteAllFiles(usize),
    RequestClose(usize),
}

pub(crate) fn render_tab_cell_sized(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    index: usize,
    props: TabCellProps<'_>,
) -> TabCellOutcome {
    widget_ids::surface_scope(ui, ("tab_strip.slot", index), |ui| {
        if workspace_accessors::tab_rename_matches_slot(app, index) {
            render_tab_rename_cell(ui, app, index, props)
        } else {
            render_tab_button_cell(ui, app, index, props)
        }
    })
    .inner
}

fn render_tab_button_cell(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    index: usize,
    props: TabCellProps<'_>,
) -> TabCellOutcome {
    let (tab_response, promote_response, close_response, truncated) =
        tab_button_with_width(ui, index, &props);
    let tab_response = maybe_attach_tab_tooltip(tab_response, props.tooltip, truncated);
    let dragged_slots = display_tabs::dragged_tab_slots(app, index);
    tab_drag::begin_tab_drag_if_needed(ui, index, &dragged_slots, &tab_response, &close_response);

    if props.is_active && props.pending_scroll_to_active {
        tab_response.scroll_to_me(Some(egui::Align::Center));
    }

    let clicks = TabClicks {
        context: attach_tab_context_menu(&tab_response, app, index).secondary_clicked(),
        promote: promote_response.is_some_and(|response| response.clicked()),
        close: close_response.clicked(),
        double: tab_response.double_clicked(),
        primary: tab_response.clicked(),
    };
    TabCellOutcome {
        interaction: clicks.interaction(ui, app, index),
        rect: tab_response.rect,
    }
}

/// This frame's clicks on one tab button.
struct TabClicks {
    context: bool,
    promote: bool,
    close: bool,
    double: bool,
    primary: bool,
}

impl TabClicks {
    /// The first click by priority wins; selection changes happen here.
    fn interaction(&self, ui: &egui::Ui, app: &mut ScratchpadApp, index: usize) -> TabInteraction {
        if self.context {
            TabInteraction::Activate(index)
        } else if self.promote {
            TabInteraction::PromoteAllFiles(index)
        } else if self.close {
            TabInteraction::RequestClose(index)
        } else if self.double {
            display_tabs::select_only_tab_slot(app, index);
            TabInteraction::BeginRename(index)
        } else if self.primary {
            primary_click_interaction(ui, app, index)
        } else {
            TabInteraction::None
        }
    }
}

fn primary_click_interaction(
    ui: &egui::Ui,
    app: &mut ScratchpadApp,
    index: usize,
) -> TabInteraction {
    let modifiers = ui.input(|input| input.modifiers);
    if modifiers.shift {
        display_tabs::select_tab_slot_range(app, index);
        TabInteraction::Activate(index)
    } else if modifiers.command || modifiers.ctrl {
        display_tabs::toggle_tab_slot_selection(app, index);
        TabInteraction::None
    } else {
        display_tabs::select_only_tab_slot(app, index);
        TabInteraction::Activate(index)
    }
}

fn render_tab_rename_cell(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    index: usize,
    props: TabCellProps<'_>,
) -> TabCellOutcome {
    let request_focus = workspace_accessors::take_tab_rename_focus_request_for_slot(app, index);
    let (rect, response) = {
        let draft = workspace_accessors::tab_rename_draft_mut(app)
            .expect("rename draft should exist for matching tab slot");
        tab_rename_editor_sized(
            ui,
            ("tab_strip.slot", index),
            draft,
            TabRenameEditorOptions::new(props.is_active, props.is_selected, props.width)
                .with_focus_request(request_focus)
                .with_label_font_id(props.label_font_id.clone())
                .with_row_height(props.row_height),
        )
    };

    if props.is_active && props.pending_scroll_to_active {
        response.scroll_to_me(Some(egui::Align::Center));
    }

    // TextEdit can surrender focus on Escape before returning its response.
    let pressed_escape = (response.has_focus() || response.lost_focus())
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let pressed_enter = response.has_focus()
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));

    if pressed_escape {
        workspace_accessors::cancel_tab_rename(app);
    } else if (pressed_enter || response.lost_focus())
        && !workspace_accessors::commit_tab_rename(app)
    {
        workspace_accessors::request_tab_rename_focus(app);
    }

    TabCellOutcome {
        interaction: TabInteraction::None,
        rect,
    }
}

fn tab_button_with_width(
    ui: &mut egui::Ui,
    index: usize,
    props: &TabCellProps<'_>,
) -> (egui::Response, Option<egui::Response>, egui::Response, bool) {
    let attention_marker = props.attention_state.map(attention_marker);
    tab_button_with_actions(
        ui,
        ("tab_strip.slot", index),
        props.display_name,
        props.is_active,
        props.is_selected,
        TabButtonOptions::with_actions(props.width, props.can_promote_all_files, attention_marker)
            .with_label_font_id(props.label_font_id.clone())
            .with_row_height(props.row_height),
    )
}

pub(crate) fn attention_marker(state: TabAttentionState) -> crate::app::chrome::TabAttention {
    crate::app::chrome::TabAttention {
        circle: state == TabAttentionState::StorageProblem,
        color: match state {
            TabAttentionState::AutoEdit => egui::Color32::from_rgb(230, 132, 46),
            TabAttentionState::Dirty => egui::Color32::from_rgb(70, 176, 96),
            TabAttentionState::StorageProblem | TabAttentionState::DiskProblem => {
                egui::Color32::from_rgb(220, 64, 64)
            }
        },
    }
}

fn maybe_attach_tab_tooltip(
    tab_response: egui::Response,
    tooltip: Option<String>,
    truncated: bool,
) -> egui::Response {
    if tooltip.is_some() || truncated {
        tab_response.on_hover_text(tooltip.unwrap_or_default())
    } else {
        tab_response
    }
}
