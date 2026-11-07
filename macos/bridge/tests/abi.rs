//! End-to-end through the C ABI against the real libsimplex of this machine
//! (third_party/libsimplex). Uses a temporary data dir, never the user's.
//!
//!   cargo test -p cursus-bridge --test abi -- --ignored --nocapture
//!
//! With CURSUS_ABI_DUMP=<file> it writes every reply, for checking the JSON
//! shapes against macos/Cursus/Sources/Cursus/Models.swift.

use std::ffi::{CStr, CString};

use cursus_bridge::{cursus_call, cursus_free, cursus_init, cursus_next_event};
use serde_json::{json, Value};

fn call(method: &str, params: Value, dump: &mut Vec<Value>) -> Value {
    let req = CString::new(json!({ "method": method, "params": params }).to_string()).unwrap();
    let raw = unsafe { cursus_call(req.as_ptr()) };
    assert!(!raw.is_null());
    let text = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
    unsafe { cursus_free(raw) };
    let reply: Value = serde_json::from_str(&text).unwrap();
    dump.push(json!({ "method": method, "reply": reply }));
    if let Some(err) = reply.get("error") {
        panic!("{method}: {err}");
    }
    reply["ok"].clone()
}

#[test]
#[ignore]
fn abi_round_trip() {
    let home = std::env::temp_dir().join(format!("cursus-abi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    // Before any thread: data_dir() follows XDG_DATA_HOME on Linux.
    std::env::set_var("XDG_DATA_HOME", &home);
    cursus_init();
    let mut dump = Vec::new();

    let info = call("appInfo", json!({}), &mut dump);
    assert_eq!(info["setupComplete"], false);
    assert!(info["dataDir"].as_str().unwrap().starts_with(home.to_str().unwrap()));

    let st = call("selfTest", json!({}), &mut dump);
    assert_eq!(st["user"], "Self test");

    let user = call("open", json!({
        "password": "abi-test-password", "displayName": "Abi", "encryptLocalFiles": true, "saveAccount": true,
    }), &mut dump);
    assert_eq!(user["displayName"], "Abi");
    assert_eq!(call("appInfo", json!({}), &mut dump)["setupComplete"], true);

    let chats = call("listChats", json!({}), &mut dump);
    let notes = chats.as_array().unwrap().iter().find(|c| c["type"] == "local").cloned();
    if let Some(notes) = notes {
        let r = notes["ref"].as_str().unwrap().to_string();
        call("sendText", json!({ "chat": r, "text": "hello *bold* https://example.com" }), &mut dump);
        let msgs = call("listMessages", json!({ "chat": r, "count": 20 }), &mut dump);
        let last = msgs.as_array().unwrap().last().unwrap().clone();
        assert_eq!(last["outgoing"], true);
        assert!(last["segments"].as_array().unwrap().iter().any(|s| s["style"] == "bold"));
        // (Reactions are not supported in notes — core rule.)
        call("updateText", json!({ "chat": r, "itemId": last["id"], "text": "edited" }), &mut dump);
        call("listMessages", json!({ "chat": r, "count": 20 }), &mut dump);
        call("deleteItems", json!({ "chat": r, "itemIds": [last["id"]], "forEveryone": false }), &mut dump);
    } else {
        println!("no notes chat in this core version; skipped message round trip");
    }
    let updated = call("updateProfile", json!({ "displayName": "Abi2", "shortDescr": "bio" }), &mut dump);
    assert_eq!(updated["displayName"], "Abi2");
    call("setDeliveryReceipts", json!({ "groups": false, "on": false }), &mut dump);
    // Group info panel: create → alias → members (just me) → listChats shape.
    let group = call("createGroup", json!({ "name": "Abi group" }), &mut dump);
    let group_ref = group.as_str().unwrap().to_string();
    let group_id: i64 = group_ref.trim_start_matches('#').parse().unwrap();
    call("setAlias", json!({ "chat": group_ref, "alias": "Mine" }), &mut dump);
    let members = call("listMembers", json!({ "groupId": group_id }), &mut dump);
    let me = members.as_array().unwrap().iter().find(|m| m["isMe"] == true).cloned().expect("own member row");
    assert_eq!(me["role"], "owner");
    let chats = call("listChats", json!({}), &mut dump);
    let g = chats.as_array().unwrap().iter().find(|c| c["ref"] == group_ref.as_str()).cloned().unwrap();
    assert_eq!(g["alias"], "Mine");
    assert_eq!(g["title"], "Mine");
    assert_eq!(g["name"], "Abi group");
    call("leaveGroup", json!({ "groupId": group_id }), &mut dump);
    // Needs the network (SMP server).
    match std::env::var("CURSUS_ABI_OFFLINE") {
        Ok(_) => {}
        Err(_) => {
            let link = call("createInvitation", json!({}), &mut dump);
            assert!(link["full"].as_str().unwrap().contains("simplex"));
        }
    }
    let ev = cursus_next_event(200);
    if !ev.is_null() {
        let text = unsafe { CStr::from_ptr(ev) }.to_str().unwrap().to_string();
        unsafe { cursus_free(ev) };
        dump.push(json!({ "method": "event", "reply": { "ok": serde_json::from_str::<Value>(&text).unwrap() } }));
    }
    // Errors come back as {"error": …}, never a crash.
    let bad = CString::new(r#"{"method":"listMessages","params":{"chat":"nope"}}"#).unwrap();
    let raw = unsafe { cursus_call(bad.as_ptr()) };
    let text = unsafe { CStr::from_ptr(raw) }.to_str().unwrap().to_string();
    unsafe { cursus_free(raw) };
    assert!(text.contains("bad chat reference"), "{text}");

    call("close", json!({}), &mut dump);
    if let Ok(path) = std::env::var("CURSUS_ABI_DUMP") {
        std::fs::write(path, serde_json::to_string_pretty(&dump).unwrap()).unwrap();
    }
    let _ = std::fs::remove_dir_all(&home);
}
