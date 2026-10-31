# SlackSitter

`SlackSitter` は Slack のチャンネルを横並びで閲覧するためのデスクトップアプリです。
Tauri 2 (Rust + React / TypeScript) で作られています。
以前は WinUI 3 / .NET 8 のアプリでしたが、Tauri へ移行しました。

## 主な機能

- Slack User OAuth Token による認証 (トークンは OS の資格情報ストアに保存)
- `#times*` チャンネルの横並び表示 (メッセージ本文、スレッド返信、画像、リアクション)
- 参加中 / 未参加チャンネルのフィルター切り替え
- カスタムボード (任意のチャンネルを選んだボードを最大 10 件)
- メッセージの投稿、リアクションの追加 / 取り消し
- ユーザー情報の表示、トークンの変更、ログアウト
- 5 分ごとの自動更新 (停止 / 再開が可能)
- ログの表示とコピー

## 技術スタック

- Tauri 2 (Rust)
  - `reqwest` (Slack Web API)、`tokio`、`serde`
  - `keyring` (トークン保存)、`tracing` (ログ)
- React 19 + TypeScript + Vite
  - `zustand` (状態管理)
  - `@tanstack/react-virtual` (チャンネル列の横方向仮想化)
  - `@floating-ui/react` (吹き出しポップアップ)

Slack との通信はすべて Rust 側で行います。
フロントエンドは Tauri のコマンドとイベントだけを使い、Slack API を直接呼びません。

## プロジェクト構成

- `Cargo.toml` — Cargo ワークスペース (`crates/core` と `src-tauri`)
- `crates/core/` — `slacksitter-core` クレート (Tauri に依存しない)
  - `src/slack/` — Slack API クライアント、リトライ、メッセージ解析、表示用モデル
  - `src/error.rs` — 共通のエラー型 `AppError`
- `src-tauri/` — Tauri シェル (Rust)
  - `src/commands/` — フロントエンドから呼ばれる Tauri コマンド
  - `src/persistence/` — トークン (資格情報ストア)、カスタムボード (JSON)
  - `src/logging.rs` — ファイルログ
- `src/` — React (フロントエンド)
  - `components/` — 画面部品
  - `store/` — zustand ストア
  - `actions/` — 複数の部品から使う操作 (更新、ログアウト、ボード操作)
  - `api/` — Tauri コマンド / イベントの型付きラッパー
  - `types/models.ts` — Rust の表示用モデルに対応する型

詳しくは次の文書を参照してください。

- [MODELS.md](MODELS.md) — 表示用モデル
- [VIEW_FLOW.md](VIEW_FLOW.md) — データ取得から表示までの流れ
- [SLACK_API.md](SLACK_API.md) — 利用している Slack API
- [CONTROLER.md](CONTROLER.md) — 下部コントローラーの仕様
- [HANDOFF.md](HANDOFF.md) — 引き継ぎメモ

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

### 3. 開発時の起動

```powershell
npm run tauri dev
```

### 4. テスト

```powershell
cargo test
```

リポジトリ直下で実行すると、ワークスペース全体 (`crates/core` と `src-tauri`) のテストが走ります。

フロントエンドの型チェックは次のコマンドです。

```powershell
npx tsc --noEmit
```

### 5. 配布用ビルド

```powershell
npm run tauri build
```

## Slack 側で必要な権限

User Token Scopes に少なくとも以下が必要です。

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
| アクセストークン | OS の資格情報ストア (サービス名 `SlackSitter`) |
| カスタムボード、選択中のフィルター | `%APPDATA%\com.yanqirenshi.slacksitter\custom-board.json` |
| ログ (日ごとに 1 ファイル、7 日分) | `%LOCALAPPDATA%\com.yanqirenshi.slacksitter\logs\` |

旧 WinUI 版はトークンを `.env` に平文で保存していました。
Tauri 版は `.env` を読み込みません。
初回は認証画面でトークンを入力し直してください。
