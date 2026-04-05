# Scratchpad

**Scratchpad** is a modern, lightweight, tabbed text editor built with Rust and the [egui](https://github.com/emilk/egui) framework. It features a custom-built, frameless UI designed for focus and performance.

![Scratchpad Preview](https://via.placeholder.com/800x450.png?text=Scratchpad+Preview+Placeholder)

## ✨ Key Features

-   🗂️ **Dynamic Tabbed Interface**: Effortlessly manage multiple files with a smart tab strip that includes horizontal scrolling and an overflow dropdown for easy navigation.
-   🎨 **Custom Frameless UI**: A bespoke high-contrast dark theme with pure white text for maximum readability and custom-drawn caption buttons.
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
-   🏗️ **Modular Architecture**: Built with a clean separation between UI components, theme constants, and application state.

## 🛠️ Tech Stack

-   **Language**: [Rust](https://www.rust-lang.org/) (2024 Edition)
-   **GUI Framework**: [egui](https://github.com/emilk/egui) & [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)
-   **File Dialogs**: [rfd](https://github.com/PolyMeilex/rfd)
-   **Image Handling**: [image](https://github.com/image-rs/image) crate for custom icon rendering.

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
├── main.rs          # Application entry point, visuals, and window setup
├── assets/          # Custom PNG icons and UI assets
└── app/             # Core application logic
    ├── mod.rs       # Main App state, UI loop, and keyboard handling
    ├── chrome.rs    # Reusable UI components (tabs, buttons) and icon loading
    ├── tabs.rs      # Tab state management and buffer logic
    └── theme.rs     # Centralized color palette and layout constants
```

## 🗺️ Roadmap

- [ ] **Search & Replace**: Integrated find and replace overlay (see [Search & Replace Plan](docs/search-replace-plan.md)).
- [ ] **Line Numbers**: Gutter with line counts for better navigation.
- [ ] **Tab Drag & Drop**: Reorder tabs via dragging.
- [ ] **Config File**: Persistent user settings for default themes and font choices.

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
