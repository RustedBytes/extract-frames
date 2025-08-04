#!/usr/bin/bash

rm -rf extract-frames-rs-code
git clone https://github.com/egorsmkv/extract-frames-rs extract-frames-rs-code
cd extract-frames-rs-code

cargo doc --release --no-deps
rm -rf ../docs/
mv target/doc/ ../docs/
cd ..

rm -rf extract-frames-rs-code
