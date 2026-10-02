CREATE TABLE IF NOT EXISTS slots (
    slot BIGINT PRIMARY KEY,
    parent BIGINT,
    status TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS transactions (
    signature TEXT PRIMARY KEY,
    slot BIGINT NOT NULL,
    tx_index BIGINT NOT NULL,
    is_vote BOOLEAN NOT NULL,
    success BOOLEAN NOT NULL,
    fee BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    pubkey TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    lamports BIGINT NOT NULL,
    slot BIGINT NOT NULL,
    data BYTEA NOT NULL,
    executable BOOLEAN NOT NULL,
    write_version BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS block_meta (
    slot BIGINT PRIMARY KEY,
    blockhash TEXT NOT NULL,
    parent_slot BIGINT NOT NULL,
    parent_blockhash TEXT NOT NULL,
    block_time BIGINT,
    tx_count BIGINT NOT NULL,
    block_height BIGINT
);