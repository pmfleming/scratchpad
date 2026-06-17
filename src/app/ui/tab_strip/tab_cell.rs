use crate::app::app_state::workspace::{accessors as workspace_accessors, display_tabs};
use crate::app::chrome::{TabButtonOptions, tab_button_with_actions, tab_rename_editor_sized};
use crate::app::domain::TabAttentionState;
use crate::app::ui::tab_drag;
use crate::app::ui::tab_strip::context_menu::attach_tab_context_menu;
use crate::app::ui::widget_ids;
use eframe::egui;

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
    app: &mut crate::app::app_state::ScratchpadApp,
    index: usize,
    props: TabCellProps<'_>,
) -> TabCellOutcome {
    widget_ids::surface_scope(ui, ("tab_strip.slot", index), |ui| {
        if workspace_accessors::tab_rename_matches_slot(app, index) {
            return render_tab_rename_cell(ui, app, index, props);
        }

        let (tab_response, promote_response, close_response, truncated) =
            tab_button_with_width(ui, index, &props);
        let tab_response = maybe_attach_tab_tooltip(tab_response, props.tooltip, truncated);
        let dragged_slots = display_tabs::dragged_tab_slots(app, index);
        tab_drag::begin_tab_drag_if_needed(
            ui,
            index,
            &dragged_slots,
            &tab_response,
            &close_response,
        );

        if props.is_active && props.pending_scroll_to_active {
            tab_response.scroll_to_me(Some(egui::Align::Center));
        }

        let context_click = attach_tab_context_menu(&tab_response, app, index);
        let interaction = if context_click.secondary_clicked() {
            TabInteraction::Activate(index)
        } else if promote_response.is_some_and(|response| response.clicked()) {
            TabInteraction::PromoteAllFiles(index)
        } else if close_response.clicked() {
            TabInteraction::RequestClose(index)
        } else if tab_response.double_clicked() {
            display_tabs::select_only_tab_slot(app, index);
            TabInteraction::BeginRename(index)
        } else if tab_response.clicked() {
            primary_click_interaction(ui, app, index)
        } else {
            TabInteraction::None
        };

        TabCellOutcome {
            interaction,
            rect: tab_response.rect,
        }
    })
    .inner
}

fn primary_click_interaction(
    ui: &egui::Ui,
    app: &mut crate::app::app_state::ScratchpadApp,
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
    app: &mut crate::app::app_state::ScratchpadApp,
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
            props.is_active,
            props.is_selected,
            props.width,
            request_focus,
            Some(props.label_font_id.clone()),
        )
    };

    if props.is_active && props.pending_scroll_to_active {
        response.scroll_to_me(Some(egui::Align::Center));
    }

    let pressed_escape = response.has_focus()
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
    let attention_color = props.attention_state.map(attention_color);
    tab_button_with_actions(
        ui,
        ("tab_strip.slot", index),
        props.display_name,
        props.is_active,
        props.is_selected,
        TabButtonOptions::with_actions(props.width, props.can_promote_all_files, attention_color)
            .with_label_font_id(props.label_font_id.clone()),
    )
}

fn attention_color(state: TabAttentionState) -> egui::Color32 {
    match state {
        TabAttentionState::AutoEdit => egui::Color32::from_rgb(230, 132, 46),
        TabAttentionState::Dirty => egui::Color32::from_rgb(70, 176, 96),
        TabAttentionState::DiskProblem => egui::Color32::from_rgb(220, 64, 64),
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
