PRAGMA foreign_keys = ON;
BEGIN IMMEDIATE;
CREATE TABLE library_meta (library_id TEXT PRIMARY KEY NOT NULL);
CREATE TABLE contents (
    hash TEXT PRIMARY KEY NOT NULL,
    bytes INTEGER NOT NULL CHECK(bytes >= 0)
);
CREATE TABLE documents (
    document_id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    input_format TEXT NOT NULL,
    source_path TEXT NOT NULL UNIQUE,
    initial_hash TEXT NOT NULL REFERENCES contents(hash),
    head_version_id TEXT REFERENCES versions(version_id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_render TEXT
);
CREATE TABLE versions (
    version_id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(document_id),
    seq INTEGER NOT NULL CHECK(seq > 0),
    parent_version_id TEXT REFERENCES versions(version_id),
    source_hash TEXT NOT NULL REFERENCES contents(hash),
    body TEXT NOT NULL,
    UNIQUE(document_id, seq)
);
CREATE TABLE edits (
    edit_id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL REFERENCES documents(document_id),
    status TEXT NOT NULL CHECK(status IN ('active','interrupted','committed','ended')),
    body TEXT NOT NULL
);
CREATE UNIQUE INDEX one_edit ON edits(document_id) WHERE status IN ('active','interrupted');
CREATE TABLE jobs (
    job_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    document_id TEXT REFERENCES documents(document_id),
    source_hash TEXT REFERENCES contents(hash),
    state TEXT NOT NULL CHECK(state IN ('queued','running','done','error','cancelled','interrupted')),
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE renders (
    document_id TEXT NOT NULL REFERENCES documents(document_id),
    source_hash TEXT NOT NULL REFERENCES contents(hash),
    engine_build_id TEXT NOT NULL,
    options_hash TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    PRIMARY KEY(document_id,source_hash,engine_build_id,options_hash)
);
CREATE TABLE worktree_intents (
    document_id TEXT PRIMARY KEY NOT NULL REFERENCES documents(document_id),
    status TEXT NOT NULL CHECK(status IN ('pending','conflict')),
    body TEXT NOT NULL
);
CREATE TABLE check_receipts (
    job_id TEXT NOT NULL REFERENCES jobs(job_id),
    edit_id TEXT PRIMARY KEY NOT NULL REFERENCES edits(edit_id),
    source_hash TEXT NOT NULL REFERENCES contents(hash),
    engine_build_id TEXT NOT NULL,
    options_hash TEXT NOT NULL
);
CREATE TABLE check_requests (
    edit_id TEXT REFERENCES edits(edit_id),
    job_id TEXT PRIMARY KEY NOT NULL REFERENCES jobs(job_id),
    parameters TEXT NOT NULL
);
COMMIT;
