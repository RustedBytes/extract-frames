#!/usr/bin/bash

rm -rf code
git clone https://github.com/egorsmkv/extract-frames-rs code
cd code

cargo doc --release --no-deps
rm -rf ../docs/
mv target/doc/ ../docs/
cd ..

rm -rf code
