# Slack API 利用一覧

本プロジェクト（SlackSitter）で利用している Slack API の一覧です。
API 呼び出しはすべて Rust 側 (`crates/core/src/slack/`) に集約されています。
フロントエンドは Slack API を直接呼びません。

## 構成

| ファイル | 役割 |
|----------|------|
| `crates/core/src/slack/client.rs` | `reqwest` による HTTP クライアント。Bearer トークンの付与、429 / `ok: false` の判定、リトライ |
| `crates/core/src/slack/endpoints.rs` | 各 API のラッパー (`SlackClient` のメソッド) |
| `crates/core/src/slack/dto.rs` | Slack の応答 JSON を受ける型 |

呼び出しの入口になる Tauri コマンドは `src-tauri/src/commands/` にあります。

## API 一覧

### 1. auth.test

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::auth_test()` |
| 呼び出し元 | `commands/auth.rs` — `authenticate`、`try_auto_login` |
| 用途 | トークンの検証、ワークスペース URL と認証ユーザー ID の取得 |

### 2. conversations.list

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::list_channels_page()` |
| 呼び出し元 | `commands/channels.rs` — `refresh_workspace_data` |
| 用途 | ワークスペース内のチャンネル一覧を取得 |
| 主なパラメータ | `types`: `public_channel,private_channel`、`exclude_archived`: true、`limit`: 200 |
| ページネーション | カーソルベース (`response_metadata.next_cursor`) |
| 備考 | 全チャンネルをカタログとして保持する (チャンネル選択、投稿先の解決に使用)。ボードに読み込むのは名前が `times` で始まるチャンネルと、カスタムボードが指定したチャンネルだけ |

### 3. conversations.history

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::conversation_history()` |
| 呼び出し元 | `commands/channels.rs` — チャンネル読み込み (`load_single_channel`) |
| 用途 | 指定チャンネルのメッセージ履歴を取得 |
| 主なパラメータ | `channel`、`limit`: 10、`oldest`: 差分取得用 (任意) |
| 備考 | 取得後、タイムスタンプ降順に並べる |

リアクションの追加 / 取り消しの前にも、対象メッセージ 1 件を取得して自分のリアクション済みかを確認します (`oldest` と `latest` に同じ `ts`、`inclusive`: true、`limit`: 1)。

### 4. conversations.replies

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::conversation_replies()` |
| 呼び出し元 | `commands/channels.rs` — チャンネル読み込み |
| 用途 | スレッドの返信を取得 (`reply_count` が 1 以上のメッセージだけ) |
| 主なパラメータ | `channel`、`ts`: 親メッセージの `ts`、`limit`: 20 |
| 備考 | 親メッセージは除外し、タイムスタンプ昇順に並べる。取得に失敗しても親メッセージの表示は続ける |

### 5. users.info

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::user_info()` |
| 呼び出し元 | `commands/auth.rs` (自分のアバター)、`commands/channels.rs` (投稿者のアバター、ユーザー情報ダイアログ) |
| 用途 | プロフィール (名前、アバター画像 URL) の取得 |
| 画像の優先順位 | 72 → 48 → 32 (自分のアバターは 192 → 72) |
| キャッシュ | チャンネル読み込み 1 バッチの中でユーザーごとに結果を保持。失敗した検索も保持して再試行を避ける |

### 6. emoji.list

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::emoji_list()` |
| 呼び出し元 | `commands/channels.rs` — `refresh_workspace_data` |
| 用途 | カスタム絵文字の一覧。`alias:` の連鎖は読み込み時に解決して保持する |

### 7. chat.postMessage

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::post_message()` |
| 呼び出し元 | `commands/messages.rs` — `post_message` |
| 用途 | メッセージの投稿 |
| 備考 | 投稿先はチャンネル名 (大文字小文字を区別しない) からカタログで ID に解決する |

### 8. reactions.add / reactions.remove

| 項目 | 内容 |
|------|------|
| 呼び出し | `SlackClient::add_reaction()` / `remove_reaction()` |
| 呼び出し元 | `commands/reactions.rs` — `toggle_reaction` |
| 用途 | リアクションの追加 / 取り消し |
| 備考 | 先に自分のリアクション済みかを確認してから、追加か取り消しかを決める。`already_reacted` と `no_reaction` は成功として扱う |

### 画像の取得 (Slack Web API 以外)

Slack 内のプライベート画像 (`files.slack.com` など) は Bearer トークンが必要です。
`commands/image.rs` の `fetch_image` が取得して data URL を返します。
リダイレクトは最大 5 回までで、URL に `slack` を含む場合だけトークンを付け直します。
同時に取得するのは 2 件までです。
アバター画像は公開 URL なので、フロントエンドが `<img>` で直接読み込みます。

## リトライ・レート制限対応

すべての API 呼び出しは `SlackClient::with_retry` を通ります。

| 項目 | 内容 |
|------|------|
| 最大リトライ回数 | 3 回 (最大 4 回の試行) |
| 待ち時間 | 429 で `Retry-After` があればその秒数、なければ 30 秒。それ以外は指数バックオフ (1 → 2 → 4 → 8 秒) |
| リトライ対象 | HTTP 429、エラー文字列に `ratelimited` を含むもの、タイムアウト / 接続エラー |
| 待ち時間のログ | `retrying slack api call` として警告ログに残る |

旧 WinUI 版は `conversations.list` をリトライ対象にしていませんでした。
チャンネルの多いワークスペースでは、ページ取得中の 429 で更新全体が失敗していたため、Tauri 版ではこれもリトライ対象にし、1 ページの件数を 200 にしました。

### 同時実行数

| 対象 | 上限 |
|------|------|
| チャンネルの読み込み | 10 |
| インライン画像の取得 | 2 |

## 必要な OAuth Scopes

| Scope | 用途 |
|-------|------|
| `channels:read` | パブリックチャンネル一覧の取得 |
| `channels:history` | パブリックチャンネルのメッセージ履歴取得 |
| `groups:read` | プライベートチャンネル一覧の取得 |
| `groups:history` | プライベートチャンネルのメッセージ履歴取得 |
| `mpim:read` | グループ DM の一覧取得 |
| `mpim:history` | グループ DM のメッセージ履歴取得 |
| `im:read` | DM の一覧取得 |
| `im:history` | DM のメッセージ履歴取得 |
| `users:read` | ユーザー情報の取得 |
| `chat:write` | メッセージの送信 |
| `emoji:read` | カスタム絵文字一覧の取得 |
| `reactions:write` | リアクションの追加 / 取り消し |
