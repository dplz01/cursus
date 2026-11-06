//! A whole call through the real SimpleX chat core: two in-process sessions
//! (alice, bob) become contacts over the public SMP network, then alice calls
//! bob. Exercises exactly the commands and event fields the app uses
//! (`call_flow.rs`): invite → callInvitation → offer → callOffer → answer →
//! callAnswer → extras → connected, audio both ways, end → callEnded.
//!
//! Needs libsimplex and network: `cargo test -p cursus-linux simplex_e2e -- --ignored --nocapture`

use std::sync::Arc;
use std::time::Duration;

use cursus_core::{default_libs_dir, CallMedia, Event, Session, SessionConfig};
use serde_json::Value;
use tokio::sync::mpsc;

use super::tests::{rms, tone_packets};
use super::*;

fn open(root: &std::path::Path, name: &str) -> Arc<Session> {
    let cfg = SessionConfig {
        libs_dir: default_libs_dir(),
        db_prefix: root.join(name).join("simplex_v1"),
        db_key: zeroize::Zeroizing::new(format!("{name}-key")),
        migration_confirm: "yesUp".into(),
        initial_display_name: name.into(),
        encrypt_local_files: true,
    };
    std::fs::create_dir_all(root.join(name)).unwrap();
    let mut s = Session::open(cfg).expect("open");
    s.start_chat().expect("start");
    Arc::new(s)
}

/// Wait for a core event; `want` returns Some(T) to stop.
async fn wait_for<T>(
    rx: &mut mpsc::UnboundedReceiver<Event>,
    secs: u64,
    what: &str,
    mut want: impl FnMut(&Event) -> Option<T>,
) -> T {
    let fut = async {
        while let Some(ev) = rx.recv().await {
            if let Some(t) = want(&ev) {
                return Some(t);
            }
        }
        None
    };
    tokio::time::timeout(Duration::from_secs(secs), fut)
        .await
        .unwrap_or_else(|_| panic!("timeout waiting for {what}"))
        .unwrap_or_else(|| panic!("events closed waiting for {what}"))
}

fn other<'a>(ev: &'a Event, ty: &str) -> Option<&'a Value> {
    match ev {
        Event::Other { type_name, body } if type_name == ty => Some(body),
        _ => None,
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

type Peers = (
    std::path::PathBuf,
    Arc<Session>,
    Arc<Session>,
    mpsc::UnboundedReceiver<Event>,
    mpsc::UnboundedReceiver<Event>,
    i64,
    i64,
);

/// Two fresh profiles that are contacts of each other.
async fn setup(tag: &str) -> Peers {
    let root = std::env::temp_dir().join(format!("cursus-call-e2e-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // Keep the test's tmp/files away from the real profile.
    // SAFETY: run this test on its own (it's ignored by default).
    unsafe { std::env::set_var("XDG_DATA_HOME", root.join("xdg")) };
    cursus_core::init_process_env();

    let alice = blocking({ let r = root.clone(); move || open(&r, "alice") }).await;
    let bob = blocking({ let r = root.clone(); move || open(&r, "bob") }).await;
    let mut ea = alice.spawn_receiver();
    let mut eb = bob.spawn_receiver();
    blocking({ let a = alice.clone(); move || a.activate_network() }).await.unwrap();
    blocking({ let b = bob.clone(); move || b.activate_network() }).await.unwrap();

    // Become contacts.
    let link = blocking({ let a = alice.clone(); move || a.create_invitation_link() }).await.unwrap();
    blocking({ let b = bob.clone(); move || b.connect_link(&link.full) }).await.unwrap();
    let contact_id = |ev: &Event| match ev {
        Event::ContactConnected(v) => v.pointer("/contact/contactId").and_then(|i| i.as_i64()),
        _ => None,
    };
    let bob_in_alice = wait_for(&mut ea, 120, "alice: contactConnected", contact_id).await;
    let alice_in_bob = wait_for(&mut eb, 120, "bob: contactConnected", contact_id).await;
    eprintln!("contacts: bob@alice={bob_in_alice} alice@bob={alice_in_bob}");

    (root, alice, bob, ea, eb, bob_in_alice, alice_in_bob)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs libsimplex + network"]
async fn simplex_e2e_call() {
    let (root, alice, bob, mut ea, mut eb, bob_in_alice, alice_in_bob) = setup("call").await;

    // Alice calls.
    blocking({ let a = alice.clone(); move || a.call_invite(bob_in_alice, CallMedia::Audio) })
        .await
        .expect("invite");
    let inv = wait_for(&mut eb, 60, "bob: callInvitation", |ev| other(ev, "callInvitation").cloned()).await;
    let inv = inv.get("callInvitation").expect("callInvitation.callInvitation");
    assert_eq!(inv.pointer("/contact/contactId").and_then(|v| v.as_i64()), Some(alice_in_bob));
    assert_eq!(inv.pointer("/callType/media").and_then(|v| v.as_str()), Some("audio"));
    let bob_key = inv.get("sharedKey").and_then(|k| k.as_str()).map(str::to_string);
    assert!(bob_key.is_some(), "invitation must carry a shared key (encryption)");

    // Bob accepts: offer.
    let (bob_engine, mut bob_ev) = CallEngine::with_devices(bob_key.as_deref(), false).await.unwrap();
    let offer = bob_engine.create_offer().await.unwrap();
    blocking({
        let b = bob.clone();
        move || b.call_offer(alice_in_bob, CallMedia::Audio, true, &offer.rtc_session, &offer.rtc_ice_candidates)
    })
    .await
    .expect("offer");

    let off = wait_for(&mut ea, 60, "alice: callOffer", |ev| other(ev, "callOffer").cloned()).await;
    let alice_key = off.get("sharedKey").and_then(|k| k.as_str()).map(str::to_string);
    assert_eq!(alice_key, bob_key, "both sides derive the same key");
    let (alice_engine, mut alice_ev) = CallEngine::with_devices(alice_key.as_deref(), false).await.unwrap();
    let answer = alice_engine
        .accept_offer(
            off.pointer("/offer/rtcSession").and_then(|v| v.as_str()).expect("offer.rtcSession"),
            off.pointer("/offer/rtcIceCandidates").and_then(|v| v.as_str()).expect("offer.rtcIceCandidates"),
        )
        .await
        .unwrap();
    blocking({
        let a = alice.clone();
        move || a.call_answer(bob_in_alice, &answer.rtc_session, &answer.rtc_ice_candidates)
    })
    .await
    .expect("answer");

    let ans = wait_for(&mut eb, 60, "bob: callAnswer", |ev| other(ev, "callAnswer").cloned()).await;
    bob_engine
        .apply_answer(
            ans.pointer("/answer/rtcSession").and_then(|v| v.as_str()).expect("answer.rtcSession"),
            ans.pointer("/answer/rtcIceCandidates").and_then(|v| v.as_str()).expect("answer.rtcIceCandidates"),
        )
        .await
        .unwrap();

    // Relay extras until both are connected.
    let mut connected = (false, false);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(40);
    while connected != (true, true) {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => panic!("not connected: {connected:?}"),
            Some(ev) = alice_ev.recv() => match ev {
                CallEvent::ExtraIce(ice) => { blocking({ let a = alice.clone(); move || a.call_extra(bob_in_alice, &ice) }).await.expect("extra"); }
                CallEvent::Connected => connected.0 = true,
                CallEvent::Ended(e) => panic!("alice ended: {e}"),
            },
            Some(ev) = bob_ev.recv() => match ev {
                CallEvent::ExtraIce(ice) => { blocking({ let b = bob.clone(); move || b.call_extra(alice_in_bob, &ice) }).await.expect("extra"); }
                CallEvent::Connected => connected.1 = true,
                CallEvent::Ended(e) => panic!("bob ended: {e}"),
            },
            Some(ev) = ea.recv() => if let Some(b) = other(&ev, "callExtraInfo") {
                alice_engine.add_remote_ice(b.pointer("/extraInfo/rtcIceCandidates").and_then(|v| v.as_str()).expect("extraInfo")).await;
            },
            Some(ev) = eb.recv() => if let Some(b) = other(&ev, "callExtraInfo") {
                bob_engine.add_remote_ice(b.pointer("/extraInfo/rtcIceCandidates").and_then(|v| v.as_str()).expect("extraInfo")).await;
            },
        }
    }
    blocking({ let a = alice.clone(); move || a.call_status(bob_in_alice, "connected") }).await.expect("status");

    let (mut at_alice, mut at_bob) = (Vec::new(), Vec::new());
    for p in tone_packets(100) {
        alice_engine.test_send(p.clone());
        bob_engine.test_send(p);
        tokio::time::sleep(Duration::from_millis(20)).await;
        at_alice.extend(alice_engine.test_received());
        at_bob.extend(bob_engine.test_received());
    }
    eprintln!("alice got {} frames (rms {:.0}), bob got {} (rms {:.0})", at_alice.len(), rms(&at_alice), at_bob.len(), rms(&at_bob));
    assert!(at_alice.len() > 60 && rms(&at_alice) > 1000.0);
    assert!(at_bob.len() > 60 && rms(&at_bob) > 1000.0);

    // Alice hangs up; bob hears about it.
    blocking({ let a = alice.clone(); move || a.call_end(bob_in_alice) }).await.expect("end");
    let ended = wait_for(&mut eb, 60, "bob: callEnded", |ev| other(ev, "callEnded").cloned()).await;
    assert_eq!(ended.pointer("/contact/contactId").and_then(|v| v.as_i64()), Some(alice_in_bob));
    alice_engine.close().await;
    bob_engine.close().await;
    let _ = std::fs::remove_dir_all(&root);
}

/// Declining is local: the caller is **not** told (no `callEnded`; only its
/// own `sndCall` item stays "pending"), as with the official clients. The
/// caller stops ringing by its own timeout (`call_flow::RING_TIMEOUT`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs libsimplex + network"]
async fn simplex_e2e_reject_is_local() {
    let (root, alice, bob, mut ea, mut eb, bob_in_alice, alice_in_bob) = setup("reject").await;
    blocking({ let a = alice.clone(); move || a.call_invite(bob_in_alice, CallMedia::Audio) })
        .await
        .expect("invite");
    wait_for(&mut eb, 60, "bob: callInvitation", |ev| other(ev, "callInvitation").map(|_| ())).await;
    blocking({ let b = bob.clone(); move || b.call_reject(alice_in_bob) }).await.expect("reject");
    let quiet = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(ev) = ea.recv().await {
            if other(&ev, "callEnded").is_some() {
                return;
            }
        }
        std::future::pending::<()>().await
    })
    .await;
    assert!(quiet.is_err(), "core now reports declines — use it in call_flow");
    let _ = std::fs::remove_dir_all(&root);
}
