# Scratchpad

**Scratchpad** is a modern, lightweight, tabbed text editor built with Rust and the [egui](https://github.com/emilk/egui) framework. It features a custom-built, frameless UI designed for focus and performance.

![Scratchpad Preview](https://via.placeholder.com/800x450.png?text=Scratchpad+Preview+Placeholder)

## ✨ Key Features

-   🗂️ **Dynamic Tabbed Interface**: Effortlessly manage multiple files with a smart tab strip that includes horizontal scrolling and an overflow dropdown for easy navigation.
-   🎨 **Custom Frameless UI**: A bespoke high-contrast dark theme with pure white text for maximum readability and perfectly aligned controls.
-   💎 **Modern Iconography**: Fully integrated with [Phosphor Icons](https://phosphoricons.com/) via `egui-phosphor` for a crisp, professional look.
-   📂 **Native System Integration**: Seamlessly open and save files using native OS dialogs via `rfd`.
-   💾 **Smart Dirty Tracking**: Visual indicators (`*`) for unsaved changes and protective confirmation modals to prevent data loss.
-   🔄 **Session Restore**: Your entire workspace—open tabs, active tab, wrap settings, and zoom level—is automatically saved and restored on the next launch.
-   🔍 **Precision Zoom**: Fine-tune your view with `Ctrl` + `Mouse Wheel` or dedicated keyboard shortcuts to resize the editor font independently of the UI.
-   ⚡ **Productivity Shortcuts**:
    -   `Ctrl + N`: New Tab
    -   `Ctrl + O`: Open File
    -   `Ctrl + S`: Save File
    -   `Ctrl + W`: Close Tab
    -   `Ctrl + +` / `Ctrl + -`: Zoom In/Out
    -   `Ctrl + 0`: Reset Zoom
-   🏗️ **Modular Architecture**: Built with a clean separation between UI components, domain logic, and services.

## 🛠️ Tech Stack

-   **Language**: [Rust](https://www.rust-lang.org/) (2024 Edition)
-   **GUI Framework**: [egui](https://github.com/emilk/egui) & [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
-   **Icons**: [egui-phosphor](https://crates.io/crates/egui-phosphor)
-   **File Dialogs**: [rfd](https://github.com/PolyMeilex/rfd)
-   **Serialization**: [serde](https://serde.rs/) & [serde_json](https://github.com/serde-rs/json)

## 🚀 Getting Started

### Prerequisites

-   [Rust Toolchain](https://rustup.rs/) installed.
-   A Windows environment (the project and CI pipeline target Windows specifically).

### Building and Running

1.  Clone the repository:
    ```bash
    git clone https://github.com/your-username/scratchpad.git
    cd scratchpad
    ```
2.  Launch the application:
    ```bash
    cargo run --release
    ```

## 📂 Project Structure

```text
src/
├── main.rs          # Application entry point and font initialization
└── app/             # Modular application logic
    ├── mod.rs       # Composition root and shortcuts
    ├── app_state.rs # Main application state and command handling
    ├── chrome.rs    # Reusable UI components and window chrome
    ├── theme.rs     # Centralized color palette and layout constants
    ├── domain/      # Business logic (Buffer, Tab)
    ├── services/    # Infrastructure (Session Store)
    └── ui/          # egui rendering (Tab Strip, Editor Area, Dialogs)
```

## 🗺️ Roadmap

- [ ] **Multi-Pane Layout**: Support splitting the editor into multiple vertical/horizontal panes.
- [ ] **Search & Replace**: Integrated find and replace overlay.
- [ ] **Line Numbers**: Gutter with line counts for better navigation.
- [ ] **Tab Drag & Drop**: Reorder tabs via dragging.
- [ ] **Config File**: Persistent user settings for default themes and font choices.

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
