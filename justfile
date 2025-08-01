init:
    cargo install action-validator repocat just
    brew install lefthook

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
    repocat --root . --include "*.rs" --exclude "*.lock,*.bak" > repo_content.txt

llm_non_idiomatic:
    rm -f llm_non_idiomatic.txt
    echo "Analyze Rust code below and find non-idiomatic code:\n" >> llm_non_idiomatic.txt
    repocat --root . --include "*.rs" --exclude "*.lock,*.bak" >> llm_non_idiomatic.txt
