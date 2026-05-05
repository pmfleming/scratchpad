use eframe::egui::{self, ComboBox, Id, LayerId, Order, Rect, Response, Sense};
use std::hash::Hash;

const ID_NAMESPACE: &str = "scratchpad.widget";

#[derive(Clone, Copy, Debug, Hash)]
pub(crate) enum WidgetRole {
    IconButton,
    ActionButton,
    Label,
    ToggleChip,
    TextEdit,
    ToggleSwitch,
    RadioOption,
    SettingsCardHeader,
    TabButton,
    TabPromote,
    TabClose,
    TabRenameEditor,
}

pub(crate) fn configure_debug_options(ctx: &egui::Context) {
    ctx.options_mut(|options| options.warn_on_id_clash = cfg!(debug_assertions));
    let registration_id = ctx_key("diagnostics_begin_pass_registered");
    let should_register = ctx.data_mut(|data| {
        if data.get_persisted::<bool>(registration_id).unwrap_or(false) {
            false
        } else {
            data.insert_persisted(registration_id, true);
            true
        }
    });
    if should_register {
        ctx.on_begin_pass(
            "diagnostics",
            std::sync::Arc::new(|ctx| {
                crate::app::diagnostics::begin_pass(ctx.current_pass_index());
            }),
        );
    }
}

pub(crate) fn ctx_key(key: impl Hash) -> Id {
    Id::new((ID_NAMESPACE, "ctx", key))
}

pub(crate) fn area_id(key: impl Hash) -> Id {
    Id::new((ID_NAMESPACE, "area", key))
}

pub(crate) fn area(key: impl Hash) -> egui::Area {
    egui::Area::new(area_id(key))
}

pub(crate) fn root_id(key: impl Hash) -> Id {
    Id::new((ID_NAMESPACE, "root", key))
}

pub(crate) fn surface_id(key: impl Hash) -> Id {
    root_id(("surface", key))
}

pub(crate) fn rect_key(rect: Rect) -> (u32, u32, u32, u32) {
    (
        rect.min.x.to_bits(),
        rect.min.y.to_bits(),
        rect.max.x.to_bits(),
        rect.max.y.to_bits(),
    )
}

pub(crate) fn rect_surface_id(rect: Rect, role: impl Hash) -> Id {
    root_id(("rect_surface", role, rect_key(rect)))
}

pub(crate) fn surface_child(key: impl Hash, role: impl Hash) -> Id {
    child(surface_id(key), role)
}

pub(crate) fn surface_role(key: impl Hash, role: WidgetRole) -> Id {
    surface_child(key, role)
}

pub(crate) fn surface_widget<R>(
    ui: &mut egui::Ui,
    key: impl Hash,
    role: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    surface_scope(ui, (key, role), add_contents)
}

pub(crate) fn surface_response(
    ui: &mut egui::Ui,
    key: impl Hash,
    role: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui) -> Response,
) -> Response {
    surface_widget(ui, key, role, add_contents).inner
}

pub(crate) fn layer_id(order: Order, key: impl Hash) -> LayerId {
    LayerId::new(order, root_id(key))
}

pub(crate) fn local(ui: &egui::Ui, key: impl Hash) -> Id {
    ui.make_persistent_id((ID_NAMESPACE, key))
}

pub(crate) fn scroll_id(ui: &egui::Ui, key: impl Hash) -> Id {
    local(ui, ("scroll", key))
}

pub(crate) fn combo_box(ui: &egui::Ui, key: impl Hash) -> ComboBox {
    ComboBox::from_id_salt(local(ui, ("combo_box", key)))
}

pub(crate) fn child(id: Id, key: impl Hash) -> Id {
    id.with((ID_NAMESPACE, key))
}

pub(crate) fn read_deferred_persisted<T: Clone + Send + Sync + 'static>(
    ctx: &egui::Context,
    pending_id: Id,
    active_id: Id,
) -> Option<T> {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| {
        if let Some((value, apply_frame)) = data.get_persisted::<(T, u64)>(pending_id)
            && apply_frame <= frame
        {
            data.insert_persisted(active_id, value.clone());
            return Some(value);
        }
        data.get_persisted::<T>(active_id)
    })
}

pub(crate) fn write_deferred_persisted<T: Clone + Send + Sync + 'static>(
    ctx: &egui::Context,
    pending_id: Id,
    value: T,
) {
    let apply_frame = ctx.cumulative_frame_nr().saturating_add(1);
    ctx.data_mut(|data| {
        data.insert_persisted(pending_id, (value, apply_frame));
    });
    ctx.request_repaint();
}

pub(crate) fn feature_scope<R>(
    ui: &mut egui::Ui,
    feature: &'static str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    scope(ui, ("feature", feature), add_contents)
}

pub(crate) fn scope<R>(
    ui: &mut egui::Ui,
    key: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.push_id((ID_NAMESPACE, key), add_contents)
}

pub(crate) fn surface_scope<R>(
    ui: &mut egui::Ui,
    key: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.push_id(surface_id(key), add_contents)
}

pub(crate) fn rect_scope<R>(
    ui: &mut egui::Ui,
    rect: Rect,
    role: impl Hash,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    rect_scope_with_layout(ui, rect, role, *ui.layout(), add_contents)
}

pub(crate) fn rect_scope_with_layout<R>(
    ui: &mut egui::Ui,
    rect: Rect,
    role: impl Hash,
    layout: egui::Layout,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.scope_builder(rect_ui_builder(rect, role, layout), add_contents)
}

pub(crate) fn rect_child_ui(
    ui: &mut egui::Ui,
    rect: Rect,
    role: impl Hash,
    layout: egui::Layout,
) -> egui::Ui {
    ui.new_child(rect_ui_builder(rect, role, layout))
}

fn rect_ui_builder(rect: Rect, role: impl Hash, layout: egui::Layout) -> egui::UiBuilder {
    egui::UiBuilder::new()
        .id_salt(rect_surface_id(rect, role))
        .global_scope(true)
        .max_rect(rect)
        .layout(layout)
}

#[track_caller]
pub(crate) fn interact(
    ui: &egui::Ui,
    rect: Rect,
    id: Id,
    sense: Sense,
    kind: &'static str,
) -> Response {
    let response = ui.interact(rect, id, sense);
    track(id, response.rect, kind);
    response
}

#[track_caller]
pub(crate) fn allocate_exact_interact(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    id: Id,
    sense: Sense,
    kind: &'static str,
) -> Response {
    let rect = Rect::from_min_size(ui.available_rect_before_wrap().min, size);
    let response = interact(ui, rect, id, sense, kind);
    ui.advance_cursor_after_rect(rect);
    response
}

#[track_caller]
pub(crate) fn allocate_exact_rect_interact(
    ui: &mut egui::Ui,
    size: egui::Vec2,
    role: impl Hash,
    sense: Sense,
    kind: &'static str,
) -> Response {
    let rect = Rect::from_min_size(ui.available_rect_before_wrap().min, size);
    let response = interact(ui, rect, rect_surface_id(rect, role), sense, kind);
    ui.advance_cursor_after_rect(rect);
    response
}

pub(crate) fn allocate_exact_rect(ui: &mut egui::Ui, size: egui::Vec2) -> Rect {
    let rect = Rect::from_min_size(ui.available_rect_before_wrap().min, size);
    ui.advance_cursor_after_rect(rect);
    rect
}

#[track_caller]
pub(crate) fn track(id: Id, rect: Rect, kind: &'static str) {
    let location = std::panic::Location::caller();
    crate::app::diagnostics::track_widget_id(id, rect, kind, location);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    const FORBIDDEN_APP_PATTERNS: &[&str] = &[
        "widget_ids::global",
        "ui.new_child(",
        "ui.scope_builder(",
        "egui::UiBuilder::new(",
        "ui.allocate_exact_size(",
        "ui.interact(",
        ".make_persistent_id(",
        ".push_id(",
        "ui.indent((",
        "egui::Area::new(",
        "egui::LayerId::new(",
        "ComboBox::from_id_salt(",
    ];

    #[test]
    fn app_code_uses_widget_id_wrappers_for_raw_egui_ids() {
        let src_app = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/app");
        let mut violations = Vec::new();
        collect_forbidden_patterns(&src_app, &mut violations);

        assert!(
            violations.is_empty(),
            "raw egui id escape hatches found outside widget_ids.rs:\n{}",
            violations.join("\n")
        );
    }

    fn collect_forbidden_patterns(path: &Path, violations: &mut Vec<String>) {
        let Ok(metadata) = fs::metadata(path) else {
            return;
        };
        if metadata.is_dir() {
            let Ok(entries) = fs::read_dir(path) else {
                return;
            };
            for entry in entries.flatten() {
                collect_forbidden_patterns(&entry.path(), violations);
            }
            return;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("widget_ids.rs")
            || path.extension().and_then(|ext| ext.to_str()) != Some("rs")
        {
            return;
        }

        let Ok(contents) = fs::read_to_string(path) else {
            return;
        };
        for (line_index, line) in contents.lines().enumerate() {
            for pattern in FORBIDDEN_APP_PATTERNS {
                if line.contains(pattern) {
                    violations.push(format!(
                        "{}:{} contains `{}`",
                        path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                            .unwrap_or(path)
                            .display(),
                        line_index + 1,
                        pattern
                    ));
                }
            }
        }
    }
}
