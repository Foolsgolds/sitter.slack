# SlackSitter サーバー (slacksitter-server)

Slack のデータを SQLite に蓄積し、Web API として提供する常駐プロセスです。
`crates/server` にあり、Tauri のアプリとは別プロセスで動きます。
起動すると、Slack を定期的に巡回して差分を DB に取り込みます。

```
Slack Web API ◀── 同期ワーカー ──▶ SQLite ◀── REST API / SSE ◀── クライアント (Bearer API キー)
```

- Slack のトークンはサーバーだけが持ちます。クライアントには渡しません。
- TLS はサーバー自身では行いません。LAN や外部へ公開するときはリバースプロキシ (nginx、Caddy など) に任せてください。
- 最初から API キー認証を入れています。`/api/health` 以外は `Authorization: Bearer <api_key>` が必要です。

## 起動方法

Windows のサービス化や自動起動は範囲外です。
次のように手動で起動します。

```powershell
# 1. 設定ファイルを作る (slacksitter.toml は .gitignore 済み)
Copy-Item slacksitter.example.toml slacksitter.toml
# slacksitter.toml の api_key と slack.token を書き換える

# 2. 起動する
cargo run -p slacksitter-server -- --config slacksitter.toml
```

リリースビルドを使うときは次のとおりです。

```powershell
cargo build -p slacksitter-server --release
.\target\release\slacksitter-server.exe --config slacksitter.toml
```

設定ファイルを使わず、環境変数だけで渡すこともできます。

```powershell
$env:SLACKSITTER__SLACK__TOKEN = "xoxp-..."
$env:SLACKSITTER__SERVER__API_KEY = "長いランダムな文字列"
cargo run -p slacksitter-server
```

起動の順序は次のとおりです。

1. 設定を読み込む (`api_key` か `slack.token` が空なら、ここで起動を拒否する)
2. ログを開始する
3. DB を開き、マイグレーションを適用する
4. `auth.test` でトークンを検証する (失敗したら起動しない)
5. チャンネル一覧と絵文字を取得する (`sync_catalog`)。失敗しても、DB にあるデータで起動を続ける
6. HTTP の待ち受けと、同期スケジューラーを開始する

Ctrl+C で終了します。
接続中の SSE などは、5 秒待って打ち切ります。

終了コードは、正常終了が 0、設定の誤りが 2、起動時の失敗 (DB、Slack の認証、ポートの確保) が 1 です。

### コマンドライン引数

| 引数 | 内容 |
|------|------|
| `--config <path>` | 設定ファイル (TOML) のパス |
| `-h`, `--help` | 使い方を表示 |
| `-V`, `--version` | バージョンを表示 |

設定ファイルの探し方は、`--config`、環境変数 `SLACKSITTER_CONFIG`、カレントの `slacksitter.toml` の順です。
`--config` か `SLACKSITTER_CONFIG` で指定したファイルが無いときはエラーになります。
カレントの `slacksitter.toml` が無いだけなら、環境変数だけで起動できます。

## 設定

[slacksitter.example.toml](slacksitter.example.toml) が設定例です。

| 項目 | 既定値 | 内容 |
|------|--------|------|
| `server.bind` | `127.0.0.1:8787` | 待ち受けるアドレス。LAN に公開するなら `0.0.0.0:8787` にし、前段にリバースプロキシを置く |
| `server.api_key` | なし (必須) | API キー。空だと起動しない |
| `server.cors_origins` | `tauri://localhost`、`http://tauri.localhost`、`http://localhost:1420` | ブラウザ (WebView) からの呼び出しを許可するオリジン。ここに無いオリジンには CORS ヘッダーを返さない |
| `slack.token` | なし (必須) | Slack の User OAuth Token (`xoxp-...`) |
| `slack.api_base_url` | `https://slack.com/api` | Slack API の接続先。ローカルの偽 Slack に向けるテスト用で、通常は設定しない |
| `database.path` | `./data/slacksitter.db` | SQLite のファイル。親ディレクトリは自動で作る |
| `sync.channel_prefix` | `times` | 名前がこの文字列で始まるチャンネルを自動で同期対象にする (大文字小文字を区別する) |
| `sync.history_limit` | `15` | `conversations.history` 1 回で取る件数 (1〜200) |
| `sync.request_spacing_seconds` | `60` | 巡回中の `conversations.history` / `conversations.replies` の呼び出し間隔 |
| `sync.cycle_interval_seconds` | `300` | 全チャンネルを 1 周したあとに置く間隔 |
| `sync.catalog_interval_seconds` | `3600` | チャンネル一覧と絵文字を取り直す間隔 |
| `sync.max_pages_per_channel` | `10` | 1 チャンネルの同期で遡る最大ページ数 |
| `log.dir` | `./logs` | ログの出力先。日ごとに `slacksitter-server.<日付>.log` を作り、14 日分を残す |
| `log.level` | `info` | ログのレベル (`error`、`warn`、`info`、`debug`、`trace`。`tracing` のフィルター書式も可) |

### 環境変数による上書き

設定ファイルのどの項目も、`SLACKSITTER__<セクション>__<項目>` の環境変数で上書きできます (区切りはアンダースコア 2 つ)。
環境変数は設定ファイルより優先されます。

| 環境変数 | 上書きする項目 |
|----------|----------------|
| `SLACKSITTER__SLACK__TOKEN` | `slack.token` |
| `SLACKSITTER__SERVER__API_KEY` | `server.api_key` |
| `SLACKSITTER__SERVER__BIND` | `server.bind` |
| `SLACKSITTER__DATABASE__PATH` | `database.path` |
| `SLACKSITTER__SYNC__CHANNEL_PREFIX` | `sync.channel_prefix` |
| `SLACKSITTER__LOG__LEVEL` | `log.level` |

`SLACKSITTER_CONFIG` (アンダースコア 1 つ) は設定ファイルのパスです。

### 秘密情報の扱い

- Slack トークンと API キーは、ログにも標準出力にも出しません (設定の `Debug` 出力をマスクしています)。
- `slacksitter.toml`、`data/`、`logs/` は `.gitignore` に入っています。
- `slack.api_base_url` を変えると、その接続先にトークンを送ります。通常は設定しないでください。

## 認証と CORS

- `GET /api/health` だけが認証なしです。それ以外の `/api/*` は `Authorization: Bearer <api_key>` を要求し、一致しなければ 401 を返します。キーの比較は定数時間で行います。
- `EventSource` はヘッダーを付けられません。`/api/events` も同じヘッダーが必要なので、`fetch` のストリームで読んでください。
- CORS は `server.cors_origins` のオリジンだけを許可します。プリフライト (`OPTIONS`) は API キーなしで応答します。

## API 一覧

JSON のキーは camelCase です。
モデルは Tauri のコマンドが返していたものと同じ形で、`crates/core/src/slack/models.rs` と [MODELS.md](MODELS.md) に定義があります。
エラーは `{ "error": "メッセージ" }` と HTTP ステータスで返します。

| メソッド | パス | 内容 |
|----------|------|------|
| GET | `/api/health` | 認証なし。`{ "ok": true, "version": "0.1.0" }` |
| GET | `/api/me` | `{ workspaceUrl, userId, currentUserAvatarUri, channelPrefix }`。`auth.test` の結果は起動時に取得して保持している。`channelPrefix` はボードに出すチャンネルの接頭辞 (`sync.channel_prefix`) |
| GET | `/api/channels` | カタログ。`ChannelSummary[]` (アーカイブ済みを除く。名前順) |
| GET | `/api/channels/{id}?limit=10` | `ChannelWithMessages` (DB から)。`limit` は 1〜100、既定 10。未知のチャンネルは 404 |
| POST | `/api/channels/batch` | `{ "channelIds": [...] }` → `ChannelWithMessages[]` (DB から、指定順)。未知の ID は飛ばす。200 件まで。`?limit=` も使える |
| GET | `/api/users/{id}` | `SlackUserProfileInfo`。DB に無ければ `users.info` を呼んで保存する |
| GET | `/api/emoji` | `{ 名前: URL }` (alias 解決済み) |
| POST | `/api/messages` | `{ "channel": "times_foo", "text": "..." }`。チャンネル名 (大文字小文字を区別しない) から ID を解決して `chat.postMessage`。成功すると `{ "ok": true, "channelId": "..." }` を返し、そのチャンネルの同期をバックグラウンドで始める。チャンネルが無ければ 404、本文が空なら 400 |
| POST | `/api/reactions/toggle` | `{ "channelId", "ts", "name" }`。そのメッセージを読み直して自分のリアクション済みかを見て、追加か取り消しかを決める。成功後にメッセージを再取得して DB を更新し、`{ "reacted": true/false }` (操作後に自分がリアクションしているか) を返す |
| GET | `/api/images?url=<URL エンコード済み>` | 画像プロキシ。`Content-Type` を付けてバイト列を返す (下記) |
| GET | `/api/sync/status` | `{ running, lastStartedAt, lastFinishedAt, lastCatalogAt, channelPrefix, channels: [{ channelId, source, lastMessageTs, lastSyncedAt, lastError }] }`。時刻は Unix 秒。`channels` は同期対象と、対象から外れたが状態が残っているチャンネル (`source` は `null`) |
| POST | `/api/sync` | 全ターゲットの同期を今すぐ始める (間隔は空けない)。202 `{ "started": true }`。巡回が実行中なら 409 |
| POST | `/api/sync/channels/{id}` | 1 チャンネルを今すぐ同期する。202。未知のチャンネルは 404 |
| GET | `/api/sync/targets` | `{ "channelIds": [...] }` — `custom` 由来の同期対象 |
| PUT | `/api/sync/targets` | `{ "channelIds": [...] }` で `custom` 由来の同期対象を置き換える。まだ一度も同期していないチャンネルは、バックグラウンドで同期を始める |
| GET | `/api/events` | SSE (下記) |
| GET | `/api/logs?lines=500` | ログファイル (最新のもの) の末尾。`text/plain`。`lines` は 1〜5000 |

### 画像プロキシ (`/api/images`)

- `https://*.slack.com` へのリクエストにだけ Slack のトークンを付けます。リダイレクトのたびに宛先を確認します (最大 5 回)。
- 同時に取得するのは 2 件までです。
- `image/*` 以外の応答は 415、25 MB を超えるものは 413 にします。
- ディスクキャッシュはありません。応答には `Cache-Control: private, max-age=3600` を付けます。
- `http` / `https` 以外、`localhost`、ループバック、プライベート、リンクローカルのアドレスへの取得は 400 で拒否します (サーバーのネットワーク内部を探られないため)。ホスト名の DNS 解決後のアドレスまでは確認していません。

### SSE (`/api/events`)

`text/event-stream` で、次のイベントを流します。
`data` は JSON です。

| イベント | data | 意味 |
|----------|------|------|
| `ready` | `{}` | 接続できた。最初に 1 回だけ届く |
| `sync-started` | `{ "total": 件数 }` | 同期の巡回が始まった |
| `sync-progress` | `{ "done": 件数, "total": 件数 }` | 1 チャンネルの処理が終わるたびに |
| `channel-updated` | `{ "channelId": "C..." }` | そのチャンネルのメッセージが DB に取り込まれた (新規または更新) |
| `sync-completed` | `{ "failed": 件数 }` | 巡回が終わった。`failed` は失敗したチャンネル数 |
| `sync-error` | `{ "message": "..." }` | カタログまたはチャンネルの同期に失敗した。Slack のエラーコード (`missing_scope` など) を含む |

15 秒ごとにコメント行 (keep-alive) を送ります。
遅れた接続は一部のイベントを取りこぼすことがあるので、クライアントは切断時や `sync-completed` の後に読み直してください。

## 同期の仕組み

詳しい API の呼び出し頻度は [SLACK_API.md](SLACK_API.md) の「同期ワーカーからの呼び出し」を参照してください。

- 同期対象 (`sync_targets`) は、名前が `sync.channel_prefix` で始まるチャンネル (`prefix`) と、API で指定したチャンネル (`custom`) です。`prefix` はチャンネル一覧を取り直すたびに再計算します。
- スケジューラーは対象を順に巡回し、`conversations.history` と `conversations.replies` の呼び出しの間を `sync.request_spacing_seconds` 空けます。1 周したら `sync.cycle_interval_seconds` 待ちます。
- `POST /api/sync` と `POST /api/sync/channels/{id}` は間隔を空けずに呼びます。Slack が 429 を返したときは `Retry-After` に従って待ち、最大 3 回やり直します。
- メッセージは `messages.raw_json` に Slack の応答のまま保存します。本文の解析と画像の抽出は読み出し時に行うので、解析を直しても再取得は要りません。

## テスト

```powershell
cargo test -p slacksitter-server
```

- `tests/api.rs` は `tower::ServiceExt::oneshot` でルーターを直接呼びます。`/api/health` が認証なしで 200、ほかが API キーなしで 401、正しいキーで 200 になること、`/api/channels/{id}` が `ChannelWithMessages` の形であること、SSE の形、CORS を確認しています。Slack は呼ばず、DB は in-memory です。

### 偽の Slack を使った手動確認

実際の Slack トークンがなくても、`slack.api_base_url` を使って動作を確認できます。
`auth.test`、`users.info`、`conversations.list`、`conversations.history`、`conversations.replies`、`emoji.list`、`chat.postMessage` に答える簡単な HTTP サーバーを用意し、そこへ向けて起動します。

```toml
[slack]
token = "xoxp-fake"
api_base_url = "http://127.0.0.1:19999/api"
```

## 既知の制限

- 同期するのは各チャンネルの最新 `history_limit` 件 (既定 15) と、そこに新しく追加されたメッセージです。それより古いメッセージのリアクション、編集、削除は DB に反映されません。
- 削除されたメッセージは DB に残ります。
- 画像本体は保存しません (表示のたびに Slack から取得します)。
- 認証は API キー 1 つだけで、ユーザーの区別はありません。
