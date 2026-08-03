#!/bin/sh
# buf generate (build.rs) が使う protoc プラグインを固定バージョンでインストールする。
# CI・Dockerfile・開発環境で同じ生成結果になるよう、バージョン変更は必ずこのスクリプトだけで行うこと。
# (バージョンは Cargo.toml の prost / tonic の世代と互換である必要がある)
set -eu

exec cargo install --locked \
  protoc-gen-prost@0.2.3 \
  protoc-gen-tonic@0.3.0 \
  protoc-gen-prost-crate@0.3.1
