use super::{PendingActionKind, ScratchpadApp};
use crate::app::services::instance_broker::BrokerInbox;
use eframe::egui;

impl ScratchpadApp {
    pub fn attach_instance_broker(&mut self, inbox: BrokerInbox) {
        self.state.broker_inbox = Some(inbox);
    }

    pub(crate) fn apply_pending_broker_launches(&mut self, ctx: &egui::Context) {
        if self
            .state
            .broker_inbox
            .as_ref()
            .is_some_and(|inbox| inbox.take_pending())
        {
            self.collect_broker_launches();
        }
        if self.startup_restore_pending() || self.state.deferred_launch_requests.is_empty() {
            return;
        }

        while let Some(request) = self.state.deferred_launch_requests.pop_front() {
            let activate = request.activate;
            self.apply_startup_options_async(request.options);
            if activate {
                activate_primary_window(ctx);
            }
        }
    }

    fn collect_broker_launches(&mut self) {
        let mut received = Vec::new();
        if let Some(inbox) = self.state.broker_inbox.as_ref() {
            while let Ok(request) = inbox.try_recv() {
                received.push(request);
            }
        }
        self.state.deferred_launch_requests.extend(received);
    }

    fn startup_restore_pending(&self) -> bool {
        self.state
            .background_io
            .pending_background_actions
            .values()
            .any(|action| action.kind() == PendingActionKind::StartupRestore)
    }
}

fn activate_primary_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
        egui::UserAttentionType::Informational,
    ));
}

#[cfg(test)]
mod tests {
    use crate::app::services::instance_broker::LaunchRequest;
    use crate::app::startup::StartupOptions;

    #[test]
    fn activation_only_request_has_no_files() {
        let request = LaunchRequest {
            invocation_id: 1,
            sender_pid: 2,
            options: StartupOptions::default(),
            activate: true,
        };
        assert!(request.options.files.is_empty());
        assert!(request.activate);
    }
}
