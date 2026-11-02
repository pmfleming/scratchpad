use eframe::egui;

#[derive(Clone, Default)]
struct Hover {
    target: Option<u64>,
    armed: bool,
}

pub(super) struct Navigation {
    pub first: bool,
    pub reveal: bool,
    pub hover: bool,
    pub reachable_hover: bool,
    pub target: Option<u64>,
}

fn reveal_key() -> egui::Id {
    egui::Id::unique("history.rail.reveal")
}

pub(in super::super) fn request_reveal(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(reveal_key(), true));
}

pub(super) fn begin(ui: &egui::Ui, target: Option<u64>, first: bool) -> Navigation {
    let key = egui::Id::unique("history.rail.pointer");
    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<Hover>(key))
        .unwrap_or_default();
    let first = first || state.target != target;
    let requested = ui.ctx().data_mut(|d| d.remove_temp::<bool>(reveal_key()));
    let keyboard = requested.unwrap_or_default()
        || ui.ctx().memory(|m| m.has_focus(super::focus_id()))
            && ui.input(|i| {
                i.events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key {
                            key: egui::Key::ArrowUp
                                | egui::Key::ArrowDown
                                | egui::Key::Home
                                | egui::Key::End,
                            pressed: true,
                            ..
                        }
                    )
                })
            });
    let moved = ui.input(|i| {
        i.events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerMoved(_)))
    });
    if first || keyboard || (!moved && !ui.ctx().memory(|m| m.has_focus(super::focus_id()))) {
        state.armed = false;
    } else if moved {
        state.armed = true;
    }
    state.target = target;
    let hover = state.armed && !egui::Popup::is_any_open(ui.ctx());
    ui.ctx().data_mut(|d| d.insert_temp(key, state));
    Navigation {
        first,
        reveal: first || keyboard,
        hover,
        reachable_hover: false,
        target,
    }
}

pub(super) fn scroll_edges(
    ui: &egui::Ui,
    scroll: &egui::scroll_area::ScrollAreaOutput<()>,
    navigation: &Navigation,
) {
    if !navigation.hover || !navigation.reachable_hover || ui.input(|i| i.pointer.any_down()) {
        return;
    }
    let Some(pointer) = ui
        .input(|i| i.pointer.hover_pos())
        .filter(|p| scroll.inner_rect.contains(*p))
    else {
        return;
    };
    let rect = scroll.inner_rect;
    let band = 28.0_f32.min(rect.height() / 3.0);
    let speed = if pointer.y < rect.top() + band {
        -((rect.top() + band - pointer.y) / band)
    } else if pointer.y > rect.bottom() - band {
        (pointer.y - (rect.bottom() - band)) / band
    } else {
        0.0
    };
    let elapsed = ui.input(|i| i.stable_dt).clamp(0.0, 0.05);
    let offset = (scroll.state.clamped_offset().y + speed * 240.0 * elapsed)
        .clamp(0.0, (scroll.content_size.y - rect.height()).max(0.0));
    if (offset - scroll.state.clamped_offset().y).abs() > f32::EPSILON {
        ui.ctx()
            .data_mut(|d| d.insert_temp(offset_key(navigation.target), offset));
        ui.ctx().request_repaint();
    }
}

fn offset_key(target: Option<u64>) -> egui::Id {
    egui::Id::unique(("history.rail.edge.offset", target))
}

pub(super) fn pending_offset(ctx: &egui::Context, target: Option<u64>) -> Option<f32> {
    ctx.data_mut(|d| d.remove_temp::<f32>(offset_key(target)))
}
