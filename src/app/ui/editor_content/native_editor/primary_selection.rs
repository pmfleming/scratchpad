//! Publish PRIMARY only after selection settles, without retaining another text copy.
use crate::app::domain::{BufferState, EditorViewState};
use eframe::egui;

#[derive(Clone, Default)]
struct PublishedSelection(Option<(u64, u64, (usize, usize))>);

pub(super) fn publish(
    ui: &egui::Ui,
    id: egui::Id,
    buffer: &BufferState,
    view: &EditorViewState,
    focused: bool,
) {
    if !focused || ui.input(|input| input.pointer.button_down(egui::PointerButton::Primary)) {
        return;
    }
    let cursor = view.cursor_range.filter(|cursor| !cursor.is_empty());
    let selection = cursor.map(|cursor| {
        (
            buffer.id,
            buffer.document_revision(),
            cursor.sorted_indices(),
        )
    });
    let key = id.with("published_primary_selection");
    let changed = ui.data_mut(|data| {
        let previous = data.get_temp::<PublishedSelection>(key).unwrap_or_default();
        if previous.0 == selection {
            false
        } else {
            data.insert_temp(key, PublishedSelection(selection));
            true
        }
    });
    if changed && let Some(text) = cursor.and_then(|cursor| super::selected_text(buffer, cursor)) {
        ui.ctx()
            .send_cmd(egui::OutputCommand::TextSelectionSettled(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::domain::CursorRange;

    #[test]
    fn publishes_once_on_release_and_not_on_idle_or_focus_changes() {
        let ctx = egui::Context::default();
        let buffer = BufferState::new("test".to_owned(), "hello world".to_owned(), None);
        let mut view = EditorViewState::new(buffer.id);
        view.cursor_range = Some(CursorRange::two(0, 5));
        let id = egui::Id::unique("primary-test");
        let mut run = |pressed: Option<bool>, focused: bool, selection: Option<CursorRange>| {
            view.cursor_range = selection;
            let input = egui::RawInput {
                events: pressed
                    .into_iter()
                    .map(|pressed| egui::Event::PointerButton {
                        pos: egui::pos2(10.0, 10.0),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .collect(),
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(input, |ui| publish(ui, id, &buffer, &view, focused));
            let commands = output.platform_output.commands.clone();
            output.drop_without_applying_deltas();
            commands
        };
        let range = Some(CursorRange::two(0, 5));
        assert!(run(Some(true), true, range).is_empty());
        assert_eq!(
            run(Some(false), true, range),
            vec![egui::OutputCommand::TextSelectionSettled(
                "hello".to_owned()
            )]
        );
        assert!(run(None, true, range).is_empty());
        assert!(run(None, false, range).is_empty());
        assert!(run(None, true, range).is_empty());
        assert!(run(None, true, None).is_empty());
        assert_eq!(run(None, true, range).len(), 1);
    }
}
