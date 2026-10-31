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

use tokio::sync::{mpsc, watch};

use yellowstone_grpc_proto::{geyser::SubscribeUpdate, prost::Message};

use crate::{normalization::structs::Ingester, pipeline::ingester::normalize};

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

/// Replay is a producer just like live ingestion; main supervises all workers.
pub async fn produce(
    tx: mpsc::Sender<Ingester>,
    stop: watch::Receiver<bool>,
) -> std::io::Result<()> {
    let path = std::env::var("REPLAY").unwrap_or_else(|_| DEFAULT_PATH.to_string());
    let loop_forever = std::env::var("REPLAY_LOOP").is_ok();
    let load_path = path.clone();
    let updates = tokio::task::spawn_blocking(move || load(&load_path))
        .await
        .map_err(std::io::Error::other)?
        .map_err(|error| {
            eprintln!("REPLAY FAILED — cannot read {path}: {error}");
            error
        })?;
    let mut pass = 0u32;
    let mut total_sent = 0u64;

    'passes: loop {
        if *stop.borrow() {
            break;
        }
        pass = pass.saturating_add(1);
        let started = std::time::Instant::now();
        let mut sent = 0u64;
        eprintln!("REPLAY pass {pass}: {} updates from {path}", updates.len());

        for update in &updates {
            if *stop.borrow() {
                break 'passes;
            }
            let Some(data) = normalize(update.clone()) else {
                continue;
            };
            // Finish this accepted update even when shutdown arrives mid-send.
            tx.send(data).await.map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "replay processor input channel closed",
                )
            })?;
            sent += 1;
            total_sent += 1;
        }
        eprintln!(
            "REPLAY pass {pass} done: {sent} updates in {:.2}s",
            started.elapsed().as_secs_f64()
        );
        if !loop_forever {
            break;
        }
        // An empty looping capture must not prevent the supervisor seeing signals.
        tokio::task::yield_now().await;
    }

    if *stop.borrow() {
        eprintln!("REPLAY stopped: {total_sent} updates accepted; draining queued work.");
    } else {
        eprintln!("REPLAY complete — {pass} pass(es)");
    }
    Ok(())
}
