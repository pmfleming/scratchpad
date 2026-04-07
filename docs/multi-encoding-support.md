# Plan: Support Multi-Encoding and Large Files

This plan outlines the changes needed to support various character encodings and improve the app's ability to handle large text files.

## Objective
- Support opening files in encodings other than UTF-8 (e.g., UTF-16, Shift-JIS, Windows-1252).
- Detect encoding automatically using Byte Order Marks (BOM) or heuristics.
- Optimize the loading process for larger files and provide user feedback when performance may be impacted.
- Ensure that saving a file preserves its original encoding.

## Key Files & Context
- `Cargo.toml`: Add `encoding_rs`, `encoding_rs_io`, and `chardetng`.
- `src/app/services/file_service.rs`: New service for robust I/O.
- `src/app/domain/buffer.rs`: Update `BufferState` to track encoding.
- `src/app/app_state.rs`: Integrate the new file service.
- `src/app/services/session_store.rs`: Update session restoration to use the file service.
- `src/app/ui/editor_area.rs`: Display encoding information and warnings.

## Implementation Steps

### 1. Add Dependencies
- Add the following crates to `Cargo.toml`:
  - `encoding_rs`: Core encoding/decoding logic.
  - `encoding_rs_io`: Streaming decoder with BOM support.
  - `chardetng`: Heuristic encoding detection.

### 2. Update `BufferState`
- Add a `pub encoding: String` field to `BufferState`.
- Initialize it with the detected encoding name during load, or `"UTF-8"` for new files.
- Ensure the encoding is preserved during session save/restore.

### 3. Implement `FileService`
- Create `src/app/services/file_service.rs` with the following:
  - `read_file(path: &Path) -> io::Result<(String, String)>`:
    - Reads a small prefix (e.g., 4KB) to detect encoding via `chardetng`.
    - Uses `encoding_rs_io::DecodeReaderBytes` with BOM override to decode the entire file into a `String`.
    - Checks for binary content (e.g., null bytes) and returns an error if the file doesn't appear to be text.
  - `write_file(path: &Path, content: &str, encoding_name: &str) -> io::Result<()>`:
    - Looks up the `encoding_rs::Encoding` by name.
    - Encodes the UTF-8 string back to the target encoding and writes it to disk.

### 4. Integrate with App Logic
- **App State**: Replace `fs::read_to_string` in `open_file` with `FileService::read_file`. Store the returned encoding name in the new buffer.
- **Session Store**: Update `SessionStore::load` to use `FileService::read_file` when falling back to original file paths.
- **Saving**: Update `save_file_at` to use `FileService::write_file` so files are saved back in their original encoding.

### 5. UI and Performance
- **Status Bar**: Display the current file's encoding (e.g., "UTF-8", "UTF-16LE").
- **Large File Warning**: If a file exceeds a threshold (e.g., 5MB), display a warning in the status bar indicating that performance might be degraded due to the file size.
- **Binary Detection**: If a user tries to open a binary file, show a clear error message instead of failing silently or showing garbled text.

## Verification & Testing
- **Multi-Encoding**: Create test files in UTF-16, Shift-JIS, and Windows-1252. Verify they open correctly and their content is legible.
- **Encoding Preservation**: Modify a Shift-JIS file, save it, and verify using an external tool that it remains Shift-JIS.
- **Large Files**: Open a 20MB text file and verify the app remains responsive (after the initial load).
- **Binary Files**: Attempt to open an image or executable and verify the app shows a "Binary files are not supported" error.
