-- Solana DEX indexer — storage for what the indexer writes TODAY.
--
-- In:  slots, block_meta, transactions, accounts   (the four store::* modules)
--      swap_events + route_hops                     (Event::Swap)
--      trade_events                                 (Event::Trade — pump.fun curve)
--      launch_events                                (Event::LaunchEvent — pump.fun Create/CreateV2)
--      liquidity_events, pool_events                (Event::LiquidityAdd/Remove, Event::PoolCreate)
--
-- Out: token/pool registry, graduation_events, transfer_events, fee_events,
--      trades/trade_legs, ingest checkpoints/gaps/dead_letters/reorgs.
--      Nothing emits or writes those today — add them when the code exists.
--
-- Idempotency: event rows key on (slot, signature, ix_index, is_inner[, kind])
-- so replay/backfill can ON CONFLICT DO NOTHING.
--
-- Amounts are u64 in Rust → BIGINT. Every real SPL amount fits below 2^63.
-- block_time is unix seconds, nullable: fill from block_meta when available,
-- or leave NULL for replay captures.

-- ─────────────────────────────────────────────────────────────────────────
-- Chain state
-- ─────────────────────────────────────────────────────────────────────────

CREATE TABLE slots (
    slot       BIGINT PRIMARY KEY,
    parent     BIGINT,
    status     TEXT NOT NULL,   -- processed | confirmed | finalized | dead | other
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

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

CREATE TABLE transactions (
    signature  TEXT PRIMARY KEY,        -- matches store::txns ON CONFLICT (signature)
    slot       BIGINT NOT NULL,
    tx_index   BIGINT NOT NULL,
    success    BOOLEAN NOT NULL,
    versioned  BOOLEAN NOT NULL,
    error      BYTEA,
    fee        BIGINT NOT NULL,
    fee_payer  TEXT,                    -- Transaction.account_keys[0]
    block_time BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Last-write-wins by (write_version), as store::accounts already upserts.
CREATE TABLE accounts (
    pubkey        TEXT PRIMARY KEY,
    owner         TEXT NOT NULL,
    lamports      BIGINT NOT NULL,
    slot          BIGINT NOT NULL,
    data          BYTEA NOT NULL,
    executable    BOOLEAN NOT NULL,
    write_version BIGINT NOT NULL
);

-- ─────────────────────────────────────────────────────────────────────────
-- Swaps (Jupiter, PumpSwap, Raydium CLMM, Orca Whirlpool, Meteora DLMM/DAMM v2/DBC)
-- ─────────────────────────────────────────────────────────────────────────

-- One row per SwapEvent. A Jupiter route is one row with aggregator='Jupiter';
-- its venue legs arrive as their own rows (is_inner, aggregator NULL).
CREATE TABLE swap_events (
    slot          BIGINT NOT NULL,
    signature     TEXT NOT NULL,
    ix_index      INT NOT NULL,
    is_inner      BOOLEAN NOT NULL DEFAULT FALSE,

    user_address  TEXT NOT NULL,
    protocol      TEXT NOT NULL,   -- Protocol::as_str()
    aggregator    TEXT,            -- 'Jupiter' or NULL

    input_mint    TEXT NOT NULL,
    input_amount  BIGINT NOT NULL,
    output_mint   TEXT NOT NULL,
    output_amount BIGINT NOT NULL,

    block_time    BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, is_inner)
);

-- Jupiter route_plan only. percent_bps: Route sends percent*100, RouteV2 sends bps.
CREATE TABLE route_hops (
    slot         BIGINT NOT NULL,
    signature    TEXT NOT NULL,
    ix_index     INT NOT NULL,
    is_inner     BOOLEAN NOT NULL DEFAULT FALSE,
    hop_index    INT NOT NULL,

    percent_bps  INT NOT NULL,
    program_id   TEXT,
    protocol     TEXT NOT NULL,
    in_mint      TEXT,
    out_mint     TEXT,
    in_amount    BIGINT,
    out_amount   BIGINT,

    PRIMARY KEY (slot, signature, ix_index, is_inner, hop_index),
    FOREIGN KEY (slot, signature, ix_index, is_inner)
        REFERENCES swap_events (slot, signature, ix_index, is_inner)
        ON DELETE CASCADE
);

-- ─────────────────────────────────────────────────────────────────────────
-- pump.fun bonding curve trades
-- ─────────────────────────────────────────────────────────────────────────

CREATE TABLE trade_events (
    slot          BIGINT NOT NULL,
    signature     TEXT NOT NULL,
    ix_index      INT NOT NULL,
    is_inner      BOOLEAN NOT NULL DEFAULT FALSE,

    user_address  TEXT NOT NULL,
    protocol      TEXT NOT NULL,
    mint          TEXT NOT NULL,
    side          TEXT NOT NULL CHECK (side IN ('buy', 'sell')),
    token_amount  BIGINT NOT NULL,
    quote_amount  BIGINT NOT NULL,  -- TradeEvent.sol_amount; quote isn't always SOL

    block_time    BIGINT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, is_inner)
);

-- ─────────────────────────────────────────────────────────────────────────
-- pump.fun launches (Create / CreateV2)
-- ─────────────────────────────────────────────────────────────────────────

CREATE TABLE launch_events (
    slot              BIGINT NOT NULL,
    signature         TEXT NOT NULL,
    ix_index          INT NOT NULL,
    is_inner          BOOLEAN NOT NULL DEFAULT FALSE,

    timestamp_unix_ms BIGINT NOT NULL,   -- wall-clock today; program event timestamp is canonical
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

    PRIMARY KEY (slot, signature, ix_index, is_inner)
);

-- ─────────────────────────────────────────────────────────────────────────
-- Liquidity + pool creation (Raydium CLMM, Orca Whirlpool)
-- ─────────────────────────────────────────────────────────────────────────

-- kind is in the key: Orca RepositionLiquidityV2 emits remove + add in one ix.
CREATE TABLE liquidity_events (
    slot           BIGINT NOT NULL,
    signature      TEXT NOT NULL,
    ix_index       INT NOT NULL,
    is_inner       BOOLEAN NOT NULL DEFAULT FALSE,
    kind           TEXT NOT NULL CHECK (kind IN ('add', 'remove')),

    user_address   TEXT NOT NULL,
    protocol       TEXT NOT NULL,
    pool           TEXT NOT NULL,
    token_a_mint   TEXT NOT NULL,
    token_a_amount BIGINT NOT NULL,
    token_b_mint   TEXT NOT NULL,
    token_b_amount BIGINT NOT NULL,

    block_time     BIGINT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, is_inner, kind)
);

CREATE TABLE pool_events (
    slot         BIGINT NOT NULL,
    signature    TEXT NOT NULL,
    ix_index     INT NOT NULL,
    is_inner     BOOLEAN NOT NULL DEFAULT FALSE,

    protocol     TEXT NOT NULL,
    pool         TEXT NOT NULL,
    token_a_mint TEXT NOT NULL,
    token_b_mint TEXT NOT NULL,

    block_time   BIGINT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (slot, signature, ix_index, is_inner)
);

-- ─────────────────────────────────────────────────────────────────────────
-- Indexes
-- ─────────────────────────────────────────────────────────────────────────

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

-- ─────────────────────────────────────────────────────────────────────────
-- Writer dead letters
-- ─────────────────────────────────────────────────────────────────────────

CREATE TABLE dead_letter_batches (
    id             BIGSERIAL PRIMARY KEY,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    attempts       INT NOT NULL,
    row_count      INT NOT NULL,
    sqlstate       TEXT,
    error          TEXT NOT NULL,
    payload_format TEXT NOT NULL DEFAULT 'json-v1',
    payload        BYTEA NOT NULL,
    replayed_at    TIMESTAMPTZ,
    replay_error   TEXT
);

CREATE INDEX idx_dlq_created_at ON dead_letter_batches (created_at);
CREATE INDEX idx_dlq_pending ON dead_letter_batches (created_at)
    WHERE replayed_at IS NULL;
