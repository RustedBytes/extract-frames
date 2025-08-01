# Extract frames

[![unit tests](https://github.com/egorsmkv/read-video-rs/actions/workflows/test.yml/badge.svg)](https://github.com/egorsmkv/read-video-rs/actions/workflows/test.yml)

This program uses FFmpeg to extract frames from a video file with FPS=1 in parallel.

## Usage

```
./target/release/extract-frames --file video.mp4 --multicore
```

## Development

You need to use these tools to make changes in this project:

- `cargo install action-validator`
- `cargo install repocat`
- `cargo install just`
- `brew install lefthook`
- downloaded nightly version of rustc, clippy
