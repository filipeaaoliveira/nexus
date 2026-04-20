use std::time::Duration;

use nexus::testing::{AgentEvent, AgentId, AgentTool};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Verifies that events buffered before cancellation are processed,
/// and cancellation stops the loop promptly.
#[tokio::test]
async fn cancellation_drains_pending_events_then_stops() {
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(32);
    let token = CancellationToken::new();
    let mut received = Vec::new();

    for i in 1..=5u32 {
        tx.send(AgentEvent::Discovered {
            id: AgentId::new(AgentTool::Claude, i),
            pid: Some(i),
            cwd: PathBuf::from(format!("/tmp/{i}")),
            started_at: None,
        })
        .await
        .unwrap();
    }

    let cancel_token = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_token.cancel();
    });

    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            maybe_event = rx.recv() => {
                match maybe_event {
                    Some(event) => received.push(event),
                    None => break,
                }
            }
        }
    }

    assert_eq!(received.len(), 5, "expected all buffered events to be drained");
    for (i, event) in received.iter().enumerate() {
        match event {
            AgentEvent::Discovered { pid, .. } => {
                assert_eq!(*pid, Some((i + 1) as u32));
            }
            other => panic!("unexpected event type: {other:?}"),
        }
    }
}

/// Verifies that dropping the sender causes the loop to exit cleanly.
#[tokio::test]
async fn sender_drop_exits_loop_cleanly() {
    let (tx, mut rx) = mpsc::channel::<AgentEvent>(8);
    let token = CancellationToken::new();

    tx.send(AgentEvent::Discovered {
        id: AgentId::new(AgentTool::Claude, 1),
        pid: Some(1),
        cwd: PathBuf::from("/tmp/1"),
        started_at: None,
    })
    .await
    .unwrap();

    drop(tx);

    let mut count = 0usize;
    let result = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            tokio::select! {
                biased;
                _ = token.cancelled() => break "cancelled",
                maybe_event = rx.recv() => {
                    match maybe_event {
                        Some(_) => count += 1,
                        None => break "channel_closed",
                    }
                }
            }
        }
    })
    .await;

    assert_eq!(result, Ok("channel_closed"));
    assert_eq!(count, 1);
}

/// Verifies that events and effects interleave correctly
/// and cancellation stops both paths.
#[tokio::test]
async fn concurrent_events_and_effects_interleave_then_cancel() {
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(32);
    let (effect_tx, mut effect_rx) = mpsc::channel::<String>(32);
    let token = CancellationToken::new();

    for i in 1..=3u32 {
        event_tx
            .send(AgentEvent::Discovered {
                id: AgentId::new(AgentTool::Codex, i),
                pid: Some(i),
                cwd: PathBuf::from(format!("/tmp/{i}")),
                started_at: None,
            })
            .await
            .unwrap();
        effect_tx.send(format!("effect-{i}")).await.unwrap();
    }

    let cancel_token = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_token.cancel();
    });

    let mut events = 0usize;
    let mut effects = 0usize;

    loop {
        tokio::select! {
            biased;
            _ = token.cancelled() => break,
            maybe = event_rx.recv() => {
                if maybe.is_some() { events += 1; } else { break; }
            }
            maybe = effect_rx.recv() => {
                if maybe.is_some() { effects += 1; } else { break; }
            }
        }
    }

    assert_eq!(events + effects, 6, "expected all 6 items processed, got events={events} effects={effects}");
}
