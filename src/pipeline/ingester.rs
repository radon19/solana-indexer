use futures::{Stream, StreamExt};
use std::io;
use tokio::sync::{mpsc, watch};
use yellowstone_grpc_proto::{
    geyser::{SubscribeUpdate, subscribe_update::UpdateOneof},
    tonic::Status,
};

use super::shutdown;
use crate::{helpers, normalization::structs::Ingester};

pub(crate) fn normalize(update: SubscribeUpdate) -> Option<Ingester> {
    Some(match update.update_oneof? {
        UpdateOneof::Slot(slot) => helpers::slot_helper::get_slot(slot),
        UpdateOneof::Account(account) => helpers::account_helper::get_account(account),
        UpdateOneof::BlockMeta(block) => helpers::block_meta_helper::get_block_meta(block),
        UpdateOneof::Transaction(transaction) => helpers::txn_helper::get_txn(transaction),
        _ => return None,
    })
}

pub async fn ingest(
    stream: impl Stream<Item = Result<SubscribeUpdate, Status>>,
    tx: mpsc::Sender<Ingester>,
    stop: watch::Receiver<bool>,
) -> io::Result<()> {
    ingest_inner(stream, tx, stop, None).await
}

/// Full block envelopes are trusted only on the live finalized subscription.
pub async fn ingest_finalized(
    stream: impl Stream<Item = Result<SubscribeUpdate, Status>>,
    tx: mpsc::Sender<Ingester>,
    stop: watch::Receiver<bool>,
    tracker: crate::resume::Tracker,
) -> io::Result<()> {
    ingest_inner(stream, tx, stop, Some(tracker)).await
}

async fn ingest_inner(
    stream: impl Stream<Item = Result<SubscribeUpdate, Status>>,
    tx: mpsc::Sender<Ingester>,
    mut stop: watch::Receiver<bool>,
    mut tracker: Option<crate::resume::Tracker>,
) -> io::Result<()> {
    tokio::pin!(stream);
    loop {
        let message = tokio::select! {
            biased;
            _ = shutdown::cancelled(&mut stop) => break,
            message = stream.next() => message,
        };
        let Some(message) = message else { break };
        let update = message.map_err(io::Error::other)?;
        let data = if let Some(tracker) = tracker.as_mut() {
            match update.update_oneof {
                Some(UpdateOneof::Block(block)) => {
                    tracker.accept(block)?.map(Ingester::FinalizedSlot)
                }
                Some(UpdateOneof::Account(account)) => {
                    Some(helpers::account_helper::get_account(account))
                }
                // Finalized block envelopes own transaction and metadata writes.
                _ => None,
            }
        } else {
            normalize(update)
        };
        let Some(data) = data else {
            continue;
        };

        // Once accepted, finish handing off this update even after Ctrl+C.
        // Cancelling this send when the queue is full would discard that update.
        tx.send(data).await.map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "processor input channel closed")
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use yellowstone_grpc_proto::geyser::SubscribeUpdateSlot;

    fn slot(number: u64) -> SubscribeUpdate {
        SubscribeUpdate {
            update_oneof: Some(UpdateOneof::Slot(SubscribeUpdateSlot {
                slot: number,
                ..Default::default()
            })),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn finalized_notification_alone_cannot_create_a_checkpoint() {
        let (tx, mut rx) = mpsc::channel(1);
        let (_stop, stop_rx) = watch::channel(false);
        ingest_finalized(
            futures::stream::iter(vec![Ok(slot(10))]),
            tx,
            stop_rx,
            crate::resume::Tracker::new("test".into(), None),
        )
        .await
        .unwrap();
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn shutdown_preserves_an_accepted_update_waiting_on_a_full_queue() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.send(normalize(slot(1)).unwrap()).await.unwrap();
        let accepted = std::sync::Arc::new(tokio::sync::Notify::new());
        let notification = accepted.clone();
        let stream =
            futures::stream::iter(vec![Ok(slot(2))]).inspect(move |_| notification.notify_one());
        let (stop, stop_rx) = watch::channel(false);
        let producer = tokio::spawn(ingest(stream, tx, stop_rx));
        accepted.notified().await;
        stop.send_replace(true);
        let Some(Ingester::Slot(first)) = rx.recv().await else {
            panic!("expected first slot")
        };
        assert_eq!(first.slot, 1);
        producer.await.unwrap().unwrap();
        let Some(Ingester::Slot(second)) = rx.recv().await else {
            panic!("accepted update was lost")
        };
        assert_eq!(second.slot, 2);
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn shutdown_stops_an_idle_stream() {
        let (tx, mut rx) = mpsc::channel(1);
        let (stop, stop_rx) = watch::channel(false);
        let producer = tokio::spawn(ingest(futures::stream::pending(), tx, stop_rx));
        stop.send_replace(true);
        tokio::time::timeout(std::time::Duration::from_secs(1), producer)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn already_requested_shutdown_does_not_accept_another_update() {
        let (tx, mut rx) = mpsc::channel(1);
        let (_stop, stop_rx) = watch::channel(true);
        ingest(futures::stream::iter(vec![Ok(slot(1))]), tx, stop_rx)
            .await
            .unwrap();
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn stream_error_stops_instead_of_silently_accepting_later_updates() {
        let stream = futures::stream::iter(vec![
            Err(Status::unavailable("connection lost")),
            Ok(SubscribeUpdate {
                update_oneof: Some(UpdateOneof::Slot(SubscribeUpdateSlot {
                    slot: 10,
                    ..Default::default()
                })),
                ..Default::default()
            }),
        ]);
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        let (_stop, stop_rx) = watch::channel(false);
        assert!(ingest(stream, tx, stop_rx).await.is_err());
        assert!(
            rx.recv().await.is_none(),
            "updates after a fatal stream error must not be accepted"
        );
    }
}
