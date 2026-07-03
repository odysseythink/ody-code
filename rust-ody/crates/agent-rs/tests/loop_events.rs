use agent_rs::agent_loop::events::{
    DefaultLoopEventDispatcher, LoopEvent, LoopEventDispatcher, LoopInterruptReason,
    LoopLiveOnlyEvent, LoopRecordedEvent,
};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn dispatcher_appends_recorded_and_emits_live() {
    let recorded: Arc<Mutex<Vec<LoopRecordedEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let live: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let r = recorded.clone();
    let l = live.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        move |event: LoopRecordedEvent| {
            let r = r.clone();
            async move {
                r.lock().unwrap().push(event);
                Ok(()) as Result<(), anyhow::Error>
            }
        },
        Some(Box::new(move |event| {
            if let LoopEvent::Live(LoopLiveOnlyEvent::TextDelta { delta }) = event {
                l.lock().unwrap().push(delta);
            }
        })),
    );

    dispatcher.dispatch_live(LoopLiveOnlyEvent::TextDelta {
        delta: "hello".into(),
    });
    dispatcher
        .dispatch_recorded(LoopRecordedEvent::StepBegin {
            uuid: "s1".into(),
            turn_id: "t1".into(),
            step: 1,
        })
        .await
        .unwrap();

    assert_eq!(live.lock().unwrap().as_slice(), &["hello"]);
    assert_eq!(recorded.lock().unwrap().len(), 1);
}

#[test]
fn live_event_serializes_like_ts() {
    let event = LoopLiveOnlyEvent::TurnInterrupted {
        reason: LoopInterruptReason::MaxSteps,
        attempted_steps: 3,
        active_step: Some(2),
        message: Some("too many".into()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"turn.interrupted\""), "{}", json);
    assert!(json.contains("\"attemptedSteps\":3"), "{}", json);
}
