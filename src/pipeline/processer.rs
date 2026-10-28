use crate::{
    normalization::structs::Ingester,
    pipeline::{formatter::render_tx, router::route},
    store::{batch_schema::WriteMsg, writer::Sender},
};

pub async fn process(mut rx: tokio::sync::mpsc::Receiver<Ingester>, writer: Option<Sender>) {
    // stdout rendering is typically more expensive than the channel send.
    // PRINT_EVENTS=1 keeps the old dump for debugging.
    let print_events = std::env::var_os("PRINT_EVENTS").is_some();

    while let Some(data) = rx.recv().await {
        match data {
            Ingester::Slot(slot) => {
                if let Some(w) = &writer {
                    let _ = w.send(WriteMsg::Slot(slot)).await;
                }
            }

            Ingester::Transaction(txn) => {
                let events = route(&txn);

                if print_events && !events.is_empty() {
                    println!("{}", render_tx(&txn, &events));
                }

                if let Some(w) = &writer {
                    let _ = w.send(WriteMsg::transaction(txn, events)).await;
                }
            }

            Ingester::Account(account) => {
                if let Some(w) = &writer {
                    let _ = w.send(WriteMsg::Account(account)).await;
                }
            }

            Ingester::BlockMeta(block) => {
                if let Some(w) = &writer {
                    let _ = w.send(WriteMsg::BlockMeta(block)).await;
                }
            }
        }
    }
}
