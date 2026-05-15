use log::Metadata;

pub(super) fn widget_rect_changed_fingerprint(
    message: &str,
    prev_sites: &[String],
    new_sites: &[String],
) -> String {
    let rect = message
        .strip_prefix("Widget rect ")
        .and_then(|rest| rest.split_once(" changed id between passes"))
        .map_or(message, |(rect, _)| rect);
    format!(
        "widget_rect_changed|{}|{}|{}",
        rect,
        prev_sites.join(" | "),
        new_sites.join(" | ")
    )
}

pub(super) fn extract_hexes(message: &str, prefix: &str, suffix: &str) -> Vec<String> {
    let Some(start) = message.find(prefix) else {
        return Vec::new();
    };
    let after_prefix = &message[start + prefix.len()..];
    let Some(end) = after_prefix.find(suffix) else {
        return Vec::new();
    };

    after_prefix[..end]
        .split(',')
        .filter_map(|part| {
            let hex = part.trim().trim_matches('"');
            (!hex.is_empty()).then(|| hex.to_owned())
        })
        .collect()
}

pub(super) fn should_capture_log_record(metadata: &Metadata<'_>, message: &str) -> bool {
    metadata.level() <= log::Level::Warn
        && (is_app_target(metadata.target())
            || is_egui_target(metadata.target())
            || is_egui_warning_message(message))
}

pub(super) fn is_egui_target(target: &str) -> bool {
    target.starts_with("egui") || target.starts_with("eframe") || target.contains("egui")
}

pub(super) fn is_egui_warning_message(message: &str) -> bool {
    message.contains("egui")
        || message.contains("Id ")
        || message.contains("Widget rect")
        || message.contains("same id")
}

fn is_app_target(target: &str) -> bool {
    target == env!("CARGO_CRATE_NAME")
        || target.starts_with(concat!(env!("CARGO_CRATE_NAME"), "::"))
}
