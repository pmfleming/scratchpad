use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::SplitAxis;
use crate::app::domain::{RenderedLayout, ViewId, WorkspaceTab};
use crate::app::fonts::EDITOR_FONT_FAMILY;
use crate::app::theme::*;
use crate::app::ui::autoscroll::{AutoScrollAxis, AutoScrollConfig, edge_auto_scroll_delta};
use crate::app::ui::editor_content::{
    self, EditorContentOutcome, EditorContentStyle, EditorHighlightStyle, TextEditOptions,
};
use crate::app::ui::tab_drag;
use crate::app::ui::tile_header::{
    self, SplitPreviewOverlay, TileAction, TileHeaderRequest, TileHeaderState,
};
use crate::app::ui::widget_ids;
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_CLOCKWISE, ARROW_COUNTER_CLOCKWISE, ARROW_DOWN, ARROW_LEFT, ARROW_LINE_UP, ARROW_RIGHT,
    ARROW_UP, ARROWS_COUNTER_CLOCKWISE, ARROWS_SPLIT, CARET_RIGHT, CLIPBOARD_TEXT,
    CLOCK_COUNTER_CLOCKWISE, COPY, FLOPPY_DISK, FOLDER_OPEN, MAGNIFYING_GLASS, SCISSORS,
    SELECTION_ALL, X,
};

const DEFAULT_SPLIT_RATIO: f32 = 0.5;
const EDITOR_CONTEXT_MENU_WIDTH: f32 = 192.0;
const EDITOR_CONTEXT_SUBMENU_WIDTH: f32 = 168.0;
const EDITOR_CONTEXT_ICON_BUTTON_SIZE: egui::Vec2 = egui::vec2(38.0, 30.0);
const EDITOR_CONTEXT_CARET_WIDTH: f32 = 28.0;
const EDITOR_SELECTION_AUTOSCROLL_CONFIG: AutoScrollConfig = AutoScrollConfig {
    edge_extent: 36.0,
    max_step: 18.0,
    cross_axis_margin: 12.0,
};

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
        let request_focus = app.should_focus_view(request.view_id);
        let editor_font_id = editor_font_id(app.font_size());
        let content_style =
            editor_content_style(app, request.is_active, request_focus, &editor_font_id);
        let tab = &mut app.tabs_mut()[request.tab_index];
        let Some(_buffer) = tab.buffer_for_view(request.view_id) else {
            return TileBodyOutcome {
                changed: false,
                focused: false,
                interaction_response: None,
            };
        };
        let previous_layout = take_previous_layout(tab, request.view_id);
        let outcome = show_editor_scroll_area(
            ui,
            tab,
            TileScrollRequest {
                view_id: request.view_id,
                scroll_bar_visibility: editor_scroll_bar_visibility(ui.ctx()),
                content_style: EditorContentStyle {
                    previous_layout: previous_layout.as_ref(),
                    ..content_style
                },
            },
        );
        restore_previous_layout_if_needed(tab, request.view_id, previous_layout);
        apply_tile_focus_request(
            app,
            request.view_id,
            request_focus,
            outcome.request_editor_focus,
        );

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
        widget_ids::local(ui, ("tile", request.tab_index, request.view_id)),
        egui::Sense::click(),
    );
    activate_inactive_tile_on_secondary_click(app, &tile_response, request);
    if tile_response.clicked() {
        actions.push(TileAction::Activate(request.view_id));
    }
    tile_response
}

fn activate_inactive_tile_on_secondary_click(
    app: &mut ScratchpadApp,
    tile_response: &egui::Response,
    request: TileRenderRequest,
) {
    if tile_response.secondary_clicked() && !request.is_active {
        app.activate_view(request.view_id);
        app.request_focus_for_view(request.view_id);
    }
}

fn attach_editor_context_menu(
    tile_response: &egui::Response,
    _ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    request: TileRenderRequest,
    actions: &mut Vec<TileAction>,
) {
    activate_inactive_tile_on_secondary_click(app, tile_response, request);

    let can_promote = app.tabs()[request.tab_index].can_promote_view(request.view_id);
    let save_existing = app.tabs()[request.tab_index]
        .buffer_for_view(request.view_id)
        .is_some_and(|buffer| buffer.path.is_some());
    tile_response.context_menu(|ui| {
        set_menu_width(ui, EDITOR_CONTEXT_MENU_WIDTH);
        render_history_menu(ui, app);
        ui.separator();
        render_file_menu(ui, app, save_existing);
        ui.separator();
        render_tile_menu(ui, actions, request, can_promote);
        ui.separator();
        render_icon_rail_menu(ui, app);
    });
}

fn set_menu_width(ui: &mut egui::Ui, width: f32) {
    ui.set_min_width(width);
    ui.set_max_width(width);
}

fn render_history_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    run_menu_command(
        ui,
        app,
        "Undo",
        Some(ARROW_COUNTER_CLOCKWISE),
        app.active_buffer_can_undo_text_operation(),
        AppCommand::UndoActiveBufferTextOperation,
        true,
    );
    run_menu_command(
        ui,
        app,
        "Redo",
        Some(ARROW_CLOCKWISE),
        app.active_buffer_can_redo_text_operation(),
        AppCommand::RedoActiveBufferTextOperation,
        true,
    );
    run_menu_command(
        ui,
        app,
        "History",
        Some(CLOCK_COUNTER_CLOCKWISE),
        true,
        AppCommand::OpenHistory,
        false,
    );
}

fn render_file_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp, save_existing: bool) {
    run_menu_command(
        ui,
        app,
        "Find",
        Some(MAGNIFYING_GLASS),
        true,
        AppCommand::OpenSearch,
        false,
    );
    run_menu_command(
        ui,
        app,
        "Replace",
        Some(ARROWS_COUNTER_CLOCKWISE),
        true,
        AppCommand::OpenSearchAndReplace,
        false,
    );
    run_menu_command(
        ui,
        app,
        "Open File Here",
        Some(FOLDER_OPEN),
        true,
        AppCommand::OpenFileHere,
        true,
    );
    run_save_menu_action(ui, app, save_existing);
}

fn render_tile_menu(
    ui: &mut egui::Ui,
    actions: &mut Vec<TileAction>,
    request: TileRenderRequest,
    can_promote: bool,
) {
    split_menu_row(ui, actions);
    if menu_action_button(ui, "Promote Tile", Some(ARROW_LINE_UP), can_promote) {
        actions.push(TileAction::Promote(request.view_id));
        ui.close();
    }
    if menu_action_button(ui, "Close Tile", Some(X), request.can_close) {
        actions.push(TileAction::Close(request.view_id));
        ui.close();
    }
}

fn render_icon_rail_menu(ui: &mut egui::Ui, app: &mut ScratchpadApp) {
    let any_action = ui
        .with_layout(
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.horizontal(|ui| {
                    run_icon_rail_action(ui, app, SCISSORS, "Cut", |ui, app| {
                        copy_icon_text(ui, app.cut_selected_text_in_active_view())
                    }) || run_icon_rail_action(ui, app, COPY, "Copy", |ui, app| {
                        copy_icon_text(ui, app.copy_selected_text_in_active_view())
                    }) || run_icon_rail_action(ui, app, CLIPBOARD_TEXT, "Paste", |ui, _| {
                        ui.ctx()
                            .clone()
                            .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
                        true
                    }) || run_icon_rail_action(ui, app, SELECTION_ALL, "Select All", |_, app| {
                        app.select_all_in_active_view()
                    })
                })
                .inner
            },
        )
        .inner;

    if any_action {
        app.request_focus_for_active_view();
        ui.close();
    }
}

fn run_menu_command(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
    command: AppCommand,
    request_focus: bool,
) -> bool {
    run_context_menu_action(
        ui,
        label,
        icon,
        enabled,
        |_, app| {
            app.handle_command(command);
            if request_focus {
                app.request_focus_for_active_view();
            }
            true
        },
        app,
    )
}

fn icon_action_clicked(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> bool {
    icon_rail_button(ui, icon, tooltip, true).clicked()
}

fn run_save_menu_action(ui: &mut egui::Ui, app: &mut ScratchpadApp, save_existing: bool) -> bool {
    run_context_menu_action(
        ui,
        if save_existing { "Save" } else { "Save As" },
        Some(FLOPPY_DISK),
        true,
        |_, app| {
            app.request_focus_for_active_view();
            if save_existing {
                app.save_file();
            } else {
                app.save_file_as();
            }
            true
        },
        app,
    )
}

fn run_context_menu_action(
    ui: &mut egui::Ui,
    label: &str,
    icon: Option<&str>,
    enabled: bool,
    action: impl FnOnce(&mut egui::Ui, &mut ScratchpadApp) -> bool,
    app: &mut ScratchpadApp,
) -> bool {
    if !menu_action_button(ui, label, icon, enabled) {
        return false;
    }

    let handled = action(ui, app);
    if handled {
        ui.close();
    }
    handled
}

fn run_icon_rail_action(
    ui: &mut egui::Ui,
    app: &mut ScratchpadApp,
    icon: &str,
    tooltip: &str,
    action: impl FnOnce(&mut egui::Ui, &mut ScratchpadApp) -> bool,
) -> bool {
    icon_action_clicked(ui, icon, tooltip) && action(ui, app)
}

fn copy_icon_text(ui: &mut egui::Ui, text: Option<String>) -> bool {
    text.is_some_and(|text| {
        ui.copy_text(text);
        true
    })
}

fn menu_action_button(ui: &mut egui::Ui, label: &str, icon: Option<&str>, enabled: bool) -> bool {
    let text = match icon {
        Some(icon) => format!("{icon}  {label}"),
        None => label.to_owned(),
    };
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(text).color(text_primary(ui)))
                .min_size(egui::vec2(EDITOR_CONTEXT_MENU_WIDTH, 28.0))
                .stroke(egui::Stroke::NONE),
        )
        .clicked()
    })
}

fn split_menu_row(ui: &mut egui::Ui, actions: &mut Vec<TileAction>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;

        let split_clicked = render_split_primary_button(ui);
        render_split_submenu(ui, actions);

        if split_clicked {
            queue_split_action(actions, SplitDirection::Right);
        }
    });
}

fn split_menu_button(ui: &mut egui::Ui, label: &str, icon: &str) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        ui.add(
            egui::Button::new(
                egui::RichText::new(format!("{icon}  {label}")).color(text_primary(ui)),
            )
            .min_size(egui::vec2(EDITOR_CONTEXT_SUBMENU_WIDTH, 28.0))
            .stroke(egui::Stroke::NONE),
        )
        .clicked()
    })
}

fn render_split_primary_button(ui: &mut egui::Ui) -> bool {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let response = ui.add(
            egui::Button::new("")
                .min_size(egui::vec2(
                    EDITOR_CONTEXT_MENU_WIDTH - EDITOR_CONTEXT_CARET_WIDTH,
                    28.0,
                ))
                .stroke(egui::Stroke::NONE),
        );
        ui.painter().text(
            response.rect.left_center() + egui::vec2(10.0, 0.0),
            egui::Align2::LEFT_CENTER,
            format!("{ARROWS_SPLIT}  Split"),
            egui::TextStyle::Button.resolve(ui.style()),
            text_primary(ui),
        );
        response.clicked()
    })
}

fn render_split_submenu(ui: &mut egui::Ui, actions: &mut Vec<TileAction>) {
    with_visual_overrides(ui, apply_context_menu_row_hover_style, |ui| {
        let button = egui::Button::new(egui::RichText::new(CARET_RIGHT).color(text_primary(ui)))
            .min_size(egui::vec2(EDITOR_CONTEXT_CARET_WIDTH, 28.0))
            .stroke(egui::Stroke::NONE);

        egui::containers::menu::SubMenuButton::from_button(button).ui(ui, |ui| {
            set_menu_width(ui, EDITOR_CONTEXT_SUBMENU_WIDTH);

            for (label, icon, direction) in [
                ("Split Left", ARROW_LEFT, SplitDirection::Left),
                ("Split Right", ARROW_RIGHT, SplitDirection::Right),
                ("Split Up", ARROW_UP, SplitDirection::Up),
                ("Split Down", ARROW_DOWN, SplitDirection::Down),
            ] {
                if split_menu_button(ui, label, icon) {
                    queue_split_action(actions, direction);
                    ui.close();
                }
            }
        });
    });
}

#[derive(Clone, Copy)]
enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

fn queue_split_action(actions: &mut Vec<TileAction>, direction: SplitDirection) {
    let (axis, new_view_first) = match direction {
        SplitDirection::Left => (SplitAxis::Vertical, true),
        SplitDirection::Right => (SplitAxis::Vertical, false),
        SplitDirection::Up => (SplitAxis::Horizontal, true),
        SplitDirection::Down => (SplitAxis::Horizontal, false),
    };
    actions.push(TileAction::Split {
        axis,
        new_view_first,
        ratio: DEFAULT_SPLIT_RATIO,
    });
}

fn apply_context_menu_row_hover_style(ui: &mut egui::Ui) {
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.active.bg_fill = hover_bg;
    visuals.widgets.active.weak_bg_fill = hover_bg;
    visuals.widgets.open.bg_fill = hover_bg;
    visuals.widgets.open.weak_bg_fill = hover_bg;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.active.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.open.bg_stroke = egui::Stroke::NONE;
}

fn icon_rail_button(ui: &mut egui::Ui, icon: &str, tooltip: &str, enabled: bool) -> egui::Response {
    with_visual_overrides(ui, apply_icon_rail_button_style, |ui| {
        let button = egui::Button::new(
            egui::RichText::new(icon)
                .font(egui::FontId::proportional(17.0))
                .color(text_primary(ui)),
        )
        .min_size(EDITOR_CONTEXT_ICON_BUTTON_SIZE)
        .stroke(egui::Stroke::new(1.0, border(ui)))
        .corner_radius(egui::CornerRadius::same(8));

        ui.add_enabled(enabled, button).on_hover_text(tooltip)
    })
}

fn apply_icon_rail_button_style(ui: &mut egui::Ui) {
    let idle_bg = action_bg(ui);
    let hover_bg = action_hover_bg(ui);
    let visuals = ui.visuals_mut();
    visuals.widgets.inactive.bg_fill = idle_bg;
    visuals.widgets.inactive.weak_bg_fill = idle_bg;
    visuals.widgets.hovered.bg_fill = hover_bg;
    visuals.widgets.hovered.weak_bg_fill = hover_bg;
    visuals.widgets.active.bg_fill = hover_bg;
    visuals.widgets.active.weak_bg_fill = hover_bg;
    visuals.widgets.open.bg_fill = hover_bg;
    visuals.widgets.open.weak_bg_fill = hover_bg;
}

fn with_visual_overrides<R>(
    ui: &mut egui::Ui,
    configure: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let previous_visuals = ui.visuals().clone();
    configure(ui);
    let result = add_contents(ui);
    *ui.visuals_mut() = previous_visuals;
    result
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

fn editor_content_style<'a>(
    app: &ScratchpadApp,
    is_active: bool,
    request_focus: bool,
    editor_font_id: &'a egui::FontId,
) -> EditorContentStyle<'a> {
    EditorContentStyle {
        editor_gutter: app.editor_gutter(),
        is_active,
        viewport: None,
        previous_layout: None,
        text_edit: TextEditOptions::new(
            request_focus,
            app.word_wrap(),
            editor_font_id,
            app.editor_text_color(),
            EditorHighlightStyle::new(
                app.editor_text_highlight_color(),
                app.editor_text_highlight_text_color(),
            ),
        ),
        background_color: app.editor_background_color(),
    }
}

fn apply_tile_focus_request(
    app: &mut ScratchpadApp,
    view_id: ViewId,
    request_focus: bool,
    request_editor_focus: bool,
) {
    if request_focus {
        app.consume_focus_request(view_id);
    } else if request_editor_focus {
        app.request_focus_for_view(view_id);
    }
}

fn editor_scroll_bar_visibility(ctx: &egui::Context) -> egui::scroll_area::ScrollBarVisibility {
    if tab_drag::is_drag_active_for_context(ctx) {
        egui::scroll_area::ScrollBarVisibility::AlwaysHidden
    } else {
        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
    }
}

fn take_previous_layout(tab: &mut WorkspaceTab, view_id: ViewId) -> Option<RenderedLayout> {
    let current_revision = tab
        .buffer_for_view(view_id)
        .map(|buffer| buffer.document_revision());
    tab.view_mut(view_id).and_then(|view| {
        if view.latest_layout_revision == current_revision {
            view.latest_layout.take()
        } else {
            view.latest_layout = None;
            view.latest_layout_revision = None;
            None
        }
    })
}

fn editor_scroll_id(view_id: ViewId) -> egui::Id {
    egui::Id::new(("editor_scroll", view_id))
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct EditorScrollAreaDebugState {
    pub(super) offset: egui::Vec2,
    pub(super) content_size: egui::Vec2,
    pub(super) inner_rect: egui::Rect,
}

#[cfg(test)]
fn editor_scroll_debug_id(view_id: ViewId) -> egui::Id {
    egui::Id::new(("editor_scroll_debug", view_id))
}

#[cfg(test)]
fn store_editor_scroll_debug_state(
    ctx: &egui::Context,
    view_id: ViewId,
    state: EditorScrollAreaDebugState,
) {
    ctx.data_mut(|data| data.insert_temp(editor_scroll_debug_id(view_id), state));
}

#[cfg(test)]
pub(super) fn load_editor_scroll_debug_state(
    ctx: &egui::Context,
    view_id: ViewId,
) -> Option<EditorScrollAreaDebugState> {
    ctx.data(|data| data.get_temp(editor_scroll_debug_id(view_id)))
}

fn show_editor_scroll_area(
    ui: &mut egui::Ui,
    tab: &mut WorkspaceTab,
    request: TileScrollRequest<'_>,
) -> EditorContentOutcome {
    let scroll_id = editor_scroll_id(request.view_id);
    let scroll_offset = tab
        .view(request.view_id)
        .map(|view| view.editor_scroll_offset())
        .unwrap_or_default();
    let wheel_requested_scroll_offset =
        requested_scroll_offset_for_pointer_wheel(ui, scroll_offset);
    if wheel_requested_scroll_offset.is_some()
        && let Some(view) = tab.view_mut(request.view_id)
    {
        view.clear_cursor_reveal();
    }
    let render_scroll_offset = wheel_requested_scroll_offset.unwrap_or(scroll_offset);
    sync_editor_scroll_state(ui, scroll_id, render_scroll_offset);
    let row_height =
        ui.fonts_mut(|fonts| fonts.row_height(request.content_style.text_edit.editor_font_id));
    let virtual_row_height = row_height.max(request.content_style.text_edit.editor_font_id.size);
    let virtual_content_height = tab
        .buffer_for_view(request.view_id)
        .map(|buffer| buffer.line_count.max(1) as f32 * virtual_row_height)
        .unwrap_or_default();
    let output = egui::ScrollArea::both()
        .id_salt(scroll_id)
        .scroll_offset(render_scroll_offset)
        .auto_shrink([false, false])
        .scroll_source(editor_scroll_source())
        .scroll_bar_visibility(request.scroll_bar_visibility)
        .show_viewport(ui, |ui, viewport| {
            let mut content_style = request.content_style;
            content_style.viewport = Some(viewport);
            tab.buffer_and_view_mut(request.view_id)
                .map(|(buffer, view)| {
                    editor_content::render_editor_content(
                        ui,
                        buffer,
                        view,
                        request.view_id,
                        content_style,
                    )
                })
                .unwrap_or_else(missing_editor_content_outcome)
        });
    let content_size = editor_scroll_content_size(output.content_size, virtual_content_height);
    #[cfg(test)]
    store_editor_scroll_debug_state(
        ui.ctx(),
        request.view_id,
        EditorScrollAreaDebugState {
            offset: output.state.offset,
            content_size,
            inner_rect: output.inner_rect,
        },
    );
    let drag_requested_scroll_offset = requested_scroll_offset_for_selection_edge_drag(
        ui,
        scroll_offset,
        output.inner.interaction_response.as_ref(),
        content_size,
        output.inner_rect.size(),
        output.inner_rect,
    )
    .or_else(|| {
        requested_scroll_offset_for_pointer_drag(
            ui,
            scroll_offset,
            output.inner.interaction_response.as_ref(),
            content_size,
            output.inner_rect.size(),
            output.inner_rect,
        )
    });
    if let Some(view) = tab.view_mut(request.view_id) {
        view.set_editor_scroll_offset(resolve_editor_scroll_offset(
            &output,
            content_size,
            render_scroll_offset,
            wheel_requested_scroll_offset,
            drag_requested_scroll_offset,
        ));
    }
    output.inner
}

fn sync_editor_scroll_state(ui: &egui::Ui, scroll_id: egui::Id, offset: egui::Vec2) {
    let persistent_id = ui.make_persistent_id(scroll_id);
    let mut state = egui::scroll_area::State::load(ui.ctx(), persistent_id).unwrap_or_default();
    if state.offset != offset {
        state.offset = offset;
        state.store(ui.ctx(), persistent_id);
    }
}

fn editor_scroll_source() -> egui::scroll_area::ScrollSource {
    egui::scroll_area::ScrollSource {
        drag: false,
        mouse_wheel: false,
        ..egui::scroll_area::ScrollSource::ALL
    }
}

fn resolve_editor_scroll_offset(
    output: &egui::scroll_area::ScrollAreaOutput<EditorContentOutcome>,
    content_size: egui::Vec2,
    fallback_scroll_offset: egui::Vec2,
    wheel_requested_scroll_offset: Option<egui::Vec2>,
    drag_requested_scroll_offset: Option<egui::Vec2>,
) -> egui::Vec2 {
    clamp_scroll_offset(
        drag_requested_scroll_offset
            .or(output.inner.requested_scroll_offset)
            .or(wheel_requested_scroll_offset)
            .unwrap_or(fallback_scroll_offset),
        content_size,
        output.inner_rect.size(),
    )
}

fn editor_scroll_content_size(content_size: egui::Vec2, virtual_content_height: f32) -> egui::Vec2 {
    egui::vec2(
        content_size.x,
        content_size.y.max(virtual_content_height.max(0.0)),
    )
}

fn requested_scroll_offset_for_pointer_drag(
    ui: &egui::Ui,
    current_offset: egui::Vec2,
    interaction_response: Option<&egui::Response>,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
    inner_rect: egui::Rect,
) -> Option<egui::Vec2> {
    if !pointer_over_rect(ui, inner_rect)
        || !ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
        || interaction_response
            .is_some_and(|response| response.dragged_by(egui::PointerButton::Primary))
    {
        return None;
    }

    scroll_offset_from_drag_delta(
        current_offset,
        ui.input(|input| input.pointer.delta()),
        content_size,
        viewport_size,
    )
}

fn requested_scroll_offset_for_selection_edge_drag(
    ui: &egui::Ui,
    current_offset: egui::Vec2,
    interaction_response: Option<&egui::Response>,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
    inner_rect: egui::Rect,
) -> Option<egui::Vec2> {
    if !ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary))
        || !interaction_response
            .is_some_and(|response| response.dragged_by(egui::PointerButton::Primary))
    {
        return None;
    }

    let pointer_pos = ui.input(|input| input.pointer.latest_pos())?;
    scroll_offset_from_selection_edge_drag(
        current_offset,
        selection_edge_drag_delta(inner_rect, pointer_pos),
        content_size,
        viewport_size,
    )
}

fn requested_scroll_offset_for_pointer_wheel(
    ui: &egui::Ui,
    current_offset: egui::Vec2,
) -> Option<egui::Vec2> {
    if !pointer_over_rect(ui, ui.max_rect()) {
        return None;
    }

    scroll_offset_from_wheel_delta(current_offset, ui.input(|input| input.smooth_scroll_delta))
}

fn pointer_over_rect(ui: &egui::Ui, rect: egui::Rect) -> bool {
    ui.input(|input| {
        input
            .pointer
            .hover_pos()
            .is_some_and(|pos| rect.contains(pos))
    })
}

fn scroll_offset_from_wheel_delta(
    current_offset: egui::Vec2,
    scroll_delta: egui::Vec2,
) -> Option<egui::Vec2> {
    let desired = egui::vec2(
        (current_offset.x - scroll_delta.x).max(0.0),
        (current_offset.y - scroll_delta.y).max(0.0),
    );
    (desired != current_offset).then_some(desired)
}

fn scroll_offset_from_drag_delta(
    current_offset: egui::Vec2,
    drag_delta: egui::Vec2,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
) -> Option<egui::Vec2> {
    if drag_delta == egui::Vec2::ZERO {
        return None;
    }

    let desired = clamp_scroll_offset(current_offset - drag_delta, content_size, viewport_size);
    (desired != current_offset).then_some(desired)
}

fn scroll_offset_from_selection_edge_drag(
    current_offset: egui::Vec2,
    drag_delta: egui::Vec2,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
) -> Option<egui::Vec2> {
    if drag_delta == egui::Vec2::ZERO {
        return None;
    }

    let desired = clamp_scroll_offset(current_offset + drag_delta, content_size, viewport_size);
    (desired != current_offset).then_some(desired)
}

fn selection_edge_drag_delta(viewport_rect: egui::Rect, pointer_pos: egui::Pos2) -> egui::Vec2 {
    egui::vec2(
        edge_auto_scroll_delta(
            viewport_rect,
            pointer_pos,
            AutoScrollAxis::Horizontal,
            EDITOR_SELECTION_AUTOSCROLL_CONFIG,
        ),
        edge_auto_scroll_delta(
            viewport_rect,
            pointer_pos,
            AutoScrollAxis::Vertical,
            EDITOR_SELECTION_AUTOSCROLL_CONFIG,
        ),
    )
}

fn clamp_scroll_offset(
    offset: egui::Vec2,
    content_size: egui::Vec2,
    viewport_size: egui::Vec2,
) -> egui::Vec2 {
    let max_offset = max_scroll_offset(content_size, viewport_size);
    egui::vec2(
        offset.x.clamp(0.0, max_offset.x),
        offset.y.clamp(0.0, max_offset.y),
    )
}

fn max_scroll_offset(content_size: egui::Vec2, viewport_size: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        (content_size.x - viewport_size.x).max(0.0),
        (content_size.y - viewport_size.y).max(0.0),
    )
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
        request_editor_focus: false,
        requested_scroll_offset: None,
        interaction_response: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_scroll_offset, editor_scroll_id, editor_scroll_source, max_scroll_offset,
        scroll_offset_from_drag_delta, scroll_offset_from_selection_edge_drag,
        scroll_offset_from_wheel_delta, selection_edge_drag_delta,
    };
    use crate::app::domain::{EditorViewState, WorkspaceTab};
    use eframe::egui;

    #[test]
    fn editor_scroll_id_is_scoped_to_the_view() {
        assert_eq!(editor_scroll_id(7), editor_scroll_id(7));
        assert_ne!(editor_scroll_id(7), editor_scroll_id(8));
    }

    #[test]
    fn wheel_delta_requests_explicit_scroll_offset() {
        assert_eq!(
            scroll_offset_from_wheel_delta(egui::vec2(12.0, 90.0), egui::vec2(4.0, -18.0)),
            Some(egui::vec2(8.0, 108.0))
        );
        assert_eq!(
            scroll_offset_from_wheel_delta(egui::vec2(0.0, 10.0), egui::vec2(0.0, 20.0)),
            Some(egui::vec2(0.0, 0.0))
        );
    }

    #[test]
    fn editor_scroll_source_disables_builtin_drag_scrolling() {
        let source = editor_scroll_source();

        assert!(!source.drag);
        assert!(!source.mouse_wheel);
        assert!(source.scroll_bar);
    }

    #[test]
    fn drag_delta_requests_clamped_scroll_offset() {
        assert_eq!(
            scroll_offset_from_drag_delta(
                egui::vec2(80.0, 60.0),
                egui::vec2(-200.0, -160.0),
                egui::vec2(320.0, 260.0),
                egui::vec2(120.0, 100.0),
            ),
            Some(egui::vec2(200.0, 160.0))
        );
    }

    #[test]
    fn selection_edge_drag_delta_pushes_down_near_bottom_edge() {
        assert_eq!(
            selection_edge_drag_delta(
                egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 120.0)),
                egui::pos2(100.0, 150.0),
            ),
            egui::vec2(0.0, 18.0)
        );
    }

    #[test]
    fn selection_edge_drag_delta_is_zero_away_from_edges() {
        assert_eq!(
            selection_edge_drag_delta(
                egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(200.0, 120.0)),
                egui::pos2(100.0, 80.0),
            ),
            egui::Vec2::ZERO
        );
    }

    #[test]
    fn selection_edge_drag_requests_clamped_scroll_offset() {
        assert_eq!(
            scroll_offset_from_selection_edge_drag(
                egui::vec2(80.0, 60.0),
                egui::vec2(0.0, 18.0),
                egui::vec2(320.0, 260.0),
                egui::vec2(120.0, 100.0),
            ),
            Some(egui::vec2(80.0, 78.0))
        );
    }

    #[test]
    fn clamp_scroll_offset_limits_east_and_south_to_content_bounds() {
        assert_eq!(
            clamp_scroll_offset(
                egui::vec2(280.0, 220.0),
                egui::vec2(320.0, 260.0),
                egui::vec2(120.0, 100.0),
            ),
            egui::vec2(200.0, 160.0)
        );
        assert_eq!(
            max_scroll_offset(egui::vec2(320.0, 260.0), egui::vec2(120.0, 100.0)),
            egui::vec2(200.0, 160.0)
        );
    }

    #[test]
    fn duplicated_views_can_track_independent_scroll_offsets() {
        let mut tab = WorkspaceTab::untitled();
        let buffer_id = tab.buffer.id;
        let first_view_id = tab.active_view_id;
        let second_view = EditorViewState::new(buffer_id, false);
        let second_view_id = second_view.id;
        tab.views.push(second_view);

        tab.view_mut(first_view_id)
            .expect("first view")
            .set_editor_scroll_offset(egui::vec2(0.0, 120.0));
        tab.view_mut(second_view_id)
            .expect("second view")
            .set_editor_scroll_offset(egui::vec2(0.0, 420.0));

        assert_eq!(
            tab.view(first_view_id)
                .expect("first view")
                .editor_scroll_offset(),
            egui::vec2(0.0, 120.0)
        );
        assert_eq!(
            tab.view(second_view_id)
                .expect("second view")
                .editor_scroll_offset(),
            egui::vec2(0.0, 420.0)
        );
    }
}
