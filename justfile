check: fmt
    cargo +nightly clippy -- -W clippy::pedantic

check_fmt:
    cargo +nightly fmt -- --check

fmt:
    cargo +nightly fmt

test:
    cargo test

doc:
    cargo doc --open

release: check
    cargo build --release

download_test_video:
    wget -O "video.mp4" "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4"

llm_cat:
    repocat --root . --include "*.rs,*.toml,*.yml,*.md" --exclude "*.lock,*.bak" > repo_content.txt
