use crate::{
    normalization::structs::Ingester,
    pipeline::{formatter::render_tx, router::route},
    store::{batch_schema::WriteMsg, writer::Sender},
};
use std::io;

pub async fn process(
    mut rx: tokio::sync::mpsc::Receiver<Ingester>,
    writer: Option<Sender>,
) -> io::Result<()> {
    let print_events = std::env::var_os("PRINT_EVENTS").is_some();

    // Closing input stops new arrivals, but recv() still drains buffered updates.
    while let Some(data) = rx.recv().await {
        let message = match data {
            Ingester::FinalizedSlot(complete) => {
                let transactions = complete
                    .transactions
                    .into_iter()
                    .map(|txn| {
                        let events = route(&txn);
                        if print_events && !events.is_empty() {
                            println!("{}", render_tx(&txn, &events));
                        }
                        WriteMsg::transaction(txn, events)
                    })
                    .collect();
                WriteMsg::FinalizedSlot {
                    block: complete.block,
                    transactions,
                    checkpoint: complete.checkpoint,
                }
            }
            Ingester::Slot(slot) => WriteMsg::Slot(slot),
            Ingester::Account(account) => WriteMsg::Account(account),
            Ingester::BlockMeta(block) => WriteMsg::BlockMeta(block),
            Ingester::Transaction(txn) => {
                let events = route(&txn);
                if print_events && !events.is_empty() {
                    println!("{}", render_tx(&txn, &events));
                }
                WriteMsg::transaction(txn, events)
            }
        };
        if let Some(writer) = &writer {
            writer.send(message).await.map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "database writer channel closed")
            })?;
        }
    }
    // Returning drops the last writer sender so storage can drain and flush.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalization::structs::{Slot, Status};

    #[tokio::test]
    async fn closed_writer_stops_processor_and_closes_input() {
        let (writer, writer_rx) = tokio::sync::mpsc::channel(1);
        drop(writer_rx);
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let mut task = tokio::spawn(process(rx, Some(writer)));
        tx.send(Ingester::Slot(Slot {
            slot: 1,
            parent: None,
            status: Status::Processed,
        }))
        .await
        .unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_millis(200), &mut task).await;
        if result.is_err() {
            task.abort();
        }
        assert!(
            result.is_ok(),
            "processor must report a closed writer instead of waiting for more input"
        );
        assert!(tx.is_closed());
    }

    #[tokio::test]
    async fn closing_input_drains_every_buffered_message_to_writer() {
        let (writer, mut writes) = tokio::sync::mpsc::channel(1);
        let (tx, rx) = tokio::sync::mpsc::channel(3);
        for slot in 1..=3 {
            tx.send(Ingester::Slot(Slot {
                slot,
                parent: None,
                status: Status::Processed,
            }))
            .await
            .unwrap();
        }
        drop(tx);
        let task = tokio::spawn(process(rx, Some(writer)));
        let mut saved = Vec::new();
        while let Some(message) = writes.recv().await {
            let WriteMsg::Slot(slot) = message else {
                panic!("expected slot")
            };
            saved.push(slot.slot);
        }
        task.await.unwrap().unwrap();
        assert_eq!(saved, vec![1, 2, 3]);
    }
}
