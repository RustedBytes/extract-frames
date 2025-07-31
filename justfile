check: fmt
    cargo clippy -- -W clippy::pedantic

check_fmt:
    cargo fmt -- --check

fmt:
    cargo fmt

test:
    cargo test

doc:
    cargo doc --open

release: check
    cargo build --release
