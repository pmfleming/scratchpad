#![windows_subsystem = "windows"]

extern crate native_windows_gui as nwg;

use std::cell::RefCell;

struct TabPage {
    _tab: nwg::Tab,
    _editor: nwg::TextBox,
    _layout: nwg::GridLayout,
}

#[derive(Default)]
struct EditorApp {
    window: nwg::Window,
    window_layout: nwg::GridLayout,
    tabs: nwg::TabsContainer,
    pages: RefCell<Vec<TabPage>>,
}

impl EditorApp {
    fn build() -> Result<Self, nwg::NwgError> {
        let mut app = Self::default();

        nwg::Window::builder()
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
            .size((960, 640))
            .position((120, 120))
            .title("Rust Notepad")
            .build(&mut app.window)?;

        nwg::TabsContainer::builder()
            .parent(&app.window)
            .build(&mut app.tabs)?;

        nwg::GridLayout::builder()
            .parent(&app.window)
            .spacing(0)
            .margin([0, 0, 0, 0])
            .child(0, 0, &app.tabs)
            .build(&app.window_layout)?;

        app.add_tab("Untitled 1", "")?;
        app.add_tab(
            "Welcome",
            "Native Win32 tab scaffold is up.\r\n\r\nPhase 1 target:\r\n- window\r\n- tabs\r\n- multiline editor\r\n",
        )?;

        Ok(app)
    }

    fn add_tab(&self, title: &str, text: &str) -> Result<(), nwg::NwgError> {
        let mut tab = nwg::Tab::default();
        let mut editor = nwg::TextBox::default();
        let layout = nwg::GridLayout::default();

        nwg::Tab::builder()
            .text(title)
            .parent(&self.tabs)
            .build(&mut tab)?;

        nwg::TextBox::builder()
            .parent(&tab)
            .text(text)
            .focus(self.pages.borrow().is_empty())
            .flags(
                nwg::TextBoxFlags::VISIBLE
                    | nwg::TextBoxFlags::TAB_STOP
                    | nwg::TextBoxFlags::VSCROLL
                    | nwg::TextBoxFlags::HSCROLL
                    | nwg::TextBoxFlags::AUTOVSCROLL
                    | nwg::TextBoxFlags::AUTOHSCROLL,
            )
            .build(&mut editor)?;

        nwg::GridLayout::builder()
            .parent(&tab)
            .spacing(0)
            .margin([0, 0, 0, 0])
            .child(0, 0, &editor)
            .build(&layout)?;

        self.pages.borrow_mut().push(TabPage {
            _tab: tab,
            _editor: editor,
            _layout: layout,
        });

        Ok(())
    }
}

fn main() {
    nwg::init().expect("failed to initialize Native Windows GUI");
    nwg::Font::set_global_family("Segoe UI").expect("failed to set default font");

    let app = EditorApp::build().expect("failed to build the editor shell");
    let handle = app.window.handle;

    let event_handler = nwg::full_bind_event_handler(&handle, move |event, _event_data, control| {
        if event == nwg::Event::OnWindowClose && control == handle {
            nwg::stop_thread_dispatch();
        }
    });

    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&event_handler);
}
