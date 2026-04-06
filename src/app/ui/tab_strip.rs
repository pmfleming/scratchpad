use crate::app::app_state::ScratchpadApp;
use crate::app::chrome::*;
use crate::app::commands::AppCommand;
use crate::app::domain::WorkspaceTab;
use crate::app::theme::*;
use crate::app::ui::tab_overflow;
use eframe::egui::{self, Sense, Stroke, TextureHandle};
use std::collections::HashMap;

pub(crate) fn show_header(ctx: &egui::Context, app: &mut ScratchpadApp) {
    egui::TopBottomPanel::top("header")
        .exact_height(HEADER_HEIGHT)
        .frame(
            egui::Frame::none()
                .fill(HEADER_BG)
                .stroke(Stroke::new(1.0, BORDER))
                .inner_margin(egui::Margin {
                    left: HEADER_LEFT_PADDING,
                    right: HEADER_RIGHT_PADDING,
                    top: HEADER_VERTICAL_PADDING,
                    bottom: HEADER_VERTICAL_PADDING,
                }),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                show_primary_actions(ui, app);

                ui.add_space(8.0);
                let layout =
                    HeaderLayout::measure(app, ui.available_width(), 4.0);
                let outcome = show_tab_region(ctx, ui, app, &layout);

                ui.add_space(8.0);
                show_caption_controls(ctx, ui, app, &layout);
                apply_tab_outcome(app, outcome);
            });
        });
}

struct HeaderLayout {
    spacing: f32,
    caption_controls_width: f32,
    has_overflow: bool,
    visible_strip_width: f32,
    drag_width: f32,
    tab_area_width: f32,
}

impl HeaderLayout {
    fn measure(app: &ScratchpadApp, remaining_width: f32, spacing: f32) -> Self {
        let caption_controls_width =
            CAPTION_BUTTON_SIZE.x * 3.0 + CAPTION_BUTTON_SPACING * 2.0 + CAPTION_TRAILING_PADDING;
        let tab_action_width = BUTTON_SIZE.x;
        let overflow_button_width = BUTTON_SIZE.x;
        let spacer_before_captions = 8.0;

        let viewport_width_with_overflow = (remaining_width
            - caption_controls_width
            - spacer_before_captions
            - tab_action_width
            - spacing
            - overflow_button_width
            - spacing)
            .max(0.0);
        let total_tab_width = app.estimated_tab_strip_width(spacing);
        let has_overflow = total_tab_width > viewport_width_with_overflow;
        let viewport_width = (remaining_width
            - caption_controls_width
            - spacer_before_captions
            - tab_action_width
            - spacing
            - if has_overflow {
                overflow_button_width + spacing
            } else {
                0.0
            })
        .max(0.0);
        let visible_strip_width = total_tab_width.min(viewport_width);
        let drag_width = (viewport_width - visible_strip_width).max(0.0);
        let tab_area_width =
            (remaining_width - caption_controls_width - spacer_before_captions).max(0.0);

        Self {
            spacing,
            caption_controls_width,
            has_overflow,
            visible_strip_width,
            drag_width,
            tab_area_width,
        }
    }
}

#[derive(Default)]
struct TabStripOutcome {
    activated_tab: Option<usize>,
    close_requested_tab: Option<usize>,
    consumed_scroll_request: bool,
}

fn show_primary_actions(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let button_spacing = 4.0;
    if phosphor_button(
        ui,
        egui_phosphor::regular::FOLDER_OPEN,
        BUTTON_SIZE,
        ACTION_BG,
        ACTION_HOVER_BG,
        "Open File",
    )
    .clicked()
    {
        app.handle_command(AppCommand::OpenFile);
    }

    ui.add_space(button_spacing);
    if phosphor_button(
        ui,
        egui_phosphor::regular::FLOPPY_DISK,
        BUTTON_SIZE,
        ACTION_BG,
        ACTION_HOVER_BG,
        "Save As",
    )
    .clicked()
    {
        app.handle_command(AppCommand::SaveFileAs);
    }

    ui.add_space(button_spacing);
    if phosphor_button(
        ui,
        egui_phosphor::regular::MAGNIFYING_GLASS,
        BUTTON_SIZE,
        ACTION_BG,
        ACTION_HOVER_BG,
        "Search",
    )
    .clicked()
    {
        app.status_message = Some("Search is not implemented yet.".to_owned());
    }
}

fn show_tab_region(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) -> TabStripOutcome {
    let duplicate_name_counts = duplicate_name_counts(&app.tabs);
    let mut outcome = TabStripOutcome::default();

    ui.allocate_ui_with_layout(
        egui::vec2(layout.tab_area_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            if layout.visible_strip_width > 0.0 {
                show_scrolling_tab_strip(ui, app, layout, &duplicate_name_counts, &mut outcome);
            }

            if layout.has_overflow || app.overflow_popup_open {
                show_overflow_controls(ctx, ui, app, layout, &duplicate_name_counts, &mut outcome);
            }

            ui.add_space(layout.spacing);
            if phosphor_button(
                ui,
                egui_phosphor::regular::PLUS,
                BUTTON_SIZE,
                ACTION_BG,
                ACTION_HOVER_BG,
                "New Tab",
            )
            .clicked()
            {
                app.handle_command(AppCommand::NewTab);
            }

            show_drag_region(ctx, ui, layout.drag_width);
        },
    );

    outcome
}

fn show_scrolling_tab_strip(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) {
    ui.allocate_ui_with_layout(
        egui::vec2(layout.visible_strip_width, TAB_HEIGHT),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_width(layout.visible_strip_width);
            ui.set_min_width(layout.visible_strip_width);
            ui.set_max_width(layout.visible_strip_width);

            egui::ScrollArea::horizontal()
                .id_source("tab_strip")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = layout.spacing;
                    ui.horizontal(|ui| {
                        for (index, tab) in app.tabs.iter().enumerate() {
                            let is_active = app.active_tab_index == index;
                            let mut clicked = false;
                            let mut closed = false;

                            ui.push_id(index, |ui| {
                                let has_duplicate = duplicate_name_counts
                                    .get(&tab.buffer.name)
                                    .copied()
                                    .unwrap_or(0)
                                    > 1;
                                let display_name = tab.full_display_name(has_duplicate);

                                let (tab_response, close_response, truncated) =
                                    tab_button(ui, &display_name, is_active);

                                let tab_response = if truncated {
                                    tab_response.on_hover_text(tab.display_name())
                                } else {
                                    tab_response
                                };

                                if is_active && app.pending_scroll_to_active {
                                    tab_response.scroll_to_me(Some(egui::Align::Center));
                                    outcome.consumed_scroll_request = true;
                                }

                                if tab_response.clicked() {
                                    clicked = true;
                                }

                                if close_response.clicked() {
                                    closed = true;
                                }
                            });

                            if clicked {
                                outcome.activated_tab = Some(index);
                            }

                            if closed {
                                outcome.close_requested_tab = Some(index);
                            }
                        }
                    });
                });
        },
    );
}

fn show_overflow_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
    duplicate_name_counts: &HashMap<String, usize>,
    outcome: &mut TabStripOutcome,
) {
    ui.add_space(layout.spacing);
    let overflow_outcome = tab_overflow::show_overflow_button(
        ctx,
        ui,
        &app.tabs,
        app.active_tab_index,
        &mut app.overflow_popup_open,
        duplicate_name_counts,
    );

    outcome.activated_tab = outcome.activated_tab.or(overflow_outcome.activated_tab);
    outcome.close_requested_tab = outcome
        .close_requested_tab
        .or(overflow_outcome.close_requested_tab);
}

fn show_drag_region(ctx: &egui::Context, ui: &mut egui::Ui, drag_width: f32) {
    if drag_width <= 0.0 {
        return;
    }

    let (rect, drag_response) =
        ui.allocate_exact_size(egui::vec2(drag_width, TAB_HEIGHT), Sense::click_and_drag());
    if drag_response.drag_started() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if drag_response.double_clicked() {
        let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    ui.painter().rect_filled(rect, 0.0, HEADER_BG);
}

fn show_caption_controls(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    layout: &HeaderLayout,
) {
    if caption_controls(
        ui,
        ctx,
        layout.caption_controls_width,
    ) {
        app.request_exit(ctx);
    }
}

fn apply_tab_outcome(app: &mut ScratchpadApp, outcome: TabStripOutcome) {
    if let Some(index) = outcome.activated_tab {
        app.handle_command(AppCommand::ActivateTab { index });
    }

    if let Some(index) = outcome.close_requested_tab {
        app.handle_command(AppCommand::RequestCloseTab { index });
    }

    if outcome.consumed_scroll_request {
        app.pending_scroll_to_active = false;
    }
}

fn duplicate_name_counts(tabs: &[WorkspaceTab]) -> HashMap<String, usize> {
    let mut counts = HashMap::with_capacity(tabs.len());
    for tab in tabs {
        *counts.entry(tab.buffer.name.clone()).or_insert(0) += 1;
    }
    counts
}
