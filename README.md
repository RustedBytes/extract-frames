# Extract Frames

[![unit tests](https://github.com/egorsmkv/read-video-rs/actions/workflows/test.yml/badge.svg)](https://github.com/egorsmkv/read-video-rs/actions/workflows/test.yml)
[![security audit](https://github.com/egorsmkv/extract-frames-rs/actions/workflows/audit.yml/badge.svg)](https://github.com/egorsmkv/extract-frames-rs/actions/workflows/audit.yml)

A Rust-based command-line application for extracting frames from video files using FFmpeg, supporting both sequential and parallel processing modes.

## Table of Contents

* [Features](#features)
* [Usage](#usage)
* [Command Line Arguments](#command-line-arguments)
* [Requirements](#requirements)
* [Development](#development)
* [Building and Testing](#building-and-testing)
* [Known Limitations](#known-limitations)
* [Contributing](#contributing)
* [Troubleshooting](#troubleshooting)

## Features

* Extract frames at specified intervals or one frame per second
* Support for parallel processing using multiple CPU cores
* Experimental seek-based extraction method for consistent temporal sampling
* Robust error handling for file operations and FFmpeg interactions
* Comprehensive test suite for core functionality

## Usage

The application provides several command-line options to control frame extraction.

### Basic Usage

To extract frames from a video file using default settings (every 30th frame):

```bash
cargo run -- --file input.mp4
```

The extracted frames will be saved as PNG files in the `frames` directory.

### Extract One Frame Per Second

To extract one frame per second using the experimental seek-based method:

```bash
cargo run -- --file input.mp4 --use-seek
```

### Parallel Processing

To enable parallel processing by splitting the video into segments and processing them concurrently:

```bash
cargo run -- --file input.mp4 --multicore
```

## Command Line Arguments

* `--file <PATH>`: Specify input video file (default: "video.mp4")
* `--use-seek`: Enable experimental seek-based frame extraction (one frame per second)
* `--multicore`: Enable parallel processing using multiple CPU cores

## Requirements

* FFmpeg installed and available in system PATH
* Rust toolchain (including cargo)

## Development

To contribute to this project, you'll need:

1. Rust toolchain (nightly version recommended)
2. `cargo install action-validator dircat just`
3. `brew install lefthook` (for pre-commit hooks)
4. [gemma-cli](https://github.com/egorsmkv/gemma-cli) (for LLM interactions)
5. [yamlfmt](https://github.com/google/yamlfmt) (for YAML formatting)

## Building and Testing

1. Clone the repository
2. Run `cargo build` to compile the application
3. Run `cargo test` to execute the test suite
4. Run `cargo run -- --help` to see command-line options

## Known Limitations

* The seek-based method is experimental and may not produce accurate results due to keyframe spacing issues
* Parallel processing requires FFmpeg to be installed and available in PATH

## Contributing

Contributions are welcome! Please submit pull requests with clear descriptions of changes and ensure that all tests pass before submitting.

## Troubleshooting

* If you encounter issues with FFmpeg, ensure it's installed and available in your system's PATH.
* If you experience errors during parallel processing, verify that your system has sufficient resources (CPU cores and memory).
* For other issues, please check the [issues page](https://github.com/egorsmkv/extract-frames-rs/issues) or submit a new issue with detailed information about your problem.
