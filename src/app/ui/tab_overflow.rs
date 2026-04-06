use crate::app::domain::WorkspaceTab;
use crate::app::theme::*;
use crate::app::chrome::tab_button_sized;
use eframe::egui::{self, Stroke, TextureHandle};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct OverflowMenuOutcome {
    pub(crate) activated_tab: Option<usize>,
    pub(crate) close_requested_tab: Option<usize>,
}

struct OverflowMenuContext<'a> {
    popup_width: f32,
    duplicate_name_counts: &'a HashMap<String, usize>,
    outcome: &'a mut OverflowMenuOutcome,
    overflow_popup_open: &'a mut bool,
}

pub(crate) fn show_overflow_button(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    tabs: &[WorkspaceTab],
    active_tab_index: usize,
    overflow_popup_open: &mut bool,
    duplicate_name_counts: &HashMap<String, usize>,
) -> OverflowMenuOutcome {
    let mut outcome = OverflowMenuOutcome::default();
    let overflow_popup_id = ui.id().with("tab_overflow_popup");
    let overflow_button_response = ui.add_sized(
        [BUTTON_SIZE.x, BUTTON_SIZE.y],
        egui::Button::new(
            egui::RichText::new(egui_phosphor::regular::CARET_DOWN).color(TEXT_PRIMARY),
        )
        .fill(ACTION_BG)
        .stroke(Stroke::new(1.0, BORDER)),
    );

    if overflow_button_response.clicked() {
        *overflow_popup_open = !*overflow_popup_open;
    }

    if *overflow_popup_open {
        let popup_width = TAB_BUTTON_WIDTH;
        let area_response = egui::Area::new(overflow_popup_id)
            .order(egui::Order::Foreground)
            .constrain(true)
            .fixed_pos(overflow_button_response.rect.right_bottom())
            .pivot(egui::Align2::RIGHT_TOP)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_width(popup_width);
                    ui.set_min_width(popup_width);

                    let mut menu = OverflowMenuContext {
                        popup_width,
                        duplicate_name_counts,
                        outcome: &mut outcome,
                        overflow_popup_open,
                    };

                    for (index, tab) in tabs.iter().enumerate() {
                        show_overflow_row(
                            ui,
                            index,
                            tab,
                            active_tab_index == index,
                            &mut menu,
                        );
                    }
                });
            });

        if ctx.input(|input| input.key_pressed(egui::Key::Escape))
            || (overflow_button_response.clicked_elsewhere()
                && !area_response.response.hovered()
                && outcome.close_requested_tab.is_none())
        {
            *overflow_popup_open = false;
        }
    }

    outcome
}

fn show_overflow_row(
    ui: &mut egui::Ui,
    index: usize,
    tab: &WorkspaceTab,
    selected: bool,
    menu: &mut OverflowMenuContext<'_>,
) {
    ui.push_id(index, |ui| {
        let has_duplicate = menu
            .duplicate_name_counts
            .get(&tab.buffer.name)
            .copied()
            .unwrap_or(0)
            > 1;

        let display_name = tab.full_display_name(has_duplicate);

        let (response, close_response, _truncated) =
            tab_button_sized(ui, &display_name, selected, menu.popup_width);

        if response.clicked() {
            menu.outcome.activated_tab = Some(index);
            *menu.overflow_popup_open = false;
        }

        if close_response.clicked() {
            menu.outcome.close_requested_tab = Some(index);
        }
    });
}