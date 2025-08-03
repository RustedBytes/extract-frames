set dotenv-load := true

init:
    cargo install action-validator dircat just
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

audit:
    cargo audit -D warnings

doc:
    cargo doc --open

release: check
    cargo build --release

download_test_video:
    wget -O "video.mp4" "https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ElephantsDream.mp4"

llm_ctx:
    dircat -b -e rs -o ctx.md .

llm_grammar_check: llm_ctx
    gemma-cli -model=gemma-3-12b-it -prompt=.llms/prompts/grammar_check.md -input=ctx.md -output=.llms/grammar_check.md

llm_non_idiomatic: llm_ctx
    gemma-cli -model=gemini-2.5-pro -prompt=.llms/prompts/non_idiomatic.md -input=ctx.md -output=.llms/non_idiomatic.md

llm_improve_comments: llm_ctx
    gemma-cli -model=gemma-3-12b-it -prompt=.llms/prompts/improve_comments.md -input=ctx.md -output=.llms/improve_comments.md

llm_llama_grammar_check: llm_ctx
    python3 .llms/inference/hf.py --model "meta-llama/Llama-4-Scout-17B-16E-Instruct:cerebras" --prompt=.llms/prompts/grammar_check.md --input=ctx.md --output=.llms/llama_grammar_check.md

llm_maverick_tests_enhancement: llm_ctx
    python3 .llms/inference/hf.py --model "meta-llama/Llama-4-Maverick-17B-128E-Instruct:cerebras" --max_tokens 65000 --prompt=.llms/prompts/enhance_tests.md --input=ctx.md --output=.llms/maverick_tests_enhancement.md

llm_maverick_enhance_readme: llm_ctx
    echo "" >> ctx.md
    echo "Source of of README file:" >> ctx.md
    echo  "\`\`\`markdown" >> ctx.md
    cat README.md >> ctx.md
    echo "\`\`\`" >> ctx.md
    python3 .llms/inference/hf.py --model "meta-llama/Llama-4-Maverick-17B-128E-Instruct:cerebras" --max_tokens 8000 --prompt=.llms/prompts/enhance_readme.md --input=ctx.md --output=.llms/maverick_readme.md

llm_qwen3_coder_non_idiomatic: llm_ctx
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/non_idiomatic.md --input=ctx.md --output=.llms/qwen3_non_idiomatic.md

llm_qwen3_coder_improve_comments: llm_ctx
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/improve_comments.md --input=ctx.md --output=.llms/qwen3_improve_comments.md

llm_qwen3_code_review: llm_ctx
    python3 .llms/inference/hf.py --model "Qwen/Qwen3-Coder-480B-A35B-Instruct:novita" --prompt=.llms/prompts/code_review.md --input=ctx.md --output=.llms/qwen3_code_review.md

llm_glm45air_code_review: llm_ctx
    python3 .llms/inference/hf.py --model "zai-org/GLM-4.5-Air-FP8:together" --max_tokens 96000 --prompt=.llms/prompts/code_review.md --input=ctx.md --output=.llms/glm45air_code_review.md
