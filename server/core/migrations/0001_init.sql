-- SlackSitter local cache. Timestamps named *_at are unix epoch seconds.
-- Slack `ts` values are stored as the original strings ("1700000000.000100");
-- they have a fixed width, so text ordering equals chronological ordering.

CREATE TABLE channels (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    topic        TEXT,
    purpose      TEXT,
    num_members  INTEGER,
    is_member    INTEGER NOT NULL DEFAULT 0,
    is_archived  INTEGER NOT NULL DEFAULT 0,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE users (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL DEFAULT '',
    display_name  TEXT NOT NULL DEFAULT '',
    real_name     TEXT NOT NULL DEFAULT '',
    title         TEXT NOT NULL DEFAULT '',
    status_text   TEXT NOT NULL DEFAULT '',
    image_32      TEXT,
    image_48      TEXT,
    image_72      TEXT,
    image_192     TEXT,
    fetched_at    INTEGER NOT NULL
);

-- Top-level messages and thread replies share this table. A reply is a row
-- whose thread_ts differs from its own ts.
CREATE TABLE messages (
    channel_id    TEXT NOT NULL REFERENCES channels(id),
    ts            TEXT NOT NULL,
    user_id       TEXT,
    text          TEXT,
    subtype       TEXT,
    thread_ts     TEXT,
    reply_count   INTEGER NOT NULL DEFAULT 0,
    latest_reply  TEXT,
    raw_json      TEXT NOT NULL,
    fetched_at    INTEGER NOT NULL,
    PRIMARY KEY (channel_id, ts)
);

CREATE INDEX idx_messages_thread ON messages(channel_id, thread_ts);

CREATE TABLE reactions (
    channel_id  TEXT NOT NULL,
    ts          TEXT NOT NULL,
    name        TEXT NOT NULL,
    count       INTEGER NOT NULL DEFAULT 0,
    users_json  TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (channel_id, ts, name),
    FOREIGN KEY (channel_id, ts) REFERENCES messages(channel_id, ts) ON DELETE CASCADE
);

CREATE TABLE files (
    id                    TEXT PRIMARY KEY,
    channel_id            TEXT NOT NULL,
    ts                    TEXT NOT NULL,
    mimetype              TEXT,
    thumb_64              TEXT,
    thumb_80              TEXT,
    thumb_160             TEXT,
    thumb_360             TEXT,
    thumb_480             TEXT,
    url_private           TEXT,
    url_private_download  TEXT,
    FOREIGN KEY (channel_id, ts) REFERENCES messages(channel_id, ts) ON DELETE CASCADE
);

CREATE INDEX idx_files_message ON files(channel_id, ts);

CREATE TABLE emoji (
    name  TEXT PRIMARY KEY,
    url   TEXT NOT NULL
);

CREATE TABLE sync_targets (
    channel_id  TEXT PRIMARY KEY,
    source      TEXT NOT NULL CHECK (source IN ('prefix', 'custom')),
    added_at    INTEGER NOT NULL
);

CREATE TABLE sync_state (
    channel_id       TEXT PRIMARY KEY,
    last_message_ts  TEXT,
    last_synced_at   INTEGER,
    last_error       TEXT
);
