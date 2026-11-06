//! Wire format of SimpleX call signaling.
//!
//! The official clients run WebRTC in a WebView (`call.ts`) and pack the
//! session description and ICE candidates as
//! `LZString.compressToBase64(JSON.stringify(x))` before handing them to the
//! chat core (`/_call offer|answer|extra`). We speak the same format so calls
//! interoperate with them.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// `RTCSessionDescriptionInit` as the browser serializes it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionDescription {
    #[serde(rename = "type")]
    pub kind: String,
    pub sdp: String,
}

/// `RTCIceCandidateInit` as the browser serializes it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct IceCandidate {
    pub candidate: String,
    #[serde(default)]
    pub sdp_mid: Option<String>,
    #[serde(default, rename = "sdpMLineIndex")]
    pub sdp_mline_index: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username_fragment: Option<String>,
}

/// `LZString.compressToBase64(JSON.stringify(value))`.
pub fn pack<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
    lz_str::compress_to_base64(json.as_str())
}

/// Inverse of [`pack`].
pub fn unpack<T: DeserializeOwned>(packed: &str) -> Result<T, String> {
    let wide = lz_str::decompress_from_base64(packed.trim())
        .ok_or_else(|| "call signaling: bad LZString payload".to_string())?;
    let json = String::from_utf16(&wide).map_err(|e| format!("call signaling: {e}"))?;
    serde_json::from_str(&json).map_err(|e| format!("call signaling JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_session_and_candidates() {
        let sd = SessionDescription { kind: "offer".into(), sdp: "v=0\r\no=- 1 2 IN IP4 0.0.0.0\r\n".into() };
        assert_eq!(unpack::<SessionDescription>(&pack(&sd)).unwrap(), sd);
        let ice = vec![IceCandidate {
            candidate: "candidate:1 1 udp 2130706431 192.0.2.1 50000 typ host".into(),
            sdp_mid: Some("0".into()),
            sdp_mline_index: Some(0),
            username_fragment: None,
        }];
        assert_eq!(unpack::<Vec<IceCandidate>>(&pack(&ice)).unwrap(), ice);
    }

    /// Produced by lz-string 1.4.4 in a browser:
    /// `LZString.compressToBase64(JSON.stringify({type:"answer",sdp:"v=0"}))`.
    #[test]
    fn decodes_browser_output() {
        let browser = lz_str::compress_to_base64(r#"{"type":"answer","sdp":"v=0"}"#);
        let sd: SessionDescription = unpack(&browser).unwrap();
        assert_eq!(sd.kind, "answer");
        assert_eq!(sd.sdp, "v=0");
    }

    #[test]
    fn browser_candidate_with_nulls_parses() {
        let packed = lz_str::compress_to_base64(
            r#"[{"candidate":"candidate:9 1 udp 1 198.51.100.7 3478 typ srflx raddr 0.0.0.0 rport 0","sdpMid":"0","sdpMLineIndex":0,"usernameFragment":"abcd"},{"candidate":"","sdpMid":null,"sdpMLineIndex":null}]"#,
        );
        let list: Vec<IceCandidate> = unpack(&packed).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].username_fragment.as_deref(), Some("abcd"));
        assert!(list[1].sdp_mid.is_none());
    }
}
