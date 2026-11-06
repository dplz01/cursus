//! Interop with a browser WebRTC stack, the way the official clients run it.
//!
//! `linux/tests/call-interop/harness.html` replays the relevant parts of the
//! official `call.ts` in headless Firefox; this test serves it, relays the
//! signaling, and checks that encrypted audio flows both ways — once with the
//! browser as callee (it offers) and once as caller (it answers).
//!
//! Needs Firefox and network access (LZString from cdnjs), so it's ignored:
//! `cargo test -p cursus-linux browser -- --ignored --nocapture`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::tests::{key, rms, tone_packets};
use super::*;

struct Relay {
    to_browser: Mutex<std_mpsc::Receiver<Value>>,
    from_browser: std_mpsc::Sender<Value>,
}

fn serve(listener: TcpListener, relay: Arc<Relay>) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/call-interop");
    for stream in listener.incoming().flatten() {
        let relay = Arc::clone(&relay);
        let dir = dir.clone();
        std::thread::spawn(move || {
            let _ = handle(stream, &relay, &dir);
        });
    }
}

fn handle(mut stream: TcpStream, relay: &Relay, dir: &std::path::Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line.trim().is_empty() {
            break;
        }
        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let (status, ctype, out): (&str, &str, Vec<u8>) = if path.starts_with("/from-browser") {
        if let Ok(v) = serde_json::from_slice::<Value>(&body) {
            let _ = relay.from_browser.send(v);
        }
        ("200 OK", "text/plain", Vec::new())
    } else if path.starts_with("/to-browser") {
        let msg = relay
            .to_browser
            .lock()
            .ok()
            .and_then(|rx| rx.recv_timeout(Duration::from_secs(20)).ok());
        match msg {
            Some(v) => ("200 OK", "application/json", v.to_string().into_bytes()),
            None => ("204 No Content", "text/plain", Vec::new()),
        }
    } else if path.starts_with("/worker.js") {
        ("200 OK", "text/javascript", std::fs::read(dir.join("worker.js"))?)
    } else {
        ("200 OK", "text/html", std::fs::read(dir.join("harness.html"))?)
    };
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        out.len()
    )?;
    stream.write_all(&out)
}

fn firefox(url: &str) -> std::process::Child {
    let profile = std::env::temp_dir().join(format!("cursus-ff-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::write(
        profile.join("user.js"),
        r#"
user_pref("media.peerconnection.ice.obfuscate_host_addresses", false);
user_pref("media.autoplay.default", 0);
user_pref("media.autoplay.block-webaudio", false);
user_pref("browser.shell.checkDefaultBrowser", false);
user_pref("browser.startup.homepage_override.mstone", "ignore");
user_pref("datareporting.policy.dataSubmissionEnabled", false);
user_pref("toolkit.telemetry.reportingpolicy.firstRun", false);
"#,
    )
    .unwrap();
    std::process::Command::new("firefox")
        .args(["--headless", "--no-remote", "--profile"])
        .arg(&profile)
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("firefox")
}

fn rand_suffix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

/// Next non-log message from the browser (logs are printed).
fn recv(rx: &std_mpsc::Receiver<Value>, what: &str) -> Value {
    loop {
        let v = rx
            .recv_timeout(Duration::from_secs(40))
            .unwrap_or_else(|_| panic!("browser: no {what}"));
        match v["type"].as_str() {
            Some("log") => eprintln!("[browser] {}", v["msg"].as_str().unwrap_or("")),
            _ => return v,
        }
    }
}

async fn run(browser_offers: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (to_tx, to_rx) = std_mpsc::channel::<Value>();
    let (from_tx, from_rx) = std_mpsc::channel::<Value>();
    let relay = Arc::new(Relay { to_browser: Mutex::new(to_rx), from_browser: from_tx });
    std::thread::spawn(move || serve(listener, relay));

    let key = key();
    let role = if browser_offers { "offerer" } else { "answerer" };
    let mut ff = firefox(&format!("http://127.0.0.1:{port}/?role={role}&key={key}"));

    let (engine, mut events) = CallEngine::with_devices(Some(&key), false).await.unwrap();
    // Our late candidates → browser.
    {
        let to_tx = to_tx.clone();
        let mut forward = std::mem::replace(&mut events, mpsc::unbounded_channel().1);
        let (fwd_tx, fwd_rx) = mpsc::unbounded_channel();
        events = fwd_rx;
        tokio::spawn(async move {
            while let Some(ev) = forward.recv().await {
                if let CallEvent::ExtraIce(ice) = &ev {
                    let _ = to_tx.send(json!({"type": "ice", "iceCandidates": ice}));
                }
                let _ = fwd_tx.send(ev);
            }
        });
    }

    let from = Arc::new(Mutex::new(from_rx));
    let recv_async = |what: &'static str| {
        let from = Arc::clone(&from);
        async move { tokio::task::spawn_blocking(move || recv(&from.lock().unwrap(), what)).await.unwrap() }
    };

    if browser_offers {
        let offer = recv_async("offer").await;
        assert_eq!(offer["type"], "offer");
        let answer = engine
            .accept_offer(offer["offer"].as_str().unwrap(), offer["iceCandidates"].as_str().unwrap())
            .await
            .unwrap();
        to_tx
            .send(json!({"type": "answer", "answer": answer.rtc_session, "iceCandidates": answer.rtc_ice_candidates}))
            .unwrap();
    } else {
        let offer = engine.create_offer().await.unwrap();
        to_tx
            .send(json!({"type": "offer", "offer": offer.rtc_session, "iceCandidates": offer.rtc_ice_candidates}))
            .unwrap();
        let answer = recv_async("answer").await;
        engine
            .apply_answer(answer["answer"].as_str().unwrap(), answer["iceCandidates"].as_str().unwrap())
            .await
            .unwrap();
    }

    // Browser's late candidates → engine, until the result arrives.
    let result = {
        let ice_engine = engine.clone();
        let from = Arc::clone(&from);
        let (res_tx, mut res_rx) = mpsc::unbounded_channel::<Value>();
        let (ice_tx, mut ice_rx) = mpsc::unbounded_channel::<String>();
        std::thread::spawn(move || loop {
            let v = recv(&from.lock().unwrap(), "result");
            match v["type"].as_str() {
                Some("ice") => {
                    let _ = ice_tx.send(v["iceCandidates"].as_str().unwrap_or("").to_string());
                }
                _ => {
                    let _ = res_tx.send(v);
                    return;
                }
            }
        });
        tokio::spawn(async move {
            while let Some(ice) = ice_rx.recv().await {
                ice_engine.add_remote_ice(&ice).await;
            }
        });

        tokio::time::timeout(Duration::from_secs(30), super::tests::wait_connected(&mut events))
            .await
            .expect("engine connect timeout")
            .expect("engine connect");

        // Send a tone for ~3 s and collect what the browser sends us.
        let mut got = Vec::new();
        for p in tone_packets(150) {
            engine.test_send(p);
            tokio::time::sleep(Duration::from_millis(20)).await;
            got.extend(engine.test_received());
        }
        let result = tokio::time::timeout(Duration::from_secs(30), res_rx.recv()).await.ok().flatten();
        (got, result)
    };
    let (got, result) = result;
    let _ = ff.kill();
    let _ = ff.wait();
    engine.close().await;

    let result = result.expect("browser result");
    eprintln!("browser result: {result}; frames from browser: {}, rms {:.0}", got.len(), rms(&got));
    assert_eq!(result["connection"], "connected");
    assert!(result["packets"].as_f64().unwrap_or(0.0) > 50.0, "browser received too few packets");
    assert!(result["energy"].as_f64().unwrap_or(0.0) > 0.0, "browser decoded only silence");
    assert!(got.len() > 50, "too few frames from the browser");
    assert!(rms(&got) > 500.0, "browser audio silent / undecryptable");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Firefox + network"]
async fn browser_offers_cursus_answers() {
    run(true).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs Firefox + network"]
async fn cursus_offers_browser_answers() {
    run(false).await;
}
