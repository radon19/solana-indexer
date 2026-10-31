//! Stop the producer, then let closed channels drain the remaining stages.

use std::{future::Future, io, time::Duration};
use tokio::{
    sync::watch,
    task::{JoinError, JoinHandle},
};

pub(crate) type StageResult = io::Result<()>;

pub(crate) struct Signals {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
}

impl Signals {
    /// Register before connecting so Ctrl+C also works during startup.
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            #[cfg(unix)]
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?,
            #[cfg(unix)]
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?,
        })
    }

    pub(crate) async fn recv(&mut self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let received = tokio::select! {
                received = self.interrupt.recv() => received,
                received = self.terminate.recv() => received,
            };
            received.ok_or_else(|| io::Error::other("shutdown signal listener closed"))
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await
        }
    }
}

pub(crate) async fn startup<T>(
    step: impl Future<Output = io::Result<T>>,
    signals: &mut Signals,
) -> io::Result<Option<T>> {
    tokio::select! {
        biased;
        result = signals.recv() => {
            result?;
            eprintln!("Shutdown requested during startup; no pipeline to drain.");
            Ok(None)
        }
        result = step => result.map(Some),
    }
}

pub(crate) async fn cancelled(stop: &mut watch::Receiver<bool>) {
    loop {
        if *stop.borrow_and_update() {
            return;
        }
        if stop.changed().await.is_err() {
            return;
        }
    }
}

fn record_result(stage: &str, result: Result<StageResult, JoinError>, errors: &mut Vec<String>) {
    let error = match result {
        Ok(Ok(())) => return,
        Ok(Err(error)) => format!("{stage}: {error}"),
        Err(error) => format!("{stage} task: {error}"),
    };
    eprintln!("{error}");
    errors.push(error);
}

/// Join all three workers even on errors. Never cancel an in-progress DB flush.
/// Like join!, this awaits every handle; select! also watches signals and failures.
pub(crate) async fn supervise(
    mut producer: JoinHandle<StageResult>,
    mut consumer: JoinHandle<StageResult>,
    mut writer: JoinHandle<StageResult>,
    signal: impl Future<Output = io::Result<()>>,
    stop: watch::Sender<bool>,
) -> io::Result<()> {
    tokio::pin!(signal);
    let (mut producer_done, mut consumer_done, mut writer_done) = (false, false, false);
    let mut stopping = false;
    let mut errors = Vec::new();
    let mut progress = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_secs(5),
        Duration::from_secs(5),
    );

    while !(producer_done && consumer_done && writer_done) {
        tokio::select! {
            biased;
            result = &mut signal, if !stopping => {
                if let Err(error) = result { errors.push(format!("shutdown signal: {error}")); }
                eprintln!("Shutdown requested; stopping input and draining queued updates...");
            }
            result = &mut producer, if !producer_done => {
                producer_done = true;
                record_result("producer", result, &mut errors);
            }
            result = &mut consumer, if !consumer_done => {
                consumer_done = true;
                record_result("consumer", result, &mut errors);
            }
            result = &mut writer, if !writer_done => {
                writer_done = true;
                record_result("writer", result, &mut errors);
            }
            _ = progress.tick(), if stopping => {
                eprintln!("Shutdown: waiting for queued processing and database flush; no writes aborted.");
            }
        }
        // EOF or any worker failure also shuts down the source. The processor
        // and writer receive no cancellation: they finish their channel queues.
        if !stopping {
            stopping = true;
            stop.send_replace(true);
        }
    }

    if errors.is_empty() {
        eprintln!("Shutdown complete: producer, consumer, and writer have finished.");
        Ok(())
    } else {
        Err(io::Error::other(errors.join("; ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::{Mutex, Notify, mpsc};

    #[tokio::test]
    async fn signal_joins_workers_after_their_queues_drain() {
        let (stop, mut stop_rx) = watch::channel(false);
        let (tx, mut rx) = mpsc::channel(1);
        let (writes, mut write_rx) = mpsc::channel(1);
        let saved = Arc::new(Mutex::new(Vec::new()));
        let saved_by_writer = saved.clone();
        let producer = tokio::spawn(async move {
            cancelled(&mut stop_rx).await;
            tx.send(42).await.unwrap(); // An update accepted before shutdown.
            Ok(())
        });
        let consumer = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                writes.send(message).await.unwrap();
            }
            Ok(())
        });
        let writer = tokio::spawn(async move {
            while let Some(message) = write_rx.recv().await {
                saved_by_writer.lock().await.push(message);
            }
            Ok(())
        });
        supervise(producer, consumer, writer, async { Ok(()) }, stop)
            .await
            .unwrap();
        assert_eq!(*saved.lock().await, vec![42]);
    }

    #[tokio::test]
    async fn natural_eof_joins_all_workers_without_waiting_for_a_signal() {
        let (stop, _stop_rx) = watch::channel(false);
        let producer = tokio::spawn(async { Ok(()) });
        let consumer = tokio::spawn(async { Ok(()) });
        let writer = tokio::spawn(async { Ok(()) });
        tokio::time::timeout(
            Duration::from_secs(1),
            supervise(producer, consumer, writer, std::future::pending(), stop),
        )
        .await
        .unwrap()
        .unwrap();
    }

    #[tokio::test]
    async fn writer_panic_stops_idle_producer_and_is_reported() {
        let (stop, mut stop_rx) = watch::channel(false);
        let producer = tokio::spawn(async move {
            cancelled(&mut stop_rx).await;
            Ok(())
        });
        let consumer = tokio::spawn(async { Ok(()) });
        let writer = tokio::spawn(async { panic!("writer failed") });
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            supervise(producer, consumer, writer, std::future::pending(), stop),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("writer task"));
    }

    #[tokio::test]
    async fn source_error_still_joins_downstream_workers() {
        let (stop, _stop_rx) = watch::channel(false);
        let producer = tokio::spawn(async { Err(io::Error::other("source failed")) });
        let finished = Arc::new(Notify::new());
        let consumer_finished = finished.clone();
        let consumer = tokio::spawn(async move {
            consumer_finished.notify_one();
            Ok(())
        });
        let writer = tokio::spawn(async move {
            finished.notified().await;
            Ok(())
        });
        let error = supervise(producer, consumer, writer, std::future::pending(), stop)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("producer: source failed"));
    }

    #[tokio::test]
    async fn shutdown_waits_for_storage_instead_of_aborting_it() {
        let (stop, _stop_rx) = watch::channel(false);
        let release = Arc::new(Notify::new());
        let writer_release = release.clone();
        let producer = tokio::spawn(async { Ok(()) });
        let consumer = tokio::spawn(async { Ok(()) });
        let writer = tokio::spawn(async move {
            writer_release.notified().await;
            Ok(())
        });
        let mut supervisor = tokio::spawn(supervise(
            producer,
            consumer,
            writer,
            async { Ok(()) },
            stop,
        ));
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut supervisor)
                .await
                .is_err()
        );
        release.notify_one();
        tokio::time::timeout(Duration::from_secs(1), supervisor)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn signal_listener_error_stops_workers_and_returns_failure() {
        let (stop, mut stop_rx) = watch::channel(false);
        let producer = tokio::spawn(async move {
            cancelled(&mut stop_rx).await;
            Ok(())
        });
        let consumer = tokio::spawn(async { Ok(()) });
        let writer = tokio::spawn(async { Ok(()) });
        let error = supervise(
            producer,
            consumer,
            writer,
            async { Err(io::Error::other("listener failed")) },
            stop,
        )
        .await
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("shutdown signal: listener failed")
        );
    }
}
