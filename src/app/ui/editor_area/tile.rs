use crate::app::app_state::ScratchpadApp;
use crate::app::domain::{RenderedLayout, ViewId, WorkspaceTab};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::*;
use crate::app::ui::editor_content::{
    self, EditorContentOutcome, EditorContentStyle, TextEditOptions,
};
use crate::app::ui::tab_drag;
use crate::app::ui::tile_header::{
    self, SplitPreviewOverlay, TileAction, TileHeaderRequest, TileHeaderState,
};
use eframe::egui;

struct TileBodyOutcome {
    changed: bool,
    focused: bool,
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
        handle_tile_click(ui, request, state.actions);
        paint_tile_frame(
            ui,
            request.rect,
            request.is_active,
            app.editor_background_color(),
        );

        let body_outcome = render_tile_body(ui, app, request);
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
                    previous_layout: previous_layout.as_ref(),
                    text_edit: TextEditOptions::new(
                        request_focus,
                        word_wrap,
                        &editor_font_id,
                        text_color,
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
        }
    })
    .inner
}

fn tile_ui_builder(rect: egui::Rect) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .max_rect(rect)
        .layout(egui::Layout::top_down(egui::Align::Min))
}

fn handle_tile_click(ui: &mut egui::Ui, request: TileRenderRequest, actions: &mut Vec<TileAction>) {
    let tile_response = ui.interact(
        request.rect,
        ui.make_persistent_id(("tile", request.tab_index, request.view_id)),
        egui::Sense::click(),
    );
    if tile_response.clicked() {
        actions.push(TileAction::Activate(request.view_id));
    }
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
    let border_color = if is_active {
        egui::Color32::LIGHT_BLUE
    } else {
        border(ui)
    };

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
    if tab_drag::has_tab_drag_for_context(ctx) {
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
    }
}
