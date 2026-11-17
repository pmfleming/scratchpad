//! Chrome shared by the floating panels drawn in History's style (History, Search,
//! Status and Encoding): the docked window, outside-click dismissal, the title row
//! with its switch, and the measured bottom panel.
use crate::app::app_state::{ScratchpadApp, workspace::accessors};
use crate::app::theme;
use crate::app::ui::{callout, settings, widget_ids};
use eframe::egui;

/// Gap between a panel and the screen edge it is docked to.
const SCREEN_MARGIN: f32 = 24.0;
/// Keeps the title visibly separate from the switch at minimum width.
const TITLE_GAP: f32 = 16.0;

/// Where a panel of `outer_width` opens: near the top-right corner of `screen`.
pub(crate) fn top_right(screen: egui::Rect, outer_width: f32) -> egui::Pos2 {
    egui::pos2(
        (screen.right() - outer_width - SCREEN_MARGIN).max(screen.left()),
        screen.top() + 72.0,
    )
}

/// A titleless, resizable window docked top-right that never outgrows the screen.
pub(crate) fn resizable(
    ctx: &egui::Context,
    title: &'static str,
    layer: egui::LayerId,
    frame: egui::Frame,
    default_size: egui::Vec2,
    min_size: egui::Vec2,
) -> egui::Window<'static> {
    let screen = ctx.content_rect();
    egui::Window::new(title)
        .id(layer.id)
        .order(layer.order)
        .title_bar(false)
        .resizable(true)
        .default_size(default_size.max(min_size))
        .min_size(min_size)
        .max_size((screen.size() - egui::Vec2::splat(SCREEN_MARGIN)).max(min_size))
        .frame(frame)
        .default_pos(top_right(screen, default_size.x))
}

/// Runs `add_contents` in the panel's feature scope with its typography and flat style.
pub(crate) fn contents<R>(
    ui: &mut egui::Ui,
    feature: &'static str,
    fill: egui::Color32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    widget_ids::feature_scope(ui, feature, |ui| {
        settings::apply_dialog_typography(ui);
        callout::apply_spacing(ui);
        super::apply(ui);
        ui.visuals_mut().panel_fill = fill;
        ui.visuals_mut().window_fill = fill;
        add_contents(ui)
    })
    .inner
}

/// What must be captured before a panel is shown to tell an outside click from the
/// click that opened it or one inside a menu.
pub(crate) struct OutsideClick {
    enabled: bool,
    was_visible: bool,
    popup_open: bool,
}

impl OutsideClick {
    pub(crate) fn before_show(
        ctx: &egui::Context,
        app: &ScratchpadApp,
        layer: egui::LayerId,
    ) -> Self {
        Self {
            enabled: super::dismiss_on_outside_click(app),
            was_visible: ctx.memory(|m| m.areas().visible_last_frame(&layer)),
            popup_open: egui::Popup::is_any_open(ctx),
        }
    }

    /// Whether the panel shows its own close button.
    pub(crate) fn needs_close_button(&self) -> bool {
        !self.enabled
    }

    /// Blocks scrolling behind the shown panel and reports a dismissing outside click.
    pub(crate) fn after_show(
        &self,
        ctx: &egui::Context,
        app: &ScratchpadApp,
        response: &egui::Response,
    ) -> bool {
        callout::mark_scroll_blocker_if_hovered(ctx, response);
        self.enabled
            && self.was_visible
            && !self.popup_open
            && !egui::Popup::is_any_open(ctx)
            && accessors::pending_action(app).is_none()
            && response.clicked_elsewhere()
    }
}

/// The panel's title, as History, Search and Status show it.
pub(crate) fn title(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(
        egui::RichText::new(text)
            .size(14.0)
            .strong()
            .color(callout::text(ui)),
    )
}

/// The close button panels show where outside clicks do not dismiss them.
pub(crate) fn close_button(ui: &mut egui::Ui, key: &'static str, label: &str) -> egui::Response {
    crate::app::ui::close_button::CloseButton::new(label).show(ui, key)
}

/// The right-hand side of a header row; measures each switch button it is given.
pub(crate) struct Switch<'u> {
    pub(crate) ui: &'u mut egui::Ui,
    width: f32,
}

impl Switch<'_> {
    pub(crate) fn measure(&mut self, response: &egui::Response) {
        self.width += response.rect.width() + self.ui.spacing().item_spacing.x;
    }
}

pub(crate) struct Header {
    pub(crate) close: bool,
    /// Content width that keeps the title and every switch button on one row.
    pub(crate) required_width: f32,
}

/// Title on the left; the close button (when `close` names one) and then the switch,
/// added right to left. The row is sized for buttons so the title centres against them.
pub(crate) fn header(
    ui: &mut egui::Ui,
    text: &str,
    close: Option<(&'static str, &str)>,
    add_switch: impl FnOnce(&mut Switch<'_>),
) -> Header {
    let mut header = Header {
        close: false,
        required_width: TITLE_GAP,
    };
    let height =
        ui.spacing().interact_size.y.max(
            ui.text_style_height(&egui::TextStyle::Button) + 2.0 * ui.spacing().button_padding.y,
        );
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            header.required_width += title(ui, text).rect.width();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut switch = Switch { ui, width: 0.0 };
                if let Some((key, label)) = close {
                    let button = close_button(switch.ui, key, label);
                    switch.measure(&button);
                    header.close = button.clicked();
                }
                add_switch(&mut switch);
                header.required_width += switch.width;
            });
        },
    );
    header
}

/// A header's last measured width, which the window keeps as its minimum.
pub(crate) struct MeasuredWidth {
    pub(crate) key: &'static str,
    /// Used until the header has been measured once.
    pub(crate) estimate: f32,
    pub(crate) discard_reason: &'static str,
}

impl MeasuredWidth {
    pub(crate) fn get(&self, ctx: &egui::Context) -> f32 {
        ctx.data(|d| d.get_temp::<f32>(egui::Id::unique(self.key)))
            .unwrap_or(self.estimate)
    }

    /// Counts, fonts or theme changes alter the header; resize before it is painted.
    pub(crate) fn remember(&self, ctx: &egui::Context, required: f32) {
        let required = required.ceil();
        let key = egui::Id::unique(self.key);
        let changed = ctx
            .data(|d| d.get_temp::<f32>(key))
            .is_none_or(|old| (old - required).abs() > 0.5);
        if changed {
            ctx.data_mut(|d| d.insert_temp(key, required));
            ctx.request_discard(self.discard_reason);
        }
    }
}

/// Where `measured_footer` keeps the last measured height of footer `id`.
pub(crate) fn footer_height_key(id: &'static str) -> egui::Id {
    egui::Id::unique(id).with("height")
}

/// A bottom panel sized to its real wrapped content. When the content changes size a
/// discarded sizing pass fits the panel before it is painted, which keeps the user's
/// window size.
pub(crate) fn measured_footer<R>(
    ui: &mut egui::Ui,
    id: &'static str,
    initial_height: f32,
    discard_reason: &'static str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let key = footer_height_key(id);
    let height = ui
        .ctx()
        .data(|d| d.get_temp::<f32>(key))
        .unwrap_or(initial_height);
    let (inner, measured) = egui::Panel::bottom(egui::Id::unique(id))
        .resizable(false)
        .show_separator_line(false)
        .exact_size(height)
        .frame(egui::Frame::NONE)
        .show(ui, |ui| {
            let top = ui.cursor().top();
            let inner = add_contents(ui);
            (inner, ui.cursor().top() - top)
        })
        .inner;
    if (measured - height).abs() > 0.5 {
        ui.ctx().data_mut(|d| d.insert_temp(key, measured));
        ui.ctx().request_discard(discard_reason);
    }
    inner
}

/// The accent-filled button for a panel's main action, greyed while unavailable.
pub(crate) fn primary_button(
    ui: &egui::Ui,
    text: impl Into<String>,
    enabled: bool,
) -> egui::Button<'static> {
    let (color, fill) = if enabled {
        (super::on_accent(ui), theme::tab_selected_accent(ui))
    } else {
        (theme::text_muted(ui), theme::action_bg(ui))
    };
    egui::Button::new(egui::RichText::new(text).color(color)).fill(fill)
}
