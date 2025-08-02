set dotenv-load := true

init:
    cargo install action-validator repocat just
    brew install lefthook

check: fmt
    cargo +nightly clippy -- -W clippy::pedantic

check_fmt:
    cargo +nightly fmt -- --check

fmt_yaml:
    yamlfmt lefthook.yml
    yamlfmt -dstar .github/**/*.{yaml,yml}

fmt: fmt_yaml
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

llm_grammar_check: repocat
    gemma-cli -model=gemma-3-12b-it -prompt=.llms/prompts/grammar_check.md -input=/tmp/code.txt -output=.llms/grammar_check.md

llm_non_idiomatic: repocat
    gemma-cli -model=gemini-2.5-pro -prompt=.llms/prompts/non_idiomatic.md -input=/tmp/code.txt -output=.llms/non_idiomatic.md

llm_improve_comments: repocat
    gemma-cli -model=gemma-3-12b-it -prompt=.llms/prompts/improve_comments.md -input=/tmp/code.txt -output=.llms/improve_comments.md

llm_not_tested: repocat
    gemma-cli -model=gemma-3-12b-it -prompt=.llms/prompts/not_tested.md -input=/tmp/code.txt -output=.llms/not_tested.md

llm_llama_grammar_check: repocat
    python3 .llms/inference/hf.py --model "meta-llama/Llama-4-Scout-17B-16E-Instruct:cerebras" --prompt=.llms/prompts/grammar_check.md --input=/tmp/code.txt --output=.llms/llama_grammar_check.md

llm_maverick_tests_enhancement: repocat
    python3 .llms/inference/hf.py --model "meta-llama/Llama-4-Maverick-17B-128E-Instruct:cerebras" --max_tokens 65000 --prompt=.llms/prompts/enhance_tests.md --input=/tmp/code.txt --output=.llms/maverick_tests_enhancement.md

llm_maverick_enhance_readme:
    repocat --root . --include "*.rs,*.md" > /tmp/code-and-readme.txt
    python3 .llms/inference/hf.py --model "meta-llama/Llama-4-Maverick-17B-128E-Instruct:cerebras" --max_tokens 8000 --prompt=.llms/prompts/enhance_readme.md --input=/tmp/code-and-readme.txt --output=.llms/maverick_readme.md

llm_qwen3_coder_non_idiomatic: repocat
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/non_idiomatic.md --input=/tmp/code.txt --output=.llms/qwen3_non_idiomatic.md

llm_qwen3_coder_improve_comments: repocat
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/improve_comments.md --input=/tmp/code.txt --output=.llms/qwen3_improve_comments.md

llm_qwen3_code_review: repocat
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/code_review.md --input=/tmp/code.txt --output=.llms/qwen3_code_review.md

llm_glm45air_code_review: repocat
    python3 .llms/inference/hf.py --model "zai-org/GLM-4.5-Air-FP8:together" --max_tokens 96000 --prompt=.llms/prompts/code_review.md --input=/tmp/code.txt --output=.llms/glm45air_code_review.md
