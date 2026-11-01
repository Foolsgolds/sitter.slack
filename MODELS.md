# Models ドキュメント

表示用モデルの一覧です。
Rust (`crates/core/src/slack/models.rs`) が正で、TypeScript (`src/types/models.ts`) はそれに合わせた型です。
Rust 側は `serde` の `rename_all = "camelCase"` で JSON にしているため、フィールド名は TypeScript と同じ camelCase になります。
2 つのファイルは今のところ手で同期しています。

## クラス一覧

| 型 | 役割 |
|----|------|
| `ChannelWithMessages` | 1 チャンネルとそのメッセージ (ボードの 1 列) |
| `MessageDisplayItem` | 1 メッセージ。本文、画像、リアクション、スレッド返信を持つ |
| `MessageInlineSegment` | 本文の 1 区間 (テキスト / リンク / 絵文字) |
| `MessageImageItem` | 画像 1 枚 (候補 URL の一覧) |
| `MessageReactionItem` | リアクション 1 種類 (名前と件数) |
| `ChannelSummary` | チャンネルの要約。全チャンネルのカタログ用 |
| `SlackUserProfileInfo` | ユーザー情報ダイアログ用のプロフィール |
| `AuthResult` | 認証結果 (ワークスペース URL、ユーザー ID、アバター) |
| `CustomBoardDefinition` / `CustomBoardState` | カスタムボードの保存形式 |

## 各型の役割

### `ChannelWithMessages`

| フィールド | 内容 |
|-----------|------|
| `channelId` | チャンネル ID (ボードのキー) |
| `channelUri` | Slack のチャンネル URL |
| `name` | チャンネル名 |
| `topicValue` / `purposeValue` | トピック / 説明 (全体更新で取得したときだけ入る) |
| `numMembers` | メンバー数 (同上) |
| `isMember` | 参加中か |
| `lastMessageTs` | 最新メッセージの `ts` (並び順に使う) |
| `lastFetchedTs` | 最後に取得したメッセージの `ts` |
| `messages` | `MessageDisplayItem` の一覧 (新しい順) |

`load_channels` と `refresh_channel` はチャンネルのカタログから作るため、トピック、説明、メンバー数は入りません。

### `MessageDisplayItem`

| フィールド | 内容 |
|-----------|------|
| `user` | 投稿者のユーザー ID |
| `ts` | メッセージの `ts` |
| `text` | 元の本文 |
| `userAvatarUri` | 投稿者のアバター URL |
| `permalinkUri` | メッセージの URL |
| `segments` | 本文を分割した `MessageInlineSegment` |
| `images` | `MessageImageItem` の一覧 |
| `reactions` | `MessageReactionItem` の一覧 |
| `replies` | スレッド返信 (`MessageDisplayItem`、返信自身の `replies` は空) |

### `MessageInlineSegment`

| フィールド | 内容 |
|-----------|------|
| `segType` | `text` / `link` / `emoji` |
| `text` | 表示する文字 (絵文字のときは絵文字名) |
| `uri` | リンク先 (`link` のとき) |
| `isBold` / `isItalic` / `isStrikethrough` / `isCode` | スタイル (`text` のとき) |

### `MessageImageItem`

`candidateUrls` に、同じ画像の URL を優先度順に持ちます (空と重複は除きます)。
フロントエンドは `fetch_image` に渡し、取得できた最初の URL を表示します。

### `MessageReactionItem`

`name` と `count` を持ちます。
カスタム絵文字は `emojiStore` の URL で描画します。

### `CustomBoardDefinition` / `CustomBoardState`

```json
{
  "activeFilter": "CustomOnly",
  "activeCustomBoardIndex": 0,
  "customBoards": [
    { "name": "雑談", "channelIds": ["C0123", "C0456"] }
  ]
}
```

旧 WinUI 版はチャンネルを名前で記録していましたが、Tauri 版はチャンネル ID で記録します。
チャンネル名が変わっても、ボードから外れません。
旧版の `custom-board.json` は読み込みません (名前から ID への変換をしていないため)。

## Rust 内部の型

Slack の応答を受ける型は `slack/dto.rs` にあります (`SlackConversation`、`SlackMessage`、`SlackUser`)。
未知のフィールドや欠けたフィールドで読み込みが失敗しないよう、緩い定義にしています。
これらは画面には渡さず、`models.rs` で表示用モデルに変換します。

## 依存関係

```
commands/channels.rs
   ├─ slack/endpoints.rs ──► slack/dto.rs
   └─ slack/models.rs ──► slack/parser.rs
                       ├─ slack/extract.rs
                       └─ slack/emoji.rs
```
