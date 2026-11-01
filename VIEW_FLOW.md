# SlackSitter ボード表示ロジック・フロー

Slack からデータを取得し、ボード (チャンネルの横並び) に表示するまでの流れです。
Slack との通信と DB はサーバー (`slacksitter-server`、[SERVER.md](SERVER.md)) が受け持ち、デスクトップアプリはサーバーが貯めたデータを読んで表示します。

## 1. 全体像

```
Slack Web API
   ▲  (reqwest)
slacksitter-server ── 同期ワーカー ──▶ SQLite ──▶ REST API / SSE
(crates/server + crates/core: slack/ db/ sync/)         ▲   │ HTTP (Bearer API キー)
                                                        │   ▼
Tauri シェル (src-tauri): 接続設定 (URL と API キー)、カスタムボード、UI のログ
React: api/client.ts ── actions/ ── store/ (zustand) ── components/
```

- Slack API の呼び出し、メッセージ本文の解析、画像 URL の抽出はすべてサーバー側 (`crates/core`) で行う。
- サーバーは画面にそのまま描ける「表示用モデル」(`ChannelWithMessages`) を JSON で返す。
- フロントエンドは `fetch` でサーバーを呼び、zustand のストアに入れて描画するだけで、Slack の生データは扱わない。
- Tauri の Rust 側 (`src-tauri`) は Slack に触らない。サーバーの URL と API キー、カスタムボード、UI のログだけを持つ。

## 2. データ取得のトリガー

| トリガー | 動作 |
|----------|------|
| 起動時 | 保存済みの接続設定 (`load_server_settings`) で `GET /api/me` を呼ぶ。成功すれば全体の読み込みへ。サーバーに届かないときは、スプラッシュで「サーバーに接続しています...」と出して再試行する (1 秒、2 秒、4 秒と倍にして上限 30 秒。「接続設定を変える」で接続画面へ)。API キーが拒否された (401) ときだけ接続画面を出す |
| 接続画面で「接続」 | `GET /api/health` と `GET /api/me` で確かめ、成功したら設定を保存して全体の読み込みへ |
| 全体の読み込み (`loadWorkspace`) | カタログと絵文字を読み、`times*` (サーバーの `channel_prefix`) とカスタムボードのチャンネルを `POST /api/channels/batch` で読む。あわせて `PUT /api/sync/targets` でカスタムボードのチャンネルを伝える |
| サーバーの SSE `channel-updated` | そのチャンネルだけを `GET /api/channels/{id}` で読み直す (200ms 内に重なったものはまとめて読む) |
| 更新ボタン (↻) | 表示中のボードのチャンネルをまとめて `POST /api/sync/channels` で依頼する (500 件ごとに分ける)。応答の `batchId` と同じ `batchId` の `sync-completed` を受けるか、60 秒たつと、更新中の表示を戻す |
| カスタムボードの追加 / 変更 / 削除 | `PUT /api/sync/targets` で同期対象を更新し、ストアにないチャンネルだけを読む |
| リアクションの操作 | `POST /api/reactions/toggle` のあと、対象チャンネルを `GET /api/channels/{id}` で読み直す |
| メッセージの投稿 | `POST /api/messages`。サーバーが投稿先を同期し、`channel-updated` で反映される |
| SSE が切れている間 | 30 秒ごとに `GET /api/sync/status` で接続を確かめ、保持しているチャンネルを読み直す |

5 分ごとの自動更新タイマーはありません。
Slack を見に行くのはサーバーの同期ワーカー (既定で 1 分に 1 回程度の呼び出し間隔) だけで、デスクトップアプリは読むだけです。

更新ボタンは、「Slack から取り直す」のではなく、「サーバーに今すぐ同期を頼む」ボタンです。
サーバーは頼まれたチャンネルを 1 つのバッチとして順に同期し、新しい内容が DB に入ると `channel-updated` を流します。
同期は Slack の呼び出し回数制限を受けるため、数十秒から数分かかることがあります (サーバーは先に DB の内容を返せます)。

## 3. 読み込みのフロー

### 3.1 起動と全体の読み込み

1. `App.tsx` が `loadServerSettings` で URL と API キーを読み、`GET /api/me` を呼ぶ。`/api/me` はワークスペース URL、自分のユーザー ID とアバター、`channelPrefix` を返す。
2. カスタムボードの読み込み (`hydrateBoards`) が終わってから、`loadWorkspace` を始める。
3. `GET /api/channels` (カタログ) と `GET /api/emoji` を並列で読む。
4. カタログのうち、名前が `channelPrefix` で始まるチャンネルと、カスタムボードが使うチャンネルを選ぶ。
5. 選んだチャンネルを 50 件ずつ `POST /api/channels/batch` で読み、届いた順にストアへ入れる。
6. `startServerSession` が `GET /api/events` (SSE) を購読する。

ストアへの反映は 50 件ごとなので、全部が届く前から表示が始まります。

### 3.2 サーバーが DB に貯める流れ (参考)

サーバーの同期ワーカーが、次のように Slack から DB へ取り込みます。
詳しくは [SLACK_API.md](SLACK_API.md) の「同期ワーカーからの呼び出し」を参照してください。

1. `conversations.list` と `emoji.list` を 1 時間ごとに取得する。
2. 同期対象 (`times*` とカスタムボード) を順に巡回する。各チャンネルで最新の `conversations.history` を読み、新着、リアクション、返信の変化を DB に書く。
3. 新しい内容が入ったチャンネルごとに、`channel-updated` を流す。

## 4. 表示用モデルへの変換 (Rust)

`crates/core/src/slack/models.rs` の `build_message_display_item` が、Slack のメッセージ 1 件を `MessageDisplayItem` に変える。
サーバーは DB に保存した `raw_json` からこの変換を行います。

| 処理 | 担当 |
|------|------|
| 本文を `MessageInlineSegment` の列に分割 (太字、斜体、取り消し線、コード、リンク、絵文字) | `slack/parser.rs` |
| 添付やファイルから画像 URL の候補を抽出 | `slack/extract.rs` |
| カスタム絵文字の alias を解決 | `slack/emoji.rs` |
| パーマリンクの組み立て | `slack/models.rs` |

本文の解析は、旧 WinUI 版と同じ「トグル方式」を引き継いでいます。
`*` `_` `~` は、コードの外で開始 / 終了を切り替えます。
バッククォートはコードの開始 / 終了を切り替え、絵文字はコードの外だけで解釈します。
先に HTML エンティティを戻します。
`<url|label>` 形式のリンクは、ホスト名で表示します。

## 5. ストアと表示対象の選択 (React)

| ストア | 内容 |
|--------|------|
| `authStore` | 接続状態 (`checking` / `loggedOut` / `authenticating` / `authenticated`。`loggedOut` は「サーバーに未接続」)、サーバー URL、ユーザー ID、アバター、`channelPrefix` |
| `channelsStore` | `channelId → ChannelWithMessages` の Map |
| `emojiStore` | カスタム絵文字の名前 → URL |
| `boardStore` | カスタムボード (名前とチャンネル ID の一覧)、選択中のボード、読み込み済みフラグ |
| `uiStore` | 表示フィルター、更新中フラグ、サーバーに届くか、開いている吹き出し、状態メッセージ、画像プレビュー、ユーザー情報ダイアログの対象 |

表示するチャンネルは `store/visibleChannels.ts` の `selectVisibleChannels` が決めます。

| フィルター | 表示 |
|-----------|------|
| `All` | 名前が `channelPrefix` (既定 `times`) で始まる全チャンネル |
| `JoinedOnly` | 上のうち参加中 |
| `NotJoinedOnly` | 上のうち未参加 |
| `CustomOnly` | 選択中のカスタムボードのチャンネル (選んだ順) |

`channelPrefix` は `GET /api/me` が返す値です (サーバーの `sync.channel_prefix`)。
アプリ側に同じ定数を持たないので、サーバーの設定と食い違いません。

`All` / `JoinedOnly` / `NotJoinedOnly` の並びは、参加中を先に、次に最終メッセージの新しい順、最後に名前順です。
ストアにはカスタムボード用に読み込んだ接頭辞以外のチャンネルも入りますが、この 3 つのフィルターには出ません。

## 6. カスタムボードの保存

- `boardStore` と表示フィルターが変わるたびに、`usePersistBoardState` が `custom-board.json` を書く (Tauri 側の `save_custom_boards`)。
- 起動時に `hydrateBoards` が読み込み、読み込みが終わるまでは保存しない (読み込み前に既存ファイルを上書きしないため)。
- 最初の読み込みは、この読み込みが終わってから始める (ボードのチャンネルを一緒に読むため)。
- ボードのチャンネルが変わるたびに `PUT /api/sync/targets` でサーバーへ伝える。サーバーは、接頭辞に合わないチャンネルを `custom` 由来の同期対象として覚え、まだ一度も同期していないものはすぐ同期を始める。
- Rust 側 (`persistence/boards.rs` の `normalize`) が、保存と読み込みの両方で次を整える。
  - 空の名前は「カスタムボード」にする
  - 重複と空のチャンネル ID を除く
  - チャンネルのないボードを除く
  - 最大 10 件にする
  - 選択中の番号とフィルターを矛盾のない値にする

カスタムボードの定義はこの PC の Tauri 側に保存されます。
サーバーには、必要なチャンネルの ID の一覧だけを伝えます。
サーバーを複数のクライアントで共有するときは、最後に `PUT` したクライアントの一覧がサーバーの `custom` 対象になります。

## 7. 画面の構成

```
App
├─ StartupSplash / AuthenticationView   (接続状態による。後者は「サーバー接続」画面)
├─ StatusPanelView                      状態メッセージ (サーバーに届かないときの表示を含む)
├─ ChannelBoardView                     チャンネル列 (横方向に仮想化)
│   └─ ChannelCardView
│       └─ MessageItemView (返信は再帰)
│           ├─ MessageBody              segments の描画
│           ├─ MessageImages            「画像を表示」で取得
│           └─ ReactionBar
├─ MainControllerView                   下部のコントローラー (CONTROLER.md)
├─ ImagePreviewOverlay
└─ UserProfileDialog
```

- チャンネル列は `@tanstack/react-virtual` で見えている範囲だけを描画する。カードの幅は通常 320px、スレッド返信のあるチャンネルは 356px。
- メッセージのアバターをクリックすると、`UserProfileDialog` が開く。

## 8. 状態パネルとサーバーとの接続

| 状況 | 表示 |
|------|------|
| サーバーに届かない (SSE が切れた、ポーリングが失敗した) | 「サーバーに接続できません。再接続しています...」。SSE が戻ると (またはポーリングが成功すると) 自動で消え、保持しているチャンネルを読み直す |
| サーバーが `sync-error` を流した | その文言をそのまま出す。文言に `scope` を含むとき (`missing_scope` など) は、必要な OAuth スコープの一覧も出す。次の `sync-completed` (失敗 0) で消える |
| API キーが拒否された (HTTP 401) | 接続画面に戻り、「API キーが受け付けられませんでした」と出す。保存した URL とキーは残す |
| 読み込みや同期の依頼が失敗した | そのメッセージを出す |

SSE は `fetch` のストリームで読みます (`EventSource` はヘッダーを付けられず、API キーを URL に載せることになるため)。
切れたときは 1 秒、2 秒、4 秒と待ち時間を倍にして再接続し (上限 30 秒)、つながるまでは 30 秒ごとのポーリングで補います。

## 9. 画像とアバター

| 種類 | 取得方法 |
|------|----------|
| アバター | 公開 URL なので `<img>` で直接読み込む |
| メッセージの画像 | 「画像を表示」を押したときに `GET /api/images?url=` を `fetch` で取り (候補 URL を順に試す)、`URL.createObjectURL` の URL にして表示する。クリックで拡大 (Esc、背景クリック、閉じるボタンで閉じる) |
| カスタム絵文字 | URL を `<img>` で直接読み込む |

メッセージの画像は Bearer トークンが必要で、WebView の `<img>` では付けられません。
そのためサーバーが取得して返します (Slack のホストにだけトークンを付けます。[SERVER.md](SERVER.md) を参照)。
画像の object URL は、表示している要素とプレビューの両方が使い終わったときに `revokeObjectURL` します (`api/client.ts` の参照カウント)。

## 10. 主要ファイル一覧

| ファイル | 役割 |
|----------|------|
| `crates/server/src/routes.rs` | REST と SSE のエンドポイント |
| `crates/core/src/sync/` | Slack から SQLite への同期とスケジューラー |
| `crates/core/src/db/view.rs` | DB から表示用モデルを組み立てる |
| `crates/core/src/slack/models.rs` | 表示用モデルと変換 |
| `crates/core/src/slack/parser.rs` | 本文の解析 |
| `src-tauri/src/commands/server_settings.rs` | サーバーの URL と API キーの保存 (API キーは資格情報ストア) |
| `src-tauri/src/commands/boards.rs` | カスタムボードの読み書き |
| `src/api/client.ts` | サーバーの REST を呼ぶ `fetch` ラッパー、画像の取得 |
| `src/api/events.ts`、`src/api/sse.ts` | SSE の購読 (`fetch` のストリーム、再接続、パーサー) |
| `src/App.tsx` | 起動、接続、表示チャンネルの選択 |
| `src/actions/session.ts` | 全体の読み込み、更新ボタン、SSE の処理、ポーリング、接続の解除 |
| `src/actions/boards.ts` | カスタムボードの追加・変更・削除・選択、同期対象の更新 |
| `src/store/visibleChannels.ts` | フィルターごとの表示対象 |
| `src/components/ChannelBoardView.tsx` | チャンネル列の仮想化 |
