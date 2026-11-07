# SlackSitter

`SlackSitter` は Slack のチャンネルを横並びで閲覧するためのデスクトップアプリです。
Slack のデータを SQLite に貯める常駐サーバー (`slacksitter-server`) と、それを表示するデスクトップアプリ (Tauri 2 + React / TypeScript) で構成されています。
以前は WinUI 3 / .NET 8 のアプリでしたが、Tauri へ移行しました。

```
┌──────────────────────────┐          ┌────────────────────────────────────┐
│ Tauri (UI シェル)          │  HTTP    │ slacksitter-server (axum)           │
│  React                    │◀────────▶│  ├ REST API  (Bearer API キー)       │
│  接続設定 / カスタムボード   │  SSE     │  ├ 同期ワーカー (Slack を定期巡回)    │
└──────────────────────────┘          │  └ SQLite (sqlx)                     │
                                      └─────────────────┬──────────────────┘
                                                        │ Slack Web API
                                                        ▼
```

Slack のトークンはサーバーだけが持ちます。
デスクトップアプリはサーバーの URL と API キーだけを保存します。

## 主な機能

- サーバーが Slack を定期的に巡回して SQLite に蓄積 (Slack の呼び出し回数制限に合わせて間隔を空ける)
- サーバーへの接続 (サーバー URL と API キー。API キーは OS の資格情報ストアに保存)
- `#times*` チャンネルの横並び表示 (メッセージ本文、スレッド返信、画像、リアクション)
- 参加中 / 未参加チャンネルのフィルター切り替え
- カスタムボード (任意のチャンネルを選んだボードを最大 10 件)
- メッセージの投稿、リアクションの追加 / 取り消し
- ユーザー情報の表示、サーバーへの今すぐ同期の依頼、接続の解除
- サーバーの更新通知 (SSE) による自動更新。接続が切れたときは 30 秒ごとのポーリングで補う
- ログの表示とコピー (アプリのログとサーバーのログ)

## 技術スタック

- サーバー (Rust、`server/api`)
  - `axum` (Web API、SSE)、`tokio`、`tower-http` (CORS)、`figment` (設定)
  - `sqlx` + SQLite (蓄積)、`reqwest` (Slack Web API)、`tracing` (ログ)
- Tauri 2 (Rust、`apps/desktop/src-tauri`)
  - `keyring` (API キーの保存)、`tracing` (ログ)、`serde`
- React 19 + TypeScript + Vite (`apps/web`)
  - `zustand` (状態管理)
  - `@tanstack/react-virtual` (チャンネル列の横方向仮想化)
  - `@floating-ui/react` (吹き出しポップアップ)

Slack との通信と DB はすべてサーバーが行います。
フロントエンドはサーバーの Web API (`fetch`) と SSE だけを使い、Slack API を直接呼びません。
Tauri の Rust 側は、接続設定、カスタムボード、UI のログだけを扱います。

## プロジェクト構成

Web フロント、デスクトップ シェル、サーバーが最上位で分かれたモノレポです。

```
sitter.slack/
├── apps/
│   ├── web/                     React フロント (Vite)
│   └── desktop/                 Tauri シェル
├── server/
│   ├── core/                    slacksitter-core (Slack クライアント、DB、同期)
│   ├── api/                     slacksitter-server (axum の Web API)
│   └── slacksitter.example.toml サーバー設定の例
├── docs/                        文書
├── Cargo.toml                   Rust ワークスペース (server/core、server/api、apps/desktop/src-tauri)
├── package.json                 npm workspaces (apps/web、apps/desktop) と共通スクリプト
└── README.md
```

- `apps/web/` — React (フロントエンド。パッケージ名 `@slacksitter/web`)
  - `src/components/` — 画面部品
  - `src/store/` — zustand ストア
  - `src/actions/` — 複数の部品から使う操作 (読み込み、更新ボタン、SSE の処理、接続の解除、ボード操作)
  - `src/api/` — サーバーの Web API / SSE のクライアント (`client.ts`、`events.ts`) と Tauri コマンドのラッパー (`commands.ts`)
  - `src/types/models.ts` — Rust の表示用モデルに対応する型
- `apps/desktop/` — Tauri シェル (パッケージ名 `@slacksitter/desktop`。`@tauri-apps/cli` だけを持つ)
  - `src-tauri/` — Rust のクレート `slacksitter`
    - `src/commands/` — フロントエンドから呼ばれる Tauri コマンド (接続設定、カスタムボード、ログ)
    - `src/persistence/` — サーバー接続設定 (URL は JSON、API キーは資格情報ストア)、カスタムボード (JSON)
    - `src/logging.rs` — ファイルログ
- `server/core/` — `slacksitter-core` クレート (Tauri に依存しない)
  - `src/slack/` — Slack API クライアント、リトライ、メッセージ解析、表示用モデル
  - `src/db/` — SQLite への書き込み (`store`)、表示用モデルの組み立て (`view`)。スキーマは `migrations/`
  - `src/sync/` — Slack から SQLite への差分同期 (`SyncService`) と定期巡回 (`SyncScheduler`)
  - `src/error.rs` — 共通のエラー型 `AppError`
- `server/api/` — `slacksitter-server` クレート (axum の Web API サーバー。常駐して Slack を巡回し、SQLite に蓄積する)
  - `src/config.rs` — 設定 (TOML と環境変数)、`src/auth.rs` — API キー認証、`src/routes.rs` — REST と SSE、`src/images.rs` — 画像プロキシ
  - `tests/api.rs` — API の統合テスト

詳しくは次の文書を参照してください。

- [docs/SERVER.md](docs/SERVER.md) — サーバーの設定、API 一覧、起動方法
- [docs/MODELS.md](docs/MODELS.md) — 表示用モデル
- [docs/VIEW_FLOW.md](docs/VIEW_FLOW.md) — データ取得から表示までの流れ
- [docs/SLACK_API.md](docs/SLACK_API.md) — 利用している Slack API
- [docs/CONTROLER.md](docs/CONTROLER.md) — 下部コントローラーの仕様
- [docs/HANDOFF.md](docs/HANDOFF.md) — 引き継ぎメモ

## セットアップ

### 1. 必要環境

- Windows
- Node.js 20.19 以上 (Volta で 24.15.0 に固定しています)
- Rust (stable) と、Tauri 2 の前提ツール (Microsoft C++ Build Tools、WebView2)

### 2. リポジトリ取得

```powershell
git clone https://github.com/yanqirenshi/slack.sitter.git
cd slack.sitter
npm install
```

`npm install` はリポジトリ直下で 1 回実行します (npm workspaces が `apps/web` と `apps/desktop` の依存をまとめて入れます)。

### 3. サーバーの起動 (先に起動します)

デスクトップアプリはサーバーに接続して動くので、サーバーを先に起動します。
設定ファイルを作り、Slack のトークンと API キーを書きます。

```powershell
Copy-Item server\slacksitter.example.toml slacksitter.toml
# slacksitter.toml の slack.token と server.api_key を書き換える
cargo run -p slacksitter-server -- --config slacksitter.toml
```

設定項目、API 一覧、環境変数での指定は [docs/SERVER.md](docs/SERVER.md) を参照してください。
サーバーは、リポジトリ直下から起動します (`slacksitter.toml`、`data/`、`logs/` は、カレント ディレクトリを基準にします)。
起動後、数分たつと `times*` チャンネルのメッセージが DB に貯まります。

### 4. デスクトップアプリの起動

```powershell
npm run dev:desktop
```

初回は「サーバー接続」画面が出ます。
サーバー URL (既定 `http://127.0.0.1:8787`) と、`server.api_key` の値を入力してください。

### 5. テスト

```powershell
cargo test
```

リポジトリ直下で実行すると、ワークスペース全体 (`server/core`、`server/api`、`apps/desktop/src-tauri`) のテストが走ります。

フロントエンドの型チェックは次のコマンドです。

```powershell
npm run typecheck
```

### 6. 配布用ビルド

```powershell
npm run build:web
npm run tauri -- build
```

`npm run build:web` は `apps/web/dist` を作ります。
`npm run tauri -- build` は、その `dist` を取り込んだデスクトップ アプリを作ります (Tauri が `beforeBuildCommand` で Web 側のビルドも走らせます)。

## Slack 側で必要な権限

サーバーの `slack.token` (User OAuth Token) の User Token Scopes に、少なくとも以下が必要です。

- `channels:read`
- `channels:history`
- `groups:read`
- `groups:history`
- `mpim:read`
- `mpim:history`
- `im:read`
- `im:history`
- `users:read`
- `chat:write`
- `emoji:read`

リアクションの追加 / 取り消しには `reactions:write` も必要です。

## 保存されるデータ

| 内容 | 場所 |
|------|------|
| サーバーの URL | `%APPDATA%\com.yanqirenshi.slacksitter\server-settings.json` |
| サーバーの API キー | OS の資格情報ストア (サービス名 `SlackSitter`、ユーザー名 `server_api_key`) |
| カスタムボード、選択中のフィルター | `%APPDATA%\com.yanqirenshi.slacksitter\custom-board.json` |
| アプリのログ (日ごとに 1 ファイル、7 日分) | `%LOCALAPPDATA%\com.yanqirenshi.slacksitter\logs\` |
| Slack のトークンと API キー (サーバー側) | サーバーの `slacksitter.toml` または環境変数 (`.gitignore` 済み) |
| Slack のデータ (サーバー側) | SQLite (既定 `./data/slacksitter.db`) |
| サーバーのログ | 既定 `./logs/` (日ごとに 1 ファイル、14 日分) |

以前のバージョンは Slack のトークンを OS の資格情報ストア (`slack_access_token`) に保存していました。
今のバージョンはそれを読み込みません。
トークンはサーバーの設定に書いてください。
旧 WinUI 版の `.env` も読み込みません。
