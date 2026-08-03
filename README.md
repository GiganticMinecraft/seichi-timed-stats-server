# seichi-timed-stats-server

[seichi-game-data-server](https://github.com/GiganticMinecraft/seichi-game-data-server) が提供する情報を
時系列上のデータとして読み出せるようにする一連のサービス。

サービスのAPIはgRPCにより提供されており、プロトコル定義は
[seichi-timed-stats-protocol](https://github.com/GiganticMinecraft/seichi-timed-stats-protocol)
にて管理されています。

> [!NOTE]
> かつてこのリポジトリに存在した translator / Telegraf による時系列データ取り込みスタックは、
> [seichi-timed-stats-conifers](https://github.com/GiganticMinecraft/seichi-timed-stats-conifers)
> に置き換えられ、現行環境 ([seichi_infra](https://github.com/GiganticMinecraft/seichi_infra) の
> GitOps 定義) では使われていないため 2026-08 に削除されました。
> 実装は Git 履歴から参照できます。

## アーキテクチャ俯瞰図

![architecture](./docs/images/architecture.drawio.svg)

(俯瞰図には削除済みの translator / Telegraf を含む、当時計画されていた構成が描かれています)

## 開発環境のセットアップ

reader のビルド (`cargo build` 等) には、Rust ツールチェイン (バージョンは
`servers/reader/rust-toolchain.toml` を参照) に加えて次のツールが必要です。

- [buf CLI](https://buf.build/docs/installation) — build.rs がコード生成に使います
  (バージョンは Dockerfile の `bufbuild/buf` イメージに揃えることを推奨)
- protoc — 依存クレート (pbjson-types など) のビルドに必要です (例: `brew install protobuf`)
- buf が使う protoc プラグイン — **PATH 上のバイナリがそのまま使われるため、
  バージョン違いのプラグインが入っていると互換性のない生成コードでビルドが壊れます。**
  必ず次のスクリプトでインストールしてください:

  ```sh
  ./servers/reader/install-buf-plugins.sh
  ```
