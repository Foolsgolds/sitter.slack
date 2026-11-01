# HANDOFF

## 現在の状態

- ブランチ: `master`
- ターゲット: Tauri 2 (Rust + React / TypeScript)
- WinUI 3 / .NET 8 版からの移行は完了し、旧実装 (`SlackSitter/`、`SlackSitter.slnx`) は削除済み。旧実装が必要なときは Git の履歴から取り出す (移行前の最後のコミットは `37e52cb`)
- 移行の範囲: 認証、ボード表示、メッセージ / 画像 / リアクション、投稿、ユーザー情報、カスタムボード、自動更新、ログ表示

## 開発の始め方

```powershell
npm install
npm run tauri dev
```

- 初回のビルドには数分かかる。以降は変更が自動で反映される (Rust は再ビルド、画面は HMR)
- Node.js は 20.19 以上が必要 (`package.json` の Volta 設定で 24.15.0 に固定)
- 起動時にポート 1420 (Vite) が使用中だと `tauri dev` は失敗する。前回の Vite が残っていないか確認する
- Rust のテスト: `cargo test` (リポジトリ直下。ワークスペース全体)
- 型チェック: `npx tsc --noEmit`

構成と各文書へのリンクは [README.md](README.md) を参照してください。

## 設計上の判断

- Slack との通信はすべて Rust 側。フロントエンドは Tauri のコマンドとイベントだけを使う
- トークンは OS の資格情報ストアに保存する。旧版の `.env` (平文) は読まない
- カスタムボードのチャンネルは名前ではなく ID で記録する。旧版の `custom-board.json` は読み込めない
- 全体更新は `channel-batch` イベントでページごとに流す。全チャンネルの完了を待たずに表示が始まる
- 更新ボタンは、差分取得ではなく、表示中のボードのチャンネルの全件再取得にしている (理由は [VIEW_FLOW.md](VIEW_FLOW.md))
- `conversations.list` もリトライ対象にし、1 ページを 200 件にした (旧版との違い。理由は [SLACK_API.md](SLACK_API.md))
- 本文の解析は旧版と同じ「トグル方式」をそのまま移した。`*` が閉じていないと、その後ろ全体が太字になる

## 既知の注意点

- Rust の表示用モデルと `src/types/models.ts` は手で同期している。片方を変えたらもう片方も変える
- 「`times` で始まる」という条件が Rust (`commands/channels.rs`) と TypeScript (`src/constants.ts`) の両方にある。変えるときは両方を変える
- `refresh_channel` と `load_channels` はカタログからチャンネルを作るため、トピック、説明、メンバー数が入らない
- チャンネルが多いと全体更新に時間がかかる。実測では 199 チャンネルの読み込みに約 3 分 20 秒 (Slack の `conversations.history` の呼び出し回数制限による待ちを含む)。待ちはログ (`retrying slack api call`) に残る
- 画面の自動テストはない。画面の確認は実際のウインドウか、Tauri API をモックしたブラウザで行う
- アクセストークンの権限不足 (`missing_scope`) のときは、必要な権限の一覧を状態パネルに表示する。`reactions:write` は一覧に含まれていない

## 次に着手しやすい候補

- フロントエンドの自動テスト (`selectVisibleChannels`、`normalize` に相当する処理など)
- リアクションの新規追加 (今は既存のリアクションのクリックによる追加 / 取り消しだけ)
- 本文の解析の改善 (閉じていない `*` の扱いなど)
- `src/types/models.ts` を Rust から自動生成する

## 補足

`.github/copilot-instructions.md` にあるルール:

- 読みづらくなる UI サイズ変更は戻す
- 下部アイコンコントローラの部品名は `BottomControllerView` ではなく `MainControllerView`
