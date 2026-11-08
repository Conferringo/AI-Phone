# AI Phone

[English](../../README.md) · [言語一覧](../../README.md) · Apache-2.0

## ダウンロードと現状

一般公開のダウンロードは準備中です。 [GitHub Releases](https://github.com/Conferringo/AI-Phone/releases)

Apple Silicon 搭載の macOS 14.4 以降、または Windows 11 x64 に対応します。各インストーラーに Android、Linux、エミュレーター、ローカルサービスが含まれます。

## 機能と利用例

実際のAndroidゲスト、軽量Linuxデスクトップ、Agentを一つの作業画面で扱います。標準は電話1台とコンピューター1台。会話は機器ごとに保存し、共通Agentまたは別モデルを割り当てられます。

- WeChatなどの実画面と通知を確認し、利用者の指示に沿って操作・返信。アカウントやアプリ側の制限は引き続き有効です。
- 電話で受信したファイルをLinuxへ送り、表を処理・確認して返却。機器間転送と連絡先への送付は別々に検証します。
- 指示を追加して生成を中断しても、既存のメッセージと操作履歴は保持。通知からAgentを起動でき、自主探索は機器ごとに一時停止できます。
- Skill、関連付けた知識文書、作業ファイルは共有。モデル/API、記憶、外部接続はAgentに属します。Clashはゲスト内アプリとして動作し、ホストは標準で直接接続します。

副作用のある操作には観察、機器、タスク、制御epoch、一意の操作IDを結び付け、結果不明時は再実行前に観察します。

## 技術とデータ

`apps/web`はNext.js App Router、React、Tailwind、shadcn、`apps/client`はTauri 2、`crates`はRust/Axum/Tokio、`packages/protocol`はprotobufです。PostgreSQL/SQLxが状態を保存し、Redisは任意キャッシュ用の予約構成であり、タスクキューではありません。開発はBun/Cargo、サービスはCompose。`infra`、`scripts`、`skills`、`config`に実行環境と組み込み能力を配置します。

ソースと利用者データは分離します。機器ディスク、アカウント、会話、添付、鍵は公開しません。更新は版管理された配布物、バックアップ、移行、ロールバックを使い、利用者データ上での直接の`git pull`を更新方式にしません。

## 今後・責任・ライセンス

完全パッケージ、導入再開、署名付き更新、長時間Agent動作、音声/入力、モデル検証、アプリ特化Skillを優先します。音声対話と実験的電話機能は将来計画です。

画面と初期設定は10言語に対応。Skill左の言語ボタンで切り替えます。ゲストOS言語やツール原文は自動翻訳しません。アラビア語は右から左へ表示します。

権限、アカウント、外部操作、各アプリの規約は利用者が管理してください。詐欺、なりすまし、同意のない操作に使用しないでください。独自コードはApache-2.0、第三者部品は各ライセンスに従います。

[公開状況](release-status.md) · [構成](architecture.en.md) · [互換性](compatibility.md) · [第三者声明](../../THIRD_PARTY_NOTICES.md) · [免責事項](../../DISCLAIMER.md)
