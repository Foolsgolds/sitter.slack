# HANDOFF

## 現在の状態

- ブランチ: `master` (サーバー化は `feature/phase1-core-crate` → `phase2-sqlite-sync` → `phase3-server` → `phase4-frontend` の順に積んである)
- ターゲット: slacksitter-server (axum + SQLite) と、それに接続する Tauri 2 (Rust + React / TypeScript) のデスクトップアプリ
- WinUI 3 / .NET 8 版からの移行は完了し、旧実装 (`SlackSitter/`、`SlackSitter.slnx`) は削除済み。旧実装が必要なときは Git の履歴から取り出す (移行前の最後のコミットは `37e52cb`)
- 機能の範囲: サーバー接続、ボード表示、メッセージ / 画像 / リアクション、投稿、ユーザー情報、カスタムボード、サーバーの更新通知による自動更新、ログ表示

## 開発の始め方

サーバーを先に起動します (設定は [SERVER.md](SERVER.md))。

```powershell
Copy-Item server\slacksitter.example.toml slacksitter.toml
# slacksitter.toml の slack.token と server.api_key を書き換える
cargo run -p slacksitter-server -- --config slacksitter.toml
```

別のターミナルで、デスクトップアプリを起動します。

```powershell
npm install
npm run dev:desktop
```

初回は「サーバー接続」画面で、サーバー URL と `server.api_key` を入力します。

- 初回のビルドには数分かかる。以降は変更が自動で反映される (Rust は再ビルド、画面は HMR)
- Node.js は 20.19 以上が必要 (`package.json` の Volta 設定で 24.15.0 に固定)
- 起動時にポート 1420 (Vite) が使用中だと `tauri dev` は失敗する。前回の Vite が残っていないか確認する
- Rust のテスト: `cargo test` (リポジトリ直下。ワークスペース全体)
- 型チェック: `npm run typecheck`

構成と各文書へのリンクは [README.md](../README.md) を参照してください。

## 設計上の判断

- Slack との通信と DB (SQLite) は、すべてサーバー (`server/api`、同期ロジックは `server/core`) が受け持つ。デスクトップアプリは Slack に触らない
- Slack トークンはサーバーの設定ファイル (TOML) か環境変数に置く。デスクトップアプリには持たせず、キーリングにも入れない。アプリが保存するのは、サーバーの URL (JSON) と API キー (OS の資格情報ストア。サービス名 `SlackSitter`、ユーザー名 `server_api_key`) だけ
- 旧版が資格情報ストアに保存していた Slack トークン (`slack_access_token`) と、旧 WinUI 版の `.env` (平文) は読まない
- サーバーは当面ローカルで使うが、最初から API キー認証 (`Authorization: Bearer`) を入れている。TLS はサーバー自身では行わず、外部へ公開するときはリバースプロキシに任せる
- 画像は DB に保存しない。表示時にサーバーが Slack から取得して返す (`GET /api/images`)。Slack のホストにだけトークンを付け、内部アドレスへの取得は拒否する
- フロントエンドの SSE は `EventSource` ではなく `fetch` のストリームで読む (ヘッダーを付けるため。API キーを URL に載せない)。切れたときは指数バックオフで再接続し、つながるまでは 30 秒ごとのポーリングで補う
- **更新ボタンの意味が変わった**: 以前は「表示中のボードのチャンネルを Slack から全件取り直す」だった。今は「サーバーに、表示中のボードのチャンネルを今すぐ同期するよう頼む」ボタンで、新しい内容は SSE の `channel-updated` で届く。同期は Slack の呼び出し回数制限を受けるので、反映まで数十秒から数分かかることがある。5 分ごとの自動更新タイマーはなくなった (同期はサーバーが行う)
- 「`times` で始まる」という条件はサーバーの設定 (`sync.channel_prefix`) が持ち、アプリは `GET /api/me` の `channelPrefix` を使う。アプリ側に同じ定数はない
- カスタムボードの定義はアプリ (Tauri 側の `custom-board.json`) に持ち、必要なチャンネルの ID だけを `PUT /api/sync/targets` でサーバーに伝える。カスタムボードのチャンネルは名前ではなく ID で記録する。旧版の `custom-board.json` は読み込めない
- サーバーは、各チャンネルの最新 `history_limit` 件 (既定 15) を毎回読み直す。内容が変わったメッセージだけを書き直し、`channel-updated` もそのときだけ流す。新着、最近のメッセージのリアクション、新しい返信が 1 回の呼び出しで DB に入る。保存済みの最新 `ts` に届くまで遡る (上限ページ数あり)
- 全体の手動同期 (`POST /api/sync`) は、巡回と同じ `request_spacing_seconds` を空けて呼ぶ。指定したチャンネルだけの同期 (`POST /api/sync/channels/{id}`、`POST /api/sync/channels`) は間隔を空けない。API で依頼した同期は、サーバー内で 1 バッチずつ順に実行する
- 同期の `sync-started` / `sync-progress` / `sync-completed` には `batchId` と `kind` (`scheduler` / `manual`) が付く。API の 202 応答にも同じ `batchId` が入る。更新ボタンは、自分が頼んだバッチの `sync-completed` を待って、更新中の表示を戻す
- `PUT /api/sync/targets` は、カタログにない ID を無視して `unknown` に入れて返す (古いチャンネルが 1 つ残っていても更新全体は失敗させず、同期が永久に失敗する対象も作らない)。`POST /api/sync/channels` は、未知の ID を含むと 400 で拒否する
- `server.api_key` が設定例の `change-me` のままだと起動しない
- `conversations.list` もリトライ対象にし、1 ページを 200 件にした (旧版との違い。理由は [SLACK_API.md](SLACK_API.md))
- 本文の解析は旧版と同じ「トグル方式」をそのまま移した。`*` が閉じていないと、その後ろ全体が太字になる。解析は DB の `raw_json` から読み出し時に行うので、変えても再取得は要らない
- `slack.api_base_url` はテスト用の設定で、偽の Slack に向けるために使う (通常は設定しない)

## 既知の注意点

- Rust の表示用モデル (`server/core/src/slack/models.rs`) と `apps/web/src/types/models.ts` は手で同期している。片方を変えたらもう片方も変える
- スレッドの返信 (`conversations.replies`) が 1 つでも取れないと、そのチャンネルの同期全体が失敗する (同じ回に取れた新着も書かない。一貫性を優先している)。取れない状態が続くと、そのチャンネルは更新されなくなる。原因は `sync_state.last_error` に残る
- `thread_broadcast` の返信 (スレッドからチャンネルにも投稿したもの) は、チャンネルの一覧にも親メッセージの下にも出る
- サーバーが同期するのは、各チャンネルの最新 `history_limit` 件と、そこに追加された新着だけ。それより古いメッセージのリアクション、編集、削除は DB に反映されない。削除されたメッセージは DB に残る
- 起動時にサーバーが止まっていると (常駐サーバーはアプリと同時に起動されることが多い)、スプラッシュに「サーバーに接続しています...」を出し、1 秒、2 秒、4 秒と待ち時間を倍にして (上限 30 秒) 再試行する。「接続設定を変える」ボタンで接続画面へ移れる。API キーが拒否された (HTTP 401) ときだけ、接続画面へ戻す。起動後にサーバーが止まったときは、状態パネルにエラーを出し、戻れば自動で復帰する
- まだ一度も同期されていないチャンネルは、メッセージのない列として出る (列に「同期待ちの可能性があります」と表示する)
- サーバーを複数のクライアントで共有すると、カスタムボードのチャンネルの一覧は最後に `PUT /api/sync/targets` したクライアントのものが残る
- `tauri.conf.json` の CSP は、サーバーの URL を自由に指定できるよう `connect-src` に `http:` `https:` を許可し、画像の object URL のため `img-src` に `blob:` を足している。接続先を固定できるなら絞る
- 画面の自動テストはない。画面の確認は実際のウインドウか、Tauri API をモックしたブラウザで行う (`window.__TAURI_INTERNALS__.invoke` を差し替える)
- サーバーの `slack.token` の権限不足 (`missing_scope`) は、サーバーの `sync-error` で届く。状態パネルが必要な権限の一覧を出す。`reactions:write` は一覧に含まれていない

## 次に着手しやすい候補

- フロントエンドの自動テスト (`selectVisibleChannels`、SSE パーサー、`normalize` に相当する処理など)
- リアクションの新規追加 (今は既存のリアクションのクリックによる追加 / 取り消しだけ)
- 本文の解析の改善 (閉じていない `*` の扱いなど)
- `apps/web/src/types/models.ts` を Rust から自動生成する
- 古いメッセージの変更 (リアクションの追加、削除) を DB に反映する仕組み
- `apps/web/index.html` が favicon として `/vite.svg` を参照しているが、`public/` が git に入っていないので 404 になる (以前からの状態)。ファイルを足すか、参照を外す

## 補足

`.github/copilot-instructions.md` にあるルール:

- 読みづらくなる UI サイズ変更は戻す
- 下部アイコンコントローラの部品名は `BottomControllerView` ではなく `MainControllerView`
