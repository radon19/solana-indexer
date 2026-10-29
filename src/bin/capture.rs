//! Capture a Yellowstone gRPC stream to disk for offline replay.
//!
//!     cargo run --bin capture                    record 10s -> capture.bin
//!     cargo run --bin capture -- stats capture.bin
//!
//! Frame format: 4-byte little-endian length, then a prost-encoded
//! SubscribeUpdate. Self-delimiting, so a truncated tail costs one record
//! instead of the whole file.
//!
//! The subscription mirrors src/main.rs. Because `account_include` already
//! restricts the stream server-side, nothing is filtered client-side — every
//! update that arrives is worth keeping.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use yellowstone_grpc_client::{ClientTlsConfig, GeyserGrpcClient};
use yellowstone_grpc_proto::geyser::{
    SubscribeRequest, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocksMeta,
    SubscribeRequestFilterSlots, SubscribeRequestFilterTransactions, SubscribeUpdate,
    SubscribeRequestPing, subscribe_update::UpdateOneof,
};
use yellowstone_grpc_proto::prost::Message;

const DEFAULT_SECONDS: u64 = 10;
const DEFAULT_PATH: &str = "capture.bin";

// Mirrors crate::parsers::constants::all_addresses(). Duplicated rather than
// imported because this is a standalone binary, not part of the main crate.
const TRACKED: [&str; 20] = [
    "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
    "j1o2qRpjcyUwEvwtcfhEQefh773ZgjxcVRry7LDqg5X",
    "61DFfeTKM7trxYcPQCM78bJ794ddZprZpAwAnLiwTpYH",
    "2En5Y11SEAGLNmEezTuRUCwTyzyNReHaMbSnS5gjGsL1",
    "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
    "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA",
    "pfeecUb2rBoZQddahu8RPgtLKvmjSLKCJ3oWgDF347d",
    "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
    "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C",
    "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
    "LanMV9sAd7wArD4vJFi2qDdfnVhFxYSUg6eADduJ3uj",
    "routeUGWgWzqBWFcrCfv8tritsqukccJPu3q5GPP3xS",
    "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
    "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP",
    "DjVE6JNiYqPL2QXyCUUh8rNjHrbz9hXHNYt99MQ59qw1",
    "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",
    "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB",
    "cpamdpZCGKUy5JxQXB4dcpGPiikHawvSWAd6mEn1sGG",
    "dbcij3LWUppWqq96dh6gJWwBifmcGfLSB5D4DuSMaqN",
    "24Uqj9JCLxUeoC3hGfh5W3s9FM9uCHDS2SG3LYwBpyTi",
];

fn tracked() -> Vec<String> {
    TRACKED.iter().map(|s| s.to_string()).collect()
}

fn build_request() -> SubscribeRequest {
    SubscribeRequest {
        accounts: HashMap::from([(
            "tracked".to_string(),
            SubscribeRequestFilterAccounts {
                account: tracked(),
                ..Default::default()
            },
        )]),
        slots: HashMap::from([(
            "slots".to_string(),
            SubscribeRequestFilterSlots {
                filter_by_commitment: Some(false),
                interslot_updates: Some(true),
            },
        )]),
        transactions: HashMap::from([(
            "tracked".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                account_include: tracked(),
                ..Default::default()
            },
        )]),
        blocks_meta: HashMap::from([(
            "blocks_meta".to_string(),
            SubscribeRequestFilterBlocksMeta {},
        )]),
        commitment: None,
        ..Default::default()
    }
}

// ── record ───────────────────────────────────────────────────────────────────

async fn record(path: &str, seconds: u64) -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    let url = std::env::var("GEYSER_URL").expect("GEYSER_URL missing");
    let token = std::env::var("GEYSER_X_TOKEN").expect("GEYSER_X_TOKEN missing");

    let mut client = GeyserGrpcClient::build_from_shared(url.clone())
        .expect("Invalid URL")
        .tls_config(ClientTlsConfig::new().with_native_roots())
        .expect("tls config")
        .x_token(Some(token.clone()))
        .expect("x_token")
        .max_decoding_message_size(64 * 1024 * 1024)
        .connect()
        .await
        .map_err(|e| {
            std::io::Error::other(format!("connect failed: {e:?}"))
        })?;

    println!("Connected to {url}");
    println!("Recording {seconds}s -> {path}");

    let (mut sink, mut stream) = match client.subscribe().await {
        Ok(pair) => pair,
        Err(e) => {
            return Err(std::io::Error::other(format!("subscribe failed: {e:?}")));
        }
    };

    sink.send(build_request()).await.map_err(|e| {
        std::io::Error::other(format!("send request failed: {e:?}"))
    })?;

    let mut out = BufWriter::new(File::create(path)?);
    let started = Instant::now();

    let (mut written, mut bytes, mut pings, mut errors) = (0u64, 0u64, 0u64, 0u64);
    let (mut txns, mut slots, mut accounts, mut block_meta) = (0u64, 0u64, 0u64, 0u64);
    let mut slowest_slot = 0u64;

    while started.elapsed() < Duration::from_secs(seconds) {
        let remaining = Duration::from_secs(seconds) - started.elapsed();

        let next = tokio::time::timeout(remaining, stream.next()).await;

        match next {
            Ok(None) => {
                println!("\nStream closed by server after {}s", started.elapsed().as_secs());
                break;
            }
            Ok(Some(Ok(update))) => {
                if let Some(UpdateOneof::Ping(_)) = &update.update_oneof {
                    pings += 1;
                    let _ = sink
                        .send(SubscribeRequest {
                            ping: Some(SubscribeRequestPing { id: 1 }),
                            ..Default::default()
                        })
                        .await;
                    continue;
                }

                match &update.update_oneof {
                    Some(UpdateOneof::Transaction(t)) => {
                        txns += 1;
                        if let Some(slot) = Some(t.slot) {
                            slowest_slot = slowest_slot.max(slot);
                        }
                    }
                    Some(UpdateOneof::Slot(_)) => slots += 1,
                    Some(UpdateOneof::Account(_)) => accounts += 1,
                    Some(UpdateOneof::BlockMeta(_)) => block_meta += 1,
                    _ => {}
                }

                let buf = update.encode_to_vec();
                out.write_all(&(buf.len() as u32).to_le_bytes())?;
                out.write_all(&buf)?;

                written += 1;
                bytes += buf.len() as u64;

                if written % 500 == 0 {
                    println!(
                        "  {}s  tx={txns:<7} written={written:<7} {} MB",
                        started.elapsed().as_secs(),
                        bytes / 1_048_576
                    );
                }
            }
            Ok(Some(Err(e))) => {
                errors += 1;
                let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(&e);
                while let Some(c) = cause {
                    println!("  stream error: {c}");
                    cause = c.source();
                }
            }
            Err(_) => break,
        }
    }

    out.flush()?;

    println!("\nDone in {}s", started.elapsed().as_secs());
    println!("  transactions {txns}");
    println!("  slots        {slots}");
    println!("  accounts     {accounts}");
    println!("  block_meta   {block_meta}");
    println!("  pings        {pings}");
    println!("  errors       {errors}");
    println!("  written      {written} records, {} MB", bytes / 1_048_576);
    println!("  highest slot {slowest_slot}");

    Ok(())
}

// ── stats ────────────────────────────────────────────────────────────────────

fn read_all(path: &str) -> std::io::Result<(Vec<SubscribeUpdate>, u64)> {
    let mut r = BufReader::new(File::open(path)?);
    let mut out = Vec::new();
    let mut bytes = 0u64;

    loop {
        let mut len = [0u8; 4];
        if r.read_exact(&mut len).is_err() {
            break;
        }

        let n = u32::from_le_bytes(len) as usize;
        let mut buf = vec![0u8; n];
        if r.read_exact(&mut buf).is_err() {
            println!("  (truncated final frame, dropped)");
            break;
        }

        match SubscribeUpdate::decode(buf.as_slice()) {
            Ok(u) => {
                bytes += n as u64;
                out.push(u);
            }
            Err(e) => {
                println!("  (decode failed at record {}, stopped: {e})", out.len());
                break;
            }
        }
    }

    Ok((out, bytes))
}

fn stats(path: &str) -> std::io::Result<()> {
    let (updates, bytes) = read_all(path)?;

    let (mut txns, mut slots, mut accounts, mut block_meta, mut pings) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut ixs, mut inner_ixs, mut jup_touched, mut lowest, mut highest) =
        (0u64, 0u64, 0u64, u64::MAX, 0u64);

    for u in &updates {
        match &u.update_oneof {
            Some(UpdateOneof::Transaction(t)) => {
                txns += 1;
                highest = highest.max(t.slot);
                lowest = lowest.min(t.slot);

                let Some(info) = &t.transaction else { continue };
                let Some(raw) = &info.transaction else { continue };

                if let Some(msg) = &raw.message {
                    ixs += msg.instructions.len() as u64;
                    let keys: Vec<String> = msg
                        .account_keys
                        .iter()
                        .map(|k| bs58::encode(k).into_string())
                        .collect();

                    if keys.iter().any(|k| TRACKED.contains(&k.as_str())) {
                        jup_touched += 1;
                    }
                }

                if let Some(meta) = &info.meta {
                    inner_ixs += meta
                        .inner_instructions
                        .iter()
                        .map(|g| g.instructions.len() as u64)
                        .sum::<u64>();
                }
            }
            Some(UpdateOneof::Slot(_)) => slots += 1,
            Some(UpdateOneof::Account(_)) => accounts += 1,
            Some(UpdateOneof::BlockMeta(_)) => block_meta += 1,
            Some(UpdateOneof::Ping(_)) => pings += 1,
            _ => {}
        }
    }

    println!("{path}");
    println!("  file size     {} bytes", std::fs::metadata(path)?.len());
    println!("  records       {}", updates.len());
    println!("  decoded bytes {bytes}");
    println!();
    println!("  transactions  {txns}");
    println!("  slots         {slots}");
    println!("  accounts      {accounts}");
    println!("  block_meta    {block_meta}");
    println!("  pings         {pings}");
    println!();
    println!("  top-level instructions  {ixs}");
    println!("  inner instructions     {inner_ixs}");
    println!("  tracked-program txs    {jup_touched}");
    if txns > 0 {
        println!("  slot range             {lowest} .. {highest}");
    }

    Ok(())
}

// ── main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let seconds: u64 = std::env::var("CAPTURE_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SECONDS);

    let result = match args.get(1).map(String::as_str) {
        Some("stats") => stats(args.get(2).map(String::as_str).unwrap_or(DEFAULT_PATH)),
        _ => record(
            args.get(1).map(String::as_str).unwrap_or(DEFAULT_PATH),
            seconds,
        )
        .await,
    };

    if let Err(e) = result {
        eprintln!("\nfailed: {e}");
        std::process::exit(1);
    }
}
