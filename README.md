# seichi-timed-stats-server

[seichi-game-data-server](https://github.com/GiganticMinecraft/seichi-game-data-server) が提供する情報を
時系列上のデータとして読み出せるようにすることを目指していたサービス群のリポジトリです (**開発終了**)。

> [!IMPORTANT]
> このリポジトリの実装は開発途中で終了し、時系列データの取り込みは
> [seichi-timed-stats-conifers](https://github.com/GiganticMinecraft/seichi-timed-stats-conifers)
> に置き換えられました。現行環境 ([seichi_infra](https://github.com/GiganticMinecraft/seichi_infra) の
> GitOps 定義) からこのリポジトリを参照しているものはありません。
>
> translator / Telegraf スタックは 2026-08 に削除され (#170)、未実装のまま残っていた
> reader スタブと CI も続けて削除されました。実装は Git 履歴から参照できます。

当時の gRPC プロトコル定義は
[seichi-timed-stats-protocol](https://github.com/GiganticMinecraft/seichi-timed-stats-protocol)
にて管理されています。

## 当時計画されていたアーキテクチャ

![architecture](./docs/images/architecture.drawio.svg)
