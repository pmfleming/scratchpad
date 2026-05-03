use eframe::egui::{self, ComboBox, Id, LayerId, Order, Rect, Response, Sense};
use std::hash::Hash;

const ID_NAMESPACE: &str = "scratchpad.widget";

pub(crate) fn configure_debug_options(ctx: &egui::Context) {
    ctx.options_mut(|options| options.warn_on_id_clash = cfg!(debug_assertions));
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

pub(crate) fn interact(
    ui: &egui::Ui,
    rect: Rect,
    id: Id,
    sense: Sense,
    kind: &'static str,
) -> Response {
    let response = ui.interact(rect, id, sense);
    track(ui.ctx(), id, response.rect, kind);
    response
}

pub(crate) fn track(ctx: &egui::Context, id: Id, rect: Rect, kind: &'static str) {
    crate::app::diagnostics::track_widget_id(id, rect, kind);
    #[cfg(debug_assertions)]
    ctx.check_for_id_clash(id, rect, kind);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    const FORBIDDEN_APP_PATTERNS: &[&str] = &[
        "widget_ids::global",
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
