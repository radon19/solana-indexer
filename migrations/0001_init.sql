-- This file creates the tables used by the Solana indexer.
-- Basic terms:
-- A slot is a time window in which Solana may produce a block.
-- A transaction contains instructions (program calls). A CPI is one program
-- calling another program while an instruction is running.
-- A mint address identifies a token. Amounts are whole raw units, before decimals.
-- NUMERIC(20,0) keeps large unsigned amounts exact, without rounding.
-- A primary key identifies a row; a foreign key links it to another table.
-- Event keys include the parent instruction so separate CPI calls stay distinct.
-- parent_ix_index is -1 for a top-level call; otherwise it identifies its parent.
-- block_time is seconds since 1970 and may be unknown (NULL).
-- created_at is when the database saved the row, not when the block happened.

-- Each row records a slot, its parent slot, and its confirmation status.
CREATE TABLE slots (
    slot       BIGINT PRIMARY KEY,
    parent     BIGINT,
    status     TEXT NOT NULL,   -- How far this slot has been confirmed, or whether it was dropped.
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Basic block details, including its hash, parent, time, and transaction count.
CREATE TABLE block_meta (
    slot             BIGINT PRIMARY KEY,
    blockhash        TEXT NOT NULL,
    parent_slot      BIGINT NOT NULL,
    parent_blockhash TEXT NOT NULL,
    block_time       BIGINT,
    tx_count         BIGINT NOT NULL,
    block_height     BIGINT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- One row per transaction, with its position, result, and network fee.
CREATE TABLE transactions (
    signature  TEXT PRIMARY KEY,        -- Unique transaction ID; prevents duplicate rows on replay.
    slot       BIGINT NOT NULL,
    tx_index   BIGINT NOT NULL,
    success    BOOLEAN NOT NULL,
    versioned  BOOLEAN NOT NULL,
    error      BYTEA,
    fee        NUMERIC(20,0) NOT NULL,
    fee_payer  TEXT,                    -- First account in the transaction; pays its fee.
    block_time BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT transactions_fee_u64 CHECK (fee BETWEEN 0 AND 18446744073709551615)
);

-- Latest saved account state. The writer uses write_version to ignore older updates.
CREATE TABLE accounts (
    pubkey        TEXT PRIMARY KEY,
    owner         TEXT NOT NULL,
    lamports      NUMERIC(20,0) NOT NULL,
    slot          BIGINT NOT NULL,
    data          BYTEA NOT NULL,
    executable    BOOLEAN NOT NULL,
    write_version BIGINT NOT NULL,
    CONSTRAINT accounts_lamports_u64 CHECK (lamports BETWEEN 0 AND 18446744073709551615)
);

-- One row per parsed token swap. A router swap and its individual swaps can
-- have separate rows; their instruction positions keep them distinct.
CREATE TABLE swap_events (
    slot          BIGINT NOT NULL,
    signature     TEXT NOT NULL,
    ix_index      INT NOT NULL,
    parent_ix_index INT NOT NULL,
    is_inner      BOOLEAN NOT NULL DEFAULT FALSE,

    user_address  TEXT NOT NULL,
    protocol      TEXT NOT NULL,   -- Name of the exchange or program.
    aggregator    TEXT,            -- Router name, when a router combined the swaps.

    input_mint    TEXT NOT NULL,
    input_amount  NUMERIC(20,0) NOT NULL,
    output_mint   TEXT NOT NULL,
    output_amount NUMERIC(20,0) NOT NULL,

    block_time    BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, parent_ix_index, is_inner),
    CONSTRAINT swap_events_amounts_u64 CHECK (input_amount BETWEEN 0 AND 18446744073709551615 AND output_amount BETWEEN 0 AND 18446744073709551615),
    CONSTRAINT swap_events_parent_identity CHECK ((is_inner AND parent_ix_index >= 0) OR (NOT is_inner AND parent_ix_index = -1))
);

-- Individual steps in a Jupiter swap route, linked to their swap row.
-- percent_bps is the share sent through a step: 10,000 means 100 percent.
-- Deleting the parent swap also deletes its route steps.
CREATE TABLE route_hops (
    slot         BIGINT NOT NULL,
    signature    TEXT NOT NULL,
    ix_index     INT NOT NULL,
    parent_ix_index INT NOT NULL,
    is_inner     BOOLEAN NOT NULL DEFAULT FALSE,
    hop_index    INT NOT NULL,

    percent_bps  INT NOT NULL,
    program_id   TEXT,
    protocol     TEXT NOT NULL,
    in_mint      TEXT,
    out_mint     TEXT,
    in_amount    NUMERIC(20,0),
    out_amount   NUMERIC(20,0),

    PRIMARY KEY (slot, signature, ix_index, parent_ix_index, is_inner, hop_index),
    FOREIGN KEY (slot, signature, ix_index, parent_ix_index, is_inner)
        REFERENCES swap_events (slot, signature, ix_index, parent_ix_index, is_inner)
        ON DELETE CASCADE,
    CONSTRAINT route_hops_amounts_u64 CHECK (in_amount BETWEEN 0 AND 18446744073709551615 AND out_amount BETWEEN 0 AND 18446744073709551615)
);

-- Pump.fun token purchases and sales, including token and payment amounts.
CREATE TABLE trade_events (
    slot          BIGINT NOT NULL,
    signature     TEXT NOT NULL,
    ix_index      INT NOT NULL,
    parent_ix_index INT NOT NULL,
    is_inner      BOOLEAN NOT NULL DEFAULT FALSE,

    user_address  TEXT NOT NULL,
    protocol      TEXT NOT NULL,
    mint          TEXT NOT NULL,
    side          TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    token_amount  NUMERIC(20,0) NOT NULL,
    quote_amount  NUMERIC(20,0) NOT NULL,  -- Amount of the payment token; it may be SOL or another token.

    block_time    BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, parent_ix_index, is_inner),
    CONSTRAINT trade_events_amounts_u64 CHECK (token_amount BETWEEN 0 AND 18446744073709551615 AND quote_amount BETWEEN 0 AND 18446744073709551615),
    CONSTRAINT trade_events_parent_identity CHECK ((is_inner AND parent_ix_index >= 0) OR (NOT is_inner AND parent_ix_index = -1))
);

-- Details recorded when Pump.fun creates a token, such as its mint and creator.
CREATE TABLE launch_events (
    slot              BIGINT NOT NULL,
    signature         TEXT NOT NULL,
    ix_index          INT NOT NULL,
    parent_ix_index INT NOT NULL,
    is_inner          BOOLEAN NOT NULL DEFAULT FALSE,

    timestamp_unix_ms BIGINT NOT NULL,   -- Time recorded by the parser, in milliseconds since 1970.
    protocol          TEXT NOT NULL,
    mint              TEXT NOT NULL,
    creator           TEXT NOT NULL,
    user_address      TEXT NOT NULL,
    name              TEXT NOT NULL,
    symbol            TEXT NOT NULL,
    uri               TEXT NOT NULL,
    bonding_curve     TEXT NOT NULL,

    block_time        BIGINT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, parent_ix_index, is_inner),
    CONSTRAINT launch_events_parent_identity CHECK ((is_inner AND parent_ix_index >= 0) OR (NOT is_inner AND parent_ix_index = -1))
);

-- Tokens added to or removed from a trading pool. kind keeps an add and
-- a removal separate even when both happen in the same instruction.
CREATE TABLE liquidity_events (
    slot           BIGINT NOT NULL,
    signature      TEXT NOT NULL,
    ix_index       INT NOT NULL,
    parent_ix_index INT NOT NULL,
    is_inner       BOOLEAN NOT NULL DEFAULT FALSE,
    kind           TEXT NOT NULL CHECK (kind IN ('add', 'remove')),

    user_address   TEXT NOT NULL,
    protocol       TEXT NOT NULL,
    pool           TEXT NOT NULL,
    token_a_mint   TEXT NOT NULL,
    token_a_amount NUMERIC(20,0) NOT NULL,
    token_b_mint   TEXT NOT NULL,
    token_b_amount NUMERIC(20,0) NOT NULL,

    block_time     BIGINT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, parent_ix_index, is_inner, kind),
    CONSTRAINT liquidity_events_amounts_u64 CHECK (token_a_amount BETWEEN 0 AND 18446744073709551615 AND token_b_amount BETWEEN 0 AND 18446744073709551615),
    CONSTRAINT liquidity_events_parent_identity CHECK ((is_inner AND parent_ix_index >= 0) OR (NOT is_inner AND parent_ix_index = -1))
);

-- Details recorded when a trading pool is created, including its two tokens.
CREATE TABLE pool_events (
    slot         BIGINT NOT NULL,
    signature    TEXT NOT NULL,
    ix_index     INT NOT NULL,
    parent_ix_index INT NOT NULL,
    is_inner     BOOLEAN NOT NULL DEFAULT FALSE,

    protocol     TEXT NOT NULL,
    pool         TEXT NOT NULL,
    token_a_mint TEXT NOT NULL,
    token_b_mint TEXT NOT NULL,

    block_time   BIGINT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, parent_ix_index, is_inner),
    CONSTRAINT pool_events_parent_identity CHECK ((is_inner AND parent_ix_index >= 0) OR (NOT is_inner AND parent_ix_index = -1))
);

-- Indexes speed up common searches by slot, token, user, pool, or time.
CREATE INDEX idx_slots_status        ON slots (status);

CREATE INDEX idx_tx_slot             ON transactions (slot);
CREATE INDEX idx_tx_block_time       ON transactions (block_time);
CREATE INDEX idx_tx_fee_payer        ON transactions (fee_payer);

CREATE INDEX idx_accounts_owner      ON accounts (owner);

CREATE INDEX idx_swap_slot           ON swap_events (slot);
CREATE INDEX idx_swap_signature      ON swap_events (signature);
CREATE INDEX idx_swap_protocol       ON swap_events (protocol);
CREATE INDEX idx_swap_user           ON swap_events (user_address);
CREATE INDEX idx_swap_input_mint     ON swap_events (input_mint);
CREATE INDEX idx_swap_output_mint    ON swap_events (output_mint);
CREATE INDEX idx_swap_time           ON swap_events (block_time);

CREATE INDEX idx_trade_ev_slot       ON trade_events (slot);
CREATE INDEX idx_trade_ev_signature  ON trade_events (signature);
CREATE INDEX idx_trade_ev_mint       ON trade_events (mint);
CREATE INDEX idx_trade_ev_user       ON trade_events (user_address);

CREATE INDEX idx_launch_slot         ON launch_events (slot);
CREATE INDEX idx_launch_signature    ON launch_events (signature);
CREATE INDEX idx_launch_mint         ON launch_events (mint);
CREATE INDEX idx_launch_creator      ON launch_events (creator);
CREATE INDEX idx_launch_curve        ON launch_events (bonding_curve);

CREATE INDEX idx_liq_slot            ON liquidity_events (slot);
CREATE INDEX idx_liq_signature       ON liquidity_events (signature);
CREATE INDEX idx_liq_pool            ON liquidity_events (pool);
CREATE INDEX idx_liq_user            ON liquidity_events (user_address);

CREATE INDEX idx_pool_ev_slot        ON pool_events (slot);
CREATE INDEX idx_pool_ev_signature   ON pool_events (signature);
CREATE INDEX idx_pool_ev_pool        ON pool_events (pool);
CREATE INDEX idx_pool_ev_token_a     ON pool_events (token_a_mint);
CREATE INDEX idx_pool_ev_token_b     ON pool_events (token_b_mint);

-- Batches that could not be stored normally. The payload keeps their rows
-- so they can be retried; replayed_at is filled after a successful retry.
CREATE TABLE dead_letter_batches (
    id             BIGSERIAL PRIMARY KEY,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    attempts       INT NOT NULL,
    row_count      INT NOT NULL,
    sqlstate       TEXT,
    error          TEXT NOT NULL,
    payload_format TEXT NOT NULL DEFAULT 'json-v2',
    payload        BYTEA NOT NULL,
    replayed_at    TIMESTAMPTZ,
    replay_error   TEXT
);

-- Find failed batches by age, or find those still waiting for a successful retry.
CREATE INDEX idx_dlq_created_at ON dead_letter_batches (created_at);
CREATE INDEX idx_dlq_pending ON dead_letter_batches (created_at)
    WHERE replayed_at IS NULL;

-- One checkpoint per completely stored finalized block, saved in the same
-- database transaction as that block and its parsed events.
-- scope separates endpoints and subscription settings. payload stores the
-- checkpoint details; previous_slot inside it links to the prior checkpoint.
-- A fresh run starts a new span without claiming earlier blocks were indexed.
CREATE TABLE resume (
    scope TEXT NOT NULL,
    slot BIGINT NOT NULL CHECK (slot >= 0),
    blockhash TEXT NOT NULL,
    payload BYTEA NOT NULL,
    completed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (scope, slot)
);
