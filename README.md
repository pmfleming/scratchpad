# Scratchpad

**Scratchpad** is a modern, lightweight, tabbed text editor built with Rust and the [egui](https://github.com/emilk/egui) framework. It features a custom-built, frameless UI designed for focus and performance.

![Scratchpad Preview](https://via.placeholder.com/800x450.png?text=Scratchpad+Preview+Placeholder)

## ✨ Key Features

-   🗂️ **Dynamic Tabbed Interface**: Effortlessly manage multiple files simultaneously.
-   🎨 **Custom Frameless UI**: A bespoke dark-themed interface with custom-drawn caption buttons (Minimize, Maximize, Close).
-   📂 **Native System Integration**: Uses native file dialogs via `rfd` for a seamless OS experience.
-   💾 **Smart Dirty Tracking**: Visual indicators (`*`) for unsaved changes and protective confirmation when closing an individual tab.
-   🔄 **Session Restore**: Open tabs, active tab, wrap setting, zoom level, and unsaved edits are snapshotted to temp files and restored on the next launch.
-   🧾 **Safer Exit Handling**: Closing the window writes the latest session snapshot first, so the app reopens to the same state without an app-exit unsaved warning.
-   ↩️ **Word Wrap Toggle**: Switch wrapping on or off from the File menu.
-   🔍 **Interactive Zoom**: Quickly adjust your view with `Ctrl` + `Mouse Wheel` to resize font on the fly.
-   ⚡ **Productivity Shortcuts**:
    -   `Ctrl + N`: New Tab
    -   `Ctrl + O`: Open File
    -   `Ctrl + S`: Save File
    -   `Ctrl + W`: Close Tab
-   🏗️ **Modular Architecture**: Cleanly separated logic for UI components, theme constants, and state management.

## 🛠️ Tech Stack

-   **Language**: [Rust](https://www.rust-lang.org/) (2024 Edition)
-   **GUI Framework**: [egui](https://github.com/emilk/egui) & [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) (Immediate mode GUI)
-   **File Dialogs**: [rfd](https://github.com/PolyMeilex/rfd) (Rust File Dialogs)
-   **Image Handling**: [image](https://github.com/image-rs/image) crate for custom icon rendering.

## 🚀 Getting Started

### Prerequisites

-   [Rust Toolchain](https://rustup.rs/) installed on your machine.
-   A Windows environment. This project and its CI pipeline intentionally target Windows only.

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

### Quality Checks

Run the same checks used by CI:

```powershell
.\scripts\ci.ps1
```

If you only want to apply formatting first:

```powershell
.\scripts\ci.ps1 -FixFormatting
```

## 📂 Project Structure

```text
src/
├── main.rs          # Application entry point & window setup
├── assets/          # Custom PNG icons and UI assets
└── app/             # Core application logic
    ├── mod.rs       # Main App state & UI loop
    ├── chrome.rs    # Reusable UI components & icon loading
    ├── tabs.rs      # Tab state & buffer management
    └── theme.rs     # Centralized color palette & layout constants
```

## 📝 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
