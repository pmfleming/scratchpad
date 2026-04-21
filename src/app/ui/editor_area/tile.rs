use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::SplitAxis;
use crate::app::domain::{RenderedLayout, ViewId, WorkspaceTab};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::*;
use crate::app::ui::editor_content::{
    self, EditorContentOutcome, EditorContentStyle, EditorHighlightStyle, TextEditOptions,
};
use crate::app::ui::tab_drag;
use crate::app::ui::tile_header::{
    self, SplitPreviewOverlay, TileAction, TileHeaderRequest, TileHeaderState,
};
use eframe::egui;
use egui_phosphor::regular::{
    ARROWS_SPLIT, CARET_RIGHT, CLIPBOARD_TEXT, COPY, FLOPPY_DISK, FOLDER_OPEN, MAGNIFYING_GLASS,
    SCISSORS, SELECTION_ALL,
};

const DEFAULT_SPLIT_RATIO: f32 = 0.5;
const EDITOR_CONTEXT_MENU_WIDTH: f32 = 220.0;
const EDITOR_CONTEXT_ICON_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);

struct TileBodyOutcome {
    changed: bool,
    focused: bool,
    interaction_response: Option<egui::Response>,
}

#[derive(Clone, Copy)]
pub(super) struct TileRenderRequest {
    pub(super) tab_index: usize,
    pub(super) view_id: ViewId,
    pub(super) rect: egui::Rect,
    pub(super) is_active: bool,
    pub(super) can_close: bool,
}

pub(super) struct TileRenderState<'a> {
    pub(super) actions: &'a mut Vec<TileAction>,
    pub(super) any_editor_changed: &'a mut bool,
    pub(super) preview_overlay: &'a mut Option<SplitPreviewOverlay>,
}

struct TileScrollRequest<'a> {
    tab_index: usize,
    view_id: ViewId,
    scroll_bar_visibility: egui::scroll_area::ScrollBarVisibility,
    content_style: EditorContentStyle<'a>,
}

pub(super) fn render_tile(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    state: &mut TileRenderState<'_>,
) {
    ui.scope_builder(tile_ui_builder(request.rect), |ui| {
        let tile_response = handle_tile_click(ui, app, request, state.actions);
        paint_tile_frame(
            ui,
            request.rect,
            request.is_active,
            app.editor_background_color(),
        );

        let body_outcome = render_tile_body(ui, app, request);
        let context_menu_response = body_outcome
            .interaction_response
            .as_ref()
            .unwrap_or(&tile_response);
        *state.any_editor_changed |= body_outcome.changed;
        apply_tile_body_focus(
            body_outcome.focused,
            request.is_active,
            request.view_id,
            state.actions,
        );
        tile_header::render_tile_header(
            ui,
            app,
            TileHeaderRequest {
                tab_index: request.tab_index,
                view_id: request.view_id,
                tile_rect: request.rect,
                can_close: request.can_close,
            },
            &mut TileHeaderState {
                actions: state.actions,
                preview_overlay: state.preview_overlay,
            },
        );
        attach_editor_context_menu(context_menu_response, ui, app, request, state.actions);
    });
}

fn render_tile_body(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
) -> TileBodyOutcome {
    ui.scope_builder(tile_ui_builder(request.rect), |ui| {
        let editor_font_id = editor_font_id(app.font_size());
        let request_focus = app.should_focus_view(request.view_id);
        let editor_gutter = app.editor_gutter();
        let word_wrap = app.word_wrap();
        let text_color = app.editor_text_color();
        let background_color = app.editor_background_color();
        let highlight_style = EditorHighlightStyle::new(
            app.editor_text_highlight_color(),
            app.editor_text_highlight_text_color(),
        );
        let tab = &mut app.tabs_mut()[request.tab_index];
        let previous_layout = take_previous_layout(tab, request.view_id);
        let outcome = show_editor_scroll_area(
            ui,
            tab,
            TileScrollRequest {
                tab_index: request.tab_index,
                view_id: request.view_id,
                scroll_bar_visibility: editor_scroll_bar_visibility(ui.ctx()),
                content_style: EditorContentStyle {
                    editor_gutter,
                    is_active: request.is_active,
                    previous_layout: previous_layout.as_ref(),
                    text_edit: TextEditOptions::new(
                        request_focus,
                        word_wrap,
                        &editor_font_id,
                        text_color,
                        highlight_style,
                    ),
                    background_color,
                },
            },
        );
        restore_previous_layout_if_needed(tab, request.view_id, previous_layout);
        if request_focus {
            app.consume_focus_request(request.view_id);
        }

        TileBodyOutcome {
            changed: outcome.changed,
            focused: outcome.focused,
            interaction_response: outcome.interaction_response,
        }
    })
    .inner
}

fn tile_ui_builder(rect: egui::Rect) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .max_rect(rect)
        .layout(egui::Layout::top_down(egui::Align::Min))
}

fn handle_tile_click(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    actions: &mut Vec<TileAction>,
) -> egui::Response {
    let tile_response = ui.interact(
        request.rect,
        ui.make_persistent_id(("tile", request.tab_index, request.view_id)),
        egui::Sense::click(),
    );
    if tile_response.secondary_clicked() && !request.is_active {
        app.activate_view(request.view_id);
        app.request_focus_for_view(request.view_id);
    }
    if tile_response.clicked() {
        actions.push(TileAction::Activate(request.view_id));
    }
    tile_response
}

fn attach_editor_context_menu(
    tile_response: &egui::Response,
    _ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    actions: &mut Vec<TileAction>,
) {
    if tile_response.secondary_clicked() && !request.is_active {
        app.activate_view(request.view_id);
        app.request_focus_for_view(request.view_id);
    }

    let can_promote = app.tabs()[request.tab_index].can_promote_view(request.view_id);
    let save_existing = app.tabs()[request.tab_index]
        .buffer_for_view(request.view_id)
        .is_some_and(|buffer| buffer.path.is_some());

    tile_response.context_menu(|ui| {
        ui.set_min_width(EDITOR_CONTEXT_MENU_WIDTH);

        if menu_action_button(
            ui,
            "Undo",
            None,
            app.active_buffer_can_undo_text_operation(),
        ) {
            app.handle_command(AppCommand::UndoActiveBufferTextOperation);
            app.request_focus_for_active_view();
            ui.close();
        }
        if menu_action_button(
            ui,
            "Redo",
            None,
            app.active_buffer_can_redo_text_operation(),
        ) {
            app.handle_command(AppCommand::RedoActiveBufferTextOperation);
            app.request_focus_for_active_view();
            ui.close();
        }

        ui.separator();

        if menu_action_button(ui, "Find", Some(MAGNIFYING_GLASS), true) {
            app.handle_command(AppCommand::OpenSearch);
            ui.close();
        }
        if menu_action_button(ui, "Replace", None, true) {
            app.handle_command(AppCommand::OpenSearchAndReplace);
            ui.close();
        }
        if menu_action_button(ui, "Open File Here", Some(FOLDER_OPEN), true) {
            app.handle_command(AppCommand::OpenFileHere);
            app.request_focus_for_active_view();
            ui.close();
        }
        if menu_action_button(
            ui,
            if save_existing { "Save" } else { "Save As" },
            Some(FLOPPY_DISK),
            true,
        ) {
            app.request_focus_for_active_view();
            if save_existing {
                app.save_file();
            } else {
                app.save_file_as();
            }
            ui.close();
        }

        ui.separator();

        ui.menu_button(
            egui::RichText::new(format!("{ARROWS_SPLIT}  Split {CARET_RIGHT}")),
            |ui| {
                if split_menu_button(ui, "Split Left") {
                    actions.push(TileAction::Split {
                        axis: SplitAxis::Vertical,
                        new_view_first: true,
                        ratio: DEFAULT_SPLIT_RATIO,
                    });
                    ui.close();
                }
                if split_menu_button(ui, "Split Right") {
                    actions.push(TileAction::Split {
                        axis: SplitAxis::Vertical,
                        new_view_first: false,
                        ratio: DEFAULT_SPLIT_RATIO,
                    });
                    ui.close();
                }
                if split_menu_button(ui, "Split Up") {
                    actions.push(TileAction::Split {
                        axis: SplitAxis::Horizontal,
                        new_view_first: true,
                        ratio: DEFAULT_SPLIT_RATIO,
                    });
                    ui.close();
                }
                if split_menu_button(ui, "Split Down") {
                    actions.push(TileAction::Split {
                        axis: SplitAxis::Horizontal,
                        new_view_first: false,
                        ratio: DEFAULT_SPLIT_RATIO,
                    });
                    ui.close();
                }
            },
        );

        if menu_action_button(ui, "Move Tile To New Tab", None, can_promote) {
            actions.push(TileAction::Promote(request.view_id));
            ui.close();
        }
        if menu_action_button(ui, "Close Tile", None, request.can_close) {
            actions.push(TileAction::Close(request.view_id));
            ui.close();
        }

        ui.separator();
        ui.horizontal(|ui| {
            let copied = icon_rail_button(ui, SCISSORS, "Cut", true).clicked()
                && app.cut_selected_text_in_active_view().is_some_and(|text| {
                    ui.copy_text(text);
                    true
                });
            let copied_selection = icon_rail_button(ui, COPY, "Copy", true).clicked()
                && app.copy_selected_text_in_active_view().is_some_and(|text| {
                    ui.copy_text(text);
                    true
                });
            let pasted = icon_rail_button(ui, CLIPBOARD_TEXT, "Paste", true)
                .clicked()
                .then(|| {
                    app.request_focus_for_active_view();
                    ui.ctx()
                        .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                });
            let selected_all = icon_rail_button(ui, SELECTION_ALL, "Select All", true).clicked()
                && app.select_all_in_active_view();

            if copied || copied_selection || pasted.is_some() || selected_all {
                app.request_focus_for_active_view();
                ui.close();
            }
        });
    });
}

fn menu_action_button(ui: &mut egui::Ui, label: &str, icon: Option<&str>, enabled: bool) -> bool {
    let text = match icon {
        Some(icon) => format!("{icon}  {label}"),
        None => label.to_owned(),
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).color(text_primary(ui)))
            .min_size(egui::vec2(ui.available_width(), 28.0))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE),
    )
    .clicked()
}

fn split_menu_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(text_primary(ui)))
            .min_size(egui::vec2(160.0, 28.0))
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::NONE),
    )
    .clicked()
}

fn icon_rail_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, enabled: bool) -> egui::Response {
    let button = egui::Button::new(
        egui::RichText::new(icon)
            .font(egui::FontId::proportional(17.0))
            .color(text_primary(ui)),
    )
    .min_size(EDITOR_CONTEXT_ICON_BUTTON_SIZE)
    .fill(action_hover_bg(ui))
    .stroke(egui::Stroke::new(1.0, border(ui)))
    .corner_radius(egui::CornerRadius::same(8));

    ui.add_enabled(enabled, button).on_hover_text(tooltip)
}

fn paint_tile_frame(
    ui: &egui::Ui,
    rect: egui::Rect,
    is_active: bool,
    background_color: egui::Color32,
) {
    let bg = if is_active {
        header_bg(ui)
    } else {
        background_color
    };
    let border_color = border(ui).gamma_multiply(0.0);

    ui.painter().rect_filled(rect, 4.0, bg);
    ui.painter().rect_stroke(
        rect,
        4.0,
        egui::Stroke::new(1.0, border_color),
        egui::StrokeKind::Inside,
    );
}

fn apply_tile_body_focus(
    body_focused: bool,
    is_active: bool,
    view_id: ViewId,
    actions: &mut Vec<TileAction>,
) {
    if body_focused && !is_active {
        actions.push(TileAction::Activate(view_id));
    }
}

fn editor_font_id(font_size: f32) -> egui::FontId {
    egui::FontId::new(font_size, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()))
}

fn editor_scroll_bar_visibility(ctx: &egui::Context) -> egui::scroll_area::ScrollBarVisibility {
    if tab_drag::is_drag_active_for_context(ctx) {
        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
    } else {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    }
}

fn take_previous_layout(tab: &mut WorkspaceTab, view_id: ViewId) -> Option<RenderedLayout> {
    tab.view_mut(view_id)
        .and_then(|view| view.latest_layout.take())
}

fn show_editor_scroll_area(
    ui: &mut egui::Ui,
    tab: &mut WorkspaceTab,
    request: TileScrollRequest<'_>,
) -> EditorContentOutcome {
    egui::ScrollArea::both()
        .id_salt(("editor_scroll", request.tab_index, request.view_id))
        .auto_shrink([false, false])
        .scroll_bar_visibility(request.scroll_bar_visibility)
        .show(ui, |ui| {
            tab.buffer_and_view_mut(request.view_id)
                .map(|(buffer, view)| {
                    editor_content::render_editor_content(ui, buffer, view, request.content_style)
                })
                .unwrap_or_else(missing_editor_content_outcome)
        })
        .inner
}

fn restore_previous_layout_if_needed(
    tab: &mut WorkspaceTab,
    view_id: ViewId,
    previous_layout: Option<RenderedLayout>,
) {
    if tab
        .view(view_id)
        .is_some_and(|view| view.latest_layout.is_none())
        && let Some(view) = tab.view_mut(view_id)
    {
        view.latest_layout = previous_layout;
    }
}

fn missing_editor_content_outcome() -> EditorContentOutcome {
    EditorContentOutcome {
        changed: false,
        focused: false,
        interaction_response: None,
    }
}
