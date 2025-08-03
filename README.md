# Extract frames

[![unit tests](https://github.com/egorsmkv/read-video-rs/actions/workflows/test.yml/badge.svg)](https://github.com/egorsmkv/read-video-rs/actions/workflows/test.yml)
[![security audit](https://github.com/egorsmkv/extract-frames-rs/actions/workflows/audit.yml/badge.svg)](https://github.com/egorsmkv/extract-frames-rs/actions/workflows/audit.yml)

This program uses FFmpeg to extract frames from a video file with FPS=1 in parallel.

## Usage

```
extract-frames --file video.mp4 --multicore
```

## Development

You need to use these tools to make changes in this project:

- `cargo install action-validator repocat just`
- `brew install lefthook`
- downloaded nightly version of rustc, clippy
- [gemma-cli](https://github.com/egorsmkv/gemma-cli)
- [yamlfmt](https://github.com/google/yamlfmt)
