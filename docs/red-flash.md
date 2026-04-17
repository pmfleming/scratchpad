# Investigation Report: Red Flashes During Editor Layout Changes

## Overview
This report investigates the "red flash" visual artifacts that occur when the shape of the editor screen changes (such as resizing splits or moving tabs). In `egui`/`eframe` applications, these flashes typically stem from layout recalculation delays, native window clear colors, or 1-frame delays in component sizing.

Below are the 5 most likely causes for this issue, along with their simplest solutions.

---

### 1. Unconfigured Native Window Clear Color
**Cause:**
When the OS window resizes or when the UI tree completely changes during a tab drag/drop, the graphics backend (e.g., `wgpu`) clears the screen before `egui` draws the next frame. Currently, `eframe::NativeOptions` in `src/main.rs` does not explicitly define a `clear_color`. If the OS or backend defaults to a harsh color (or uninitialized memory that appears as a color flash), any 1-frame gap in rendering will flash that color.

**Simplest Solution:**
Explicitly configure the `clear_color` in `eframe::NativeOptions` to match the application's base dark background color (`crate::app::theme::EDITOR_BG` or similar).
```rust
let options = eframe::NativeOptions {
    viewport: egui::ViewportBuilder::default()
        .with_decorations(false)
        .with_inner_size([960.0, 640.0])
        .with_min_inner_size([400.0, 300.0]),
    clear_color: egui::Color32::from_rgb(21, 24, 29), // Editor Background
    ..Default::default()
};
```

### 2. 1-Frame Layout Delay in ScrollArea
**Cause:**
`egui::ScrollArea` requires one frame to measure the layout size of its inner content. When the container (e.g., a tiled editor pane) shrinks suddenly during a layout change, the `ScrollArea` momentarily retains its old, larger bounds. This can cause the parent container to clip or misalign, exposing the underlying window clear color at the edges.

**Simplest Solution:**
Ensure that all parent containers explicitly paint their bounds with a solid background *before* the `ScrollArea` is rendered. For instance, in `src/app/ui/editor_area/tile.rs`, ensure that the `paint_tile_frame` covers the entire allocated rect solidly so that if the `ScrollArea` misaligns for one frame, it only exposes the intended editor background. Alternatively, configure the scroll areas with `.auto_shrink([false, false])` to stabilize their bounds.

### 3. Missing Base Fill in the CentralPanel
**Cause:**
The primary workspace uses `egui::CentralPanel::default().show_inside(ui, ...)`. By default, `CentralPanel` uses the current theme's `panel_fill`. If there are any gaps between the tiles or tab strips during a complex drag-and-drop hierarchy transition, the underlying panel background (or clear color) shows through.

**Simplest Solution:**
At the start of `show_editor` (in `src/app/ui/editor_area/mod.rs`), allocate the available workspace rect and explicitly fill it with the editor's base background color.
```rust
let workspace_rect = ui.available_rect_before_wrap();
ui.painter().rect_filled(workspace_rect, 0.0, app.editor_background_color());
```

### 4. Sub-Pixel Gaps During Split Calculation (Rounding Errors)
**Cause:**
When calculating dynamic split ratios in `src/app/ui/tile_header/split/geometry.rs`, floating-point arithmetic can result in fractional pixel sizes. When `egui` rasterizes these bounds, it can leave 1-pixel hairline gaps between adjacent tiles or dividers, which flash when the split is actively being dragged.

**Simplest Solution:**
When calculating tile rectangles, expand the painted background (`paint_tile_frame`) by 1 pixel or explicitly use `rect.expand(0.5)` when calling `ui.painter().rect_filled()`. This forces the tile backgrounds to overlap slightly, hiding any sub-pixel gaps caused by floating-point rounding.

### 5. Delayed Focus and State Synchronization
**Cause:**
When dropping a tab to create a new split, the UI instantiates a new `EditorViewState` and `RenderedLayout`. For the first frame, `egui::TextEdit` might lack its proper tracked layouter constraints or the `ScrollArea` might not have its target focus offset. This causes the widget to render with an empty or collapsed layout for exactly one frame, exposing the background behind it.

**Simplest Solution:**
When committing a split or tab drop action (e.g., in `src/app/ui/tile_header/split/drag.rs` or `src/app/ui/tab_strip/outcome.rs`), force an immediate subsequent UI frame by calling `ui.ctx().request_repaint()`. This ensures that the 1-frame layout artifact is instantly resolved, making the "flash" practically invisible to the user.
