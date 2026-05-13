use super::egui;

thread_local! {
    static SETTINGS_CONTROL_LANE: std::cell::Cell<Option<egui::Rect>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(in crate::app::ui::settings) struct SettingsControlMeasurement {
    pub label: String,
    pub width: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub right_x: f32,
}

#[cfg(test)]
thread_local! {
    static SETTINGS_CARD_MEASUREMENTS: std::cell::RefCell<Vec<SettingsControlMeasurement>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static SETTINGS_CONTROL_MEASUREMENTS: std::cell::RefCell<Vec<SettingsControlMeasurement>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(test)]
pub(in crate::app::ui::settings) fn reset_settings_layout_measurements() {
    SETTINGS_CARD_MEASUREMENTS.with(|measurements| measurements.borrow_mut().clear());
    SETTINGS_CONTROL_MEASUREMENTS.with(|measurements| measurements.borrow_mut().clear());
}

#[cfg(test)]
pub(in crate::app::ui::settings) fn settings_card_measurements() -> Vec<SettingsControlMeasurement>
{
    SETTINGS_CARD_MEASUREMENTS.with(|measurements| measurements.borrow().clone())
}

#[cfg(test)]
pub(in crate::app::ui::settings) fn settings_control_measurements()
-> Vec<SettingsControlMeasurement> {
    SETTINGS_CONTROL_MEASUREMENTS.with(|measurements| measurements.borrow().clone())
}

pub(super) fn record_settings_control_box(label: impl Into<String>, rect: egui::Rect) {
    #[cfg(test)]
    record_measurement(&SETTINGS_CONTROL_MEASUREMENTS, label, rect);

    #[cfg(not(test))]
    let _ = (label, rect);
}

pub(super) fn record_settings_card_box(label: impl Into<String>, rect: egui::Rect) {
    #[cfg(test)]
    record_measurement(&SETTINGS_CARD_MEASUREMENTS, label, rect);

    #[cfg(not(test))]
    let _ = (label, rect);
}

#[cfg(test)]
fn record_measurement(
    measurements: &'static std::thread::LocalKey<
        std::cell::RefCell<Vec<SettingsControlMeasurement>>,
    >,
    label: impl Into<String>,
    rect: egui::Rect,
) {
    measurements.with(|measurements| {
        measurements.borrow_mut().push(SettingsControlMeasurement {
            label: label.into(),
            width: rect.width(),
            center_x: rect.center().x,
            center_y: rect.center().y,
            right_x: rect.right(),
        });
    });
}

pub(super) fn active_settings_control_lane() -> Option<egui::Rect> {
    SETTINGS_CONTROL_LANE.with(|lane| lane.get())
}

pub(super) fn with_settings_control_lane<R>(
    rect: egui::Rect,
    add_contents: impl FnOnce() -> R,
) -> R {
    let _guard = SettingsControlLaneGuard::push(rect);
    add_contents()
}

struct SettingsControlLaneGuard {
    previous: Option<egui::Rect>,
}

impl SettingsControlLaneGuard {
    fn push(rect: egui::Rect) -> Self {
        let previous = SETTINGS_CONTROL_LANE.with(|lane| {
            let previous = lane.get();
            lane.set(Some(rect));
            previous
        });
        Self { previous }
    }
}

impl Drop for SettingsControlLaneGuard {
    fn drop(&mut self) {
        SETTINGS_CONTROL_LANE.with(|lane| lane.set(self.previous));
    }
}
