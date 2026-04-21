use crate::app::chrome::{tab_button, tab_rename_editor_sized};
use crate::app::ui::tab_drag;
use crate::app::ui::tab_strip::context_menu::attach_tab_context_menu;
use eframe::egui;

pub(crate) struct TabCellProps<'a> {
    pub display_name: &'a str,
    pub tooltip: Option<String>,
    pub can_promote_all_files: bool,
    pub is_active: bool,
    pub is_selected: bool,
    pub pending_scroll_to_active: bool,
    pub width: f32,
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
    ui.push_id(("tab_strip", index), |ui| {
        if app.tab_rename_matches_slot(index) {
            return render_tab_rename_cell(ui, app, index, props);
        }

        let (tab_response, promote_response, close_response, truncated) = tab_button_with_width(
            ui,
            props.display_name,
            props.is_active,
            props.is_selected,
            props.can_promote_all_files,
            props.width,
        );
        let tab_response = maybe_attach_tab_tooltip(tab_response, props.tooltip, truncated);
        let dragged_slots = app.dragged_tab_slots(index);
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

        let modifiers = ui.input(|input| input.modifiers);
        let interaction = if promote_response.is_some_and(|response| response.clicked()) {
            TabInteraction::PromoteAllFiles(index)
        } else if close_response.clicked() {
            TabInteraction::RequestClose(index)
        } else if tab_response.double_clicked() {
            app.select_only_tab_slot(index);
            TabInteraction::BeginRename(index)
        } else if tab_response.clicked() {
            if modifiers.shift {
                app.select_tab_slot_range(index);
                TabInteraction::Activate(index)
            } else if modifiers.command || modifiers.ctrl {
                app.toggle_tab_slot_selection(index);
                TabInteraction::None
            } else {
                app.select_only_tab_slot(index);
                TabInteraction::Activate(index)
            }
        } else {
            TabInteraction::None
        };

        attach_tab_context_menu(&tab_response, app, index);

        TabCellOutcome {
            interaction,
            rect: tab_response.rect,
        }
    })
    .inner
}

fn render_tab_rename_cell(
    ui: &mut egui::Ui,
    app: &mut crate::app::app_state::ScratchpadApp,
    index: usize,
    props: TabCellProps<'_>,
) -> TabCellOutcome {
    let request_focus = app.take_tab_rename_focus_request_for_slot(index);
    let (rect, response) = {
        let draft = app
            .tab_rename_draft_mut()
            .expect("rename draft should exist for matching tab slot");
        tab_rename_editor_sized(
            ui,
            draft,
            props.is_active,
            props.is_selected,
            props.width,
            request_focus,
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
        app.cancel_tab_rename();
    } else if (pressed_enter || response.lost_focus()) && !app.commit_tab_rename() {
        app.request_tab_rename_focus();
    }

    TabCellOutcome {
        interaction: TabInteraction::None,
        rect,
    }
}

fn tab_button_with_width(
    ui: &mut egui::Ui,
    display_name: &str,
    is_active: bool,
    is_selected: bool,
    can_promote_all_files: bool,
    width: f32,
) -> (egui::Response, Option<egui::Response>, egui::Response, bool) {
    if (width - crate::app::theme::TAB_BUTTON_WIDTH).abs() <= f32::EPSILON {
        tab_button(
            ui,
            display_name,
            is_active,
            is_selected,
            can_promote_all_files,
        )
    } else {
        crate::app::chrome::tab_button_sized_with_actions(
            ui,
            display_name,
            is_active,
            is_selected,
            can_promote_all_files,
            width,
        )
    }
}

fn maybe_attach_tab_tooltip(
    tab_response: egui::Response,
    tooltip: Option<String>,
    truncated: bool,
) -> egui::Response {
    if truncated {
        tab_response.on_hover_text(tooltip.unwrap_or_default())
    } else {
        tab_response
    }
}
