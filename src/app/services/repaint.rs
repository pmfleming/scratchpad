//! Result publication wakes the UI without polling a pending worker.
use eframe::egui;
use std::sync::{Arc, Mutex, mpsc};

#[derive(Clone, Default)]
pub(crate) struct RepaintSignal(Arc<Mutex<Option<egui::Context>>>);

impl RepaintSignal {
    pub(crate) fn bind(&self, ctx: &egui::Context) {
        let changed = {
            let mut current = self.0.lock().unwrap_or_else(|e| e.into_inner());
            if current.as_ref() == Some(ctx) {
                false
            } else {
                *current = Some(ctx.clone());
                true
            }
        };
        // A result may already have arrived before the first logic pass.
        if changed {
            self.notify();
        }
    }

    pub(crate) fn notify(&self) {
        let ctx = self.0.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    }
}

pub(crate) struct WakeSender<T> {
    sender: mpsc::Sender<T>,
    signal: RepaintSignal,
}

impl<T> Clone for WakeSender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            signal: self.signal.clone(),
        }
    }
}

impl<T> WakeSender<T> {
    pub(crate) fn new(sender: mpsc::Sender<T>, signal: RepaintSignal) -> Self {
        Self { sender, signal }
    }
    pub(crate) fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        self.sender.send(value)?;
        self.signal.notify();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Arc, RepaintSignal, WakeSender, egui, mpsc};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn publication_from_another_thread_wakes_after_enqueuing_the_result() {
        let ctx = egui::Context::default();
        let callbacks = Arc::new(AtomicUsize::new(0));
        let received = callbacks.clone();
        ctx.set_request_repaint_callback(move |_| {
            received.fetch_add(1, Ordering::SeqCst);
        });
        let signal = RepaintSignal::default();
        signal.bind(&ctx);
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
            output.textures_delta.clear();
        }
        callbacks.store(0, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        let tx = WakeSender::new(tx, signal);
        std::thread::spawn(move || tx.send(42).unwrap())
            .join()
            .unwrap();
        assert_eq!(rx.try_recv().unwrap(), 42);
        assert!(callbacks.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn binding_after_publication_and_disconnected_receivers_are_safe() {
        let signal = RepaintSignal::default();
        let (tx, rx) = mpsc::channel();
        let tx = WakeSender::new(tx, signal.clone());
        tx.send(7).unwrap();
        signal.bind(&egui::Context::default());
        assert_eq!(rx.try_recv().unwrap(), 7);
        drop(rx);
        assert_eq!(tx.send(9).unwrap_err().0, 9);
    }
}
