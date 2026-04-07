# `rust-code-analysis-cli` Quick Overview

`rust-code-analysis-cli` is a source analysis tool for inspecting code structure and metrics.

## Basic Usage

```powershell
rust-code-analysis-cli [OPTIONS]
```

Common input options:

- `--paths <PATHS>`: Files or directories to analyze.
- `--include <GLOB>`: Limit analysis to matching files.
- `--exclude <GLOB>`: Exclude matching files.
- `--language-type <LANGUAGE>`: Force a language when needed.
- `--num-jobs <N>`: Control parallelism.

## Main Commands

- `--metrics`: Compute code metrics such as cyclomatic complexity, cognitive complexity, Halstead metrics, LOC, MI, and ABC.
- `--dump`: Print the parsed AST.
- `--comments`: Remove comments from the specified files.
- `--find <NODE>`: Find syntax nodes of a given type.
- `--function`: List functions and their spans.
- `--count <NODE1,NODE2,...>`: Count specific syntax node types.
- `--ops`: Show operators and operands found in the code.
- `--preproc <MODE>`: Preprocessor-related output for C/C++ codebases.

## Output Options

- `--output <PATH>`: Write results to a file or directory.
- `--output-format <FORMAT>`: Emit `json`, `yaml`, `toml`, or `cbor`.
- `--pr`: Pretty-print JSON output.
- `--warning`: Print warnings.

## Examples

Generate metrics for the Rust source tree:

```powershell
rust-code-analysis-cli --metrics --paths src
```

Write metrics as JSON:

```powershell
rust-code-analysis-cli --metrics --paths src --output-format json --output metrics.json
```

List functions in one file:

```powershell
rust-code-analysis-cli --function --paths src/app/app_state.rs
```

Find `if_statement` nodes:

```powershell
rust-code-analysis-cli --find if_statement --paths src
```
