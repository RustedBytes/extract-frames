set dotenv-load := true

dev-deps:
    cargo install cargo-audit action-validator dircat
    cargo install --git https://github.com/ytmimi/markdown-fmt markdown-fmt --features="build-binary"
    cargo install --git https://github.com/RustedBytes/invoke-llm

init-macos: dev-deps
    brew install lefthook

init-linux: dev-deps
    go install github.com/evilmartians/lefthook@latest
    go install github.com/google/yamlfmt/cmd/yamlfmt@latest

check-dev:
    lefthook version
    action-validator --version
    dircat --version
    cargo audit --version
    yamlfmt --version
    markdown-fmt --version
    cargo --version
    rustc --version
    git --version
    yasm --version

check: fmt
    cargo +nightly clippy -- -W clippy::pedantic

check-fmt:
    cargo +nightly fmt -- --check

yaml-fmt:
    yamlfmt lefthook.yml
    yamlfmt -dstar .github/**/*.{yaml,yml}

md-fmt:
    markdown-fmt -m 80 CONTRIBUTING.md
    markdown-fmt -m 80 README.md

fmt: yaml-fmt md-fmt
    cargo +nightly fmt

test:
    cargo test

audit:
    cargo audit -D warnings

doc:
    cargo doc --open

build: check
    cargo +stable build

release: check
    cargo +stable build --release

download-test-video:
    wget -O "video.mp4" "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4"
