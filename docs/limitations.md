# Limitations and Supported Scope

Solana Indexer focuses on decoding DEX activity through a modular Rust pipeline, with PostgreSQL persistence, CPI-aware event identity, finalized-block checkpoints, offline replay and graceful shutdown. The following boundaries describe its current implementation and the areas to consider when extending or deploying it.

## Protocol and event coverage

The router includes **10 protocol parsers** across Jupiter, Pump.fun / PumpSwap, Raydium, Orca and Meteora:

| Ecosystem | Implemented parser modules |
|---|---|
| Jupiter | Swap aggregator V6 |
| Pump.fun | Bonding curve, PumpSwap AMM |
| Raydium | CLMM, CPMM, AMM v4 |
| Orca | Whirlpool |
| Meteora | DLMM, DAMM v2, DBC |

Coverage is instruction-specific. Recognizing a program does not mean every instruction, historical layout or future upgrade is supported. Swap, liquidity, pool-creation and launch coverage also differs between parsers. Unsupported instructions generally produce no event, so **100% swap coverage is not claimed**.

Raydium LaunchLab, other AMM families, general SPL transfers, mint/burn activity, standalone fee events and graduation events are outside the currently routed event coverage. Newly added protocol versions need decoder updates and regression fixtures.

## Amounts and attribution

Several parsers use invocation-matched execution data. The following remaining distinctions matter when interpreting results:

| Area | Current boundary | Practical implication |
|---|---|---|
| Pump.fun amounts | Buy/sell branches still use maximum-cost or minimum-output arguments for some payment amounts. Exact-input buy branches also pass payment and token fields in reversed positions. | These branches require correction before their amounts support reliable pricing, volume totals or threshold alerts. |
| Jupiter amounts | Several route variants store quoted input/output amounts rather than execution-derived route totals. | Quote values must not be presented as verified executed amounts. |
| Jupiter route details | Hop program IDs, per-hop mints and executed amounts are not fully populated. | Route percentages and venue names are available, but a complete executed transfer breakdown is not. |
| Routed trades | An aggregator event and its individual venue events may describe the same economic trade. | Aggregations must select the intended level rather than sum both. |
| Quote tokens and fees | Payment-token identity and fee treatment are not represented uniformly across all protocols. Pump.fun trade records retain a SOL-named payment field even for quote-token variants. | Interpret amounts using the specific instruction and mint; a raw amount alone does not establish SOL value or net wallet spend. |
| Wallet attribution | User identity follows instruction accounts and available token-account ownership metadata. | Shared-account and nested-router paths need additional validation before treating every reported user as the final end-user wallet. |

Execution matching depends on complete instruction metadata and runtime traces. Parsers that require this evidence skip amount-bearing events when the match is missing, inconsistent, failed or ambiguous. A missing parsed event therefore does not prove that no on-chain activity occurred.

## Token metadata and timestamps

- Mint resolution primarily uses the transaction's token-balance metadata. Unresolved mints retain the explicit `N/A` placeholder; the indexer does not automatically fetch missing metadata through RPC.
- Token symbols come from a limited built-in mapping. Unknown tokens are displayed through local labels and full mint references. No live token registry, price feed or USD valuation is included.
- Human-readable amounts depend on available decimals. The formatter labels amounts as raw when scaling information is unavailable or outside its supported scaling range.
- Transaction and event `block_time` values are filled when their slot's block metadata provides a timestamp. They remain `NULL` while that source information is unavailable.
- Launch `timestamp_unix_ms` intentionally records **indexer time**, not the on-chain launch time. Reprocessing an event can produce a different in-memory indexer timestamp; existing stored rows retain their original value under the current conflict policy. Use `block_time` for chain-time analysis.

## Streaming and recovery

Live ingestion requests finalized, program-filtered block envelopes. This prioritizes finality over immediate processed-slot latency and relies on a provider that supports the requested block subscriptions.

Safe checkpoints are recorded with complete finalized block data in the same database transaction. **Loading a saved checkpoint at startup is intentionally disabled**, so each normal run begins live and establishes a new indexing span. A fresh run does not backfill the gap since the previous run.

Automatic reconnection and gap backfill are not implemented. Initial connection attempts retry, but an established stream interruption requires a restart. Enabling startup resume additionally requires checking the provider's historical replay retention. A saved checkpoint cannot recover history that the provider no longer retains.

Checkpoint scope follows endpoint and subscription policy. Changing those settings establishes a separate scope. Checkpoints cover selected transaction/event data; they do not represent complete Solana history, parser coverage or an atomic snapshot of every independent account update.

## Storage and shutdown behavior

The writer uses bounded queues, retry backoff, batched SQL statements and idempotent event keys. These are designed to preserve accepted work while storage catches up, with the following operational boundaries:

- Persistent database unavailability can keep the writer retrying and make graceful shutdown wait for storage recovery. Bounded channels propagate backpressure toward ingestion; they cannot guarantee that an upstream provider retains a stalled stream indefinitely.
- A permanently failing checkpointed batch is saved to the dead-letter queue and stops the writer. Later checkpoints are not allowed to advance past that failure. DLQ replay is an explicit operator action. Payload formats are versioned; unsupported older formats require compatible source data or an explicit migration.
- Ctrl+C and SIGTERM drain accepted work and finish writes. Forced termination cannot drain memory queues; recovery of uncommitted work depends on retained source data and an enabled recovery path.
- Account records store the latest accepted state, rather than an append-only account history. Raw instruction/log provenance is not retained comprehensively in PostgreSQL for later reparsing.
- Swap rows do not currently expose a dedicated pool address field. Per-pool swap analytics need a schema extension or additional source data.
- Default row-count and flush-interval settings are fixed in code. Deployment-specific capacity, retention, partitioning, database growth and tuning remain operational responsibilities.

## Offline replay

Replay supports repeatable parsing of recorded Yellowstone updates without consuming live-provider credits. It currently loads the capture into memory, so very large files need enough RAM.

Malformed or truncated captures can stop loading while still returning a partial successful result. Frame lengths are trusted before allocation, without a configured upper bound. Capture-file integrity checks and streaming reads remain improvements to make before relying on replay as a completeness guarantee or accepting arbitrary capture files.

Offline replay does not advance live resume checkpoints. It also cannot reconstruct transactions or metadata absent from the recording. The capture utility and live subscription have different data shapes; captures should be selected for the replay normalizer's supported update types.

## Deployment and validation

- A local test validator needs a compatible Geyser/Yellowstone service and client configuration. The live client reads `GEYSER_URL`; `GEYSER_LOCAL_URL` is not selected automatically. Local TLS/authentication settings must match the client setup.
- Some configuration, connection and malformed-update paths still use `expect` or `panic` rather than structured errors. Legacy transaction normalization also converts invalid token-amount strings to zero; the finalized live-block path performs additional validation. Stream message decoding is currently limited to 64 MiB per message.
- Unit fixtures, capture regressions and PostgreSQL tests validate the exercised paths. They do not establish sustained mainnet throughput, exhaustive protocol correctness, provider compatibility or an availability SLA.
- Some acceptance checks remain advisory and do not detect every unresolved value. In particular, empty-string mint checks do not flag `N/A` as unresolved metadata; a passing suite is not proof of complete mint or amount coverage.
- CI automation, dedicated operational metrics, health endpoints and alert delivery are not implemented. The pipeline is currently a single-process indexer, without built-in distributed coordination or high-availability failover.

## Extension priorities

The most valuable next steps are execution-derived Pump.fun/Jupiter amounts, richer mint and route attribution, supervised reconnect/backfill, stricter capture validation and deployment monitoring. Email or other large-swap alerts can build on these foundations once the relevant amount paths are verified.
