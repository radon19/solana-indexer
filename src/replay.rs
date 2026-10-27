//! Offline replay of a Yellowstone capture.
//!
//! Reads a file written by `cargo run --bin capture`, pushes every update
//! through the same normalization the live path uses, and hands it to the
//! same consumer. No gRPC connection is opened, so replay costs no provider
//! quota and returns identical results every run.
//!
//!     cargo run
//!
//! Environment:
//!     REPLAY       path to the capture file (default capture.bin)
//!     REPLAY_LOOP  set to any value to repeat the file forever

use std::fs::File;
use std::io::{BufReader, Read};

use tokio::sync::mpsc;

use yellowstone_grpc_proto::{
    geyser::{SubscribeUpdate, subscribe_update::UpdateOneof},
    prost::Message,
};

use crate::{
    helpers,
    normalization::structs::Ingester,
    pipeline::processer::process,
    store::writer::Sender,
};

const DEFAULT_PATH: &str = "capture.bin";

/// Read every frame from a capture file.
///
/// Frame layout written by src/bin/capture.rs: 4-byte little-endian length,
/// then a prost-encoded SubscribeUpdate. Self-delimiting, so a truncated
/// tail costs one record instead of the whole file.
pub fn load(path: &str) -> std::io::Result<Vec<SubscribeUpdate>> {
    let mut r = BufReader::new(File::open(path)?);
    let mut out = Vec::new();

    loop {
        let mut len = [0u8; 4];
        if r.read_exact(&mut len).is_err() {
            break;
        }

        let mut buf = vec![0u8; u32::from_le_bytes(len) as usize];
        if r.read_exact(&mut buf).is_err() {
            eprintln!("  (truncated final frame, dropped)");
            break;
        }

        match SubscribeUpdate::decode(buf.as_slice()) {
            Ok(u) => out.push(u),
            Err(e) => {
                eprintln!("  (decode failed at record {}, stopping: {e})", out.len());
                break;
            }
        }
    }

    Ok(out)
}

/// Normalize one update. Mirrors the live path in pipeline::ingester.
fn normalize(update: SubscribeUpdate) -> Option<Ingester> {
    Some(match update.update_oneof? {
        UpdateOneof::Slot(slot) => helpers::slot_helper::get_slot(slot),
        UpdateOneof::Account(acc) => helpers::account_helper::get_account(acc),
        UpdateOneof::BlockMeta(meta) => helpers::block_meta_helper::get_block_meta(meta),
        UpdateOneof::Transaction(txn) => helpers::txn_helper::get_txn(txn),
        _ => return None,
    })
}

/// Replay the capture into the normal pipeline.
///
/// The consumer is spawned once and lives as long as the sender does, so
/// looping the file does not restart it — state carries across passes the
/// same way it would on a live stream.
pub async fn run(writer: Sender) {
    let path = std::env::var("REPLAY").unwrap_or_else(|_| DEFAULT_PATH.to_string());
    let loop_forever = std::env::var("REPLAY_LOOP").is_ok();

    let updates = match load(&path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("REPLAY FAILED — cannot read {path}: {e}");
            eprintln!("record one first:  cargo run --bin capture");
            return;
        }
    };

    let (tx, rx) = mpsc::channel(20);
    let consumer = tokio::spawn(process(rx, Some(writer)));

    let mut pass = 0u32;

    loop {
        pass += 1;
        let started = std::time::Instant::now();
        let mut sent = 0u64;

        eprintln!("REPLAY pass {pass}: {} updates from {path}", updates.len());

        for update in &updates {
            let Some(data) = normalize(update.clone()) else {
                continue;
            };

            if tx.send(data).await.is_err() {
                eprintln!("consumer dropped, stopping");
                return;
            }

            sent += 1;
        }

        eprintln!(
            "REPLAY pass {pass} done: {sent} updates in {:.2}s",
            started.elapsed().as_secs_f64()
        );

        if !loop_forever {
            break;
        }
    }

    // closing the sender lets the consumer finish its range and exit
    drop(tx);
    let _ = consumer.await;

    eprintln!("REPLAY complete — {pass} pass(es)");
}
