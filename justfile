set dotenv-load := true

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

repocat:
    repocat --root . --include "*.rs" > /tmp/code.txt

llm_non_idiomatic: repocat
    gemma-cli -model=gemini-2.5-pro -prompt=.llms/prompts/non_idiomatic.md -input=/tmp/code.txt -output=.llms/non_idiomatic.md

llm_improve_comments: repocat
    gemma-cli -model=gemma-3-12b-it -prompt=.llms/prompts/improve_comments.md -input=/tmp/code.txt -output=.llms/improve_comments.md

llm_not_tested: repocat
    gemma-cli -model=gemma-3-12b-it -prompt=.llms/prompts/not_tested.md -input=/tmp/code.txt -output=.llms/not_tested.md

llm_qwen3_coder_non_idiomatic: repocat
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/non_idiomatic.md --input=/tmp/code.txt --output=.llms/qwen3_non_idiomatic.md

llm_qwen3_coder_improve_comments: repocat
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/improve_comments.md --input=/tmp/code.txt --output=.llms/qwen3_improve_comments.md
