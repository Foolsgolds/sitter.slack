# SlackSitter ボード表示ロジック・フロー

Slack からデータを取得し、ボード (チャンネルの横並び) に表示するまでの流れです。

## 1. 全体像

```
Slack Web API
   ▲  (reqwest)
Rust: commands/ ── slack/ (client, endpoints, parser, extract, models: crates/core)
   │   Tauri コマンド (invoke)            ▲
   │   Tauri イベント (emit)              │
   ▼                                      │
React: api/ ── actions/ ── store/ (zustand) ── components/
```

- Slack API の呼び出し、メッセージ本文の解析、画像 URL の抽出はすべて Rust 側で行う。
- Rust は画面にそのまま描ける「表示用モデル」(`ChannelWithMessages`) を返す。
- フロントエンドは zustand のストアに入れて描画するだけで、Slack の生データは扱わない。

## 2. データ取得のトリガー

| トリガー | 動作 |
|----------|------|
| 起動時 | 保存済みトークンで自動ログイン (`try_auto_login`)。成功すれば全体更新へ |
| 認証成功 / トークン変更 | 全体更新 (`refresh_workspace_data`) |
| 5 分ごと (自動更新が有効なとき) | 全体更新。更新中なら何もしない |
| 更新ボタン (↻) | 表示中のボードのチャンネルだけを再取得 (`load_channels`) |
| カスタムボードの追加 / 変更 / 選択 | ストアにないチャンネルだけを取得 (`load_channels`) |
| リアクションの操作 | 対象チャンネルを 1 件再取得 (`refresh_channel`) |
| メッセージの投稿 | 投稿先がボードにあれば、そのチャンネルを 1 件再取得 |

更新ボタンは、表示中のボードに載っているチャンネルを全件取り直します。
差分 (新着だけ) の取得は行いません。
新着だけで置き換えるとメッセージが消えるうえ、リアクションやスレッド返信の更新も反映されないためです。
取得できたチャンネルが 0 件だった場合 (チャンネル一覧が未取得のときなど) は、全体更新に切り替えます。

## 3. 全体更新のフロー (`refresh_workspace_data`)

1. 絵文字一覧 (`emoji.list`) と自分のプロフィール (`users.info`) を並列で取得する。
2. `conversations.list` をページごとに取得する。アーカイブ済みは除く。
3. 各ページで、次のチャンネルをボード用として選ぶ。
   - 名前が `times` で始まる (大文字小文字を区別する)
   - フロントエンドから渡された `extraChannelIds` (カスタムボードが使うチャンネル) に含まれる
4. 選んだチャンネルを最大 10 並列で読み込む。
   - `conversations.history` (10 件)
   - 返信のあるメッセージには `conversations.replies` (20 件)
   - 投稿者のアバター (`users.info`、1 バッチ内でキャッシュ)
5. ページごとの結果を `channel-batch` イベントで送る。フロントエンドは届いた順にストアへ入れる。
6. 全ページの取得後に、全チャンネルのカタログを保存し、`refresh-complete` を送る。失敗したときは `refresh-error` を送る。

ページ単位で送るため、全チャンネルの読み込みが終わる前から表示が始まります。

## 4. 表示用モデルへの変換 (Rust)

`slack/models.rs` の `build_message_display_item` が、Slack のメッセージ 1 件を `MessageDisplayItem` に変える。

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
| `authStore` | 認証状態 (`checking` / `loggedOut` / `authenticating` / `authenticated`)、ユーザー ID、アバター |
| `channelsStore` | `channelId → ChannelWithMessages` の Map |
| `emojiStore` | カスタム絵文字の名前 → URL |
| `boardStore` | カスタムボード (名前とチャンネル ID の一覧)、選択中のボード、読み込み済みフラグ |
| `uiStore` | 表示フィルター、更新中フラグ、自動更新の有効 / 無効、開いている吹き出し、状態メッセージ、画像プレビュー、ユーザー情報ダイアログの対象 |

表示するチャンネルは `store/visibleChannels.ts` の `selectVisibleChannels` が決めます。

| フィルター | 表示 |
|-----------|------|
| `All` | 名前が `times` で始まる全チャンネル |
| `JoinedOnly` | 上のうち参加中 |
| `NotJoinedOnly` | 上のうち未参加 |
| `CustomOnly` | 選択中のカスタムボードのチャンネル (選んだ順) |

`All` / `JoinedOnly` / `NotJoinedOnly` の並びは、参加中を先に、次に最終メッセージの新しい順、最後に名前順です。
ストアにはカスタムボード用に読み込んだ `times` 以外のチャンネルも入りますが、この 3 つのフィルターには出ません。

## 6. カスタムボードの保存

- `boardStore` と表示フィルターが変わるたびに、`usePersistBoardState` が `custom-board.json` を書く。
- 起動時に `hydrateBoards` が読み込み、読み込みが終わるまでは保存しない (読み込み前に既存ファイルを上書きしないため)。
- 最初の全体更新は、この読み込みが終わってから始める (ボードのチャンネルを `extraChannelIds` として渡すため)。
- Rust 側 (`persistence/boards.rs` の `normalize`) が、保存と読み込みの両方で次を整える。
  - 空の名前は「カスタムボード」にする
  - 重複と空のチャンネル ID を除く
  - チャンネルのないボードを除く
  - 最大 10 件にする
  - 選択中の番号とフィルターを矛盾のない値にする

## 7. 画面の構成

```
App
├─ StartupSplash / AuthenticationView   (認証状態による)
├─ StatusPanelView                      状態メッセージ
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

## 8. 画像とアバター

| 種類 | 取得方法 |
|------|----------|
| アバター | 公開 URL なので `<img>` で直接読み込む |
| メッセージの画像 | 「画像を表示」を押したときに `fetch_image` で取得し、data URL にして表示する。クリックで拡大 (Esc、背景クリック、閉じるボタンで閉じる) |
| カスタム絵文字 | URL を `<img>` で直接読み込む |

メッセージの画像は Bearer トークンが必要で、WebView の `<img>` では付けられません。
そのため Rust 側で取得します。

## 9. 主要ファイル一覧

| ファイル | 役割 |
|----------|------|
| `src-tauri/src/commands/channels.rs` | 全体更新、チャンネル読み込み、ユーザー情報、絵文字 |
| `src-tauri/src/commands/boards.rs` | カスタムボードの読み書き |
| `crates/core/src/slack/models.rs` | 表示用モデルと変換 |
| `crates/core/src/slack/parser.rs` | 本文の解析 |
| `src/App.tsx` | 起動、イベント購読、自動更新、表示チャンネルの選択 |
| `src/actions/session.ts` | 全体更新、表示中ボードの更新、ログアウト |
| `src/actions/boards.ts` | カスタムボードの追加・変更・削除・選択 |
| `src/store/visibleChannels.ts` | フィルターごとの表示対象 |
| `src/components/ChannelBoardView.tsx` | チャンネル列の仮想化 |
