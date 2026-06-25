use serde::{Deserialize, Serialize};

use crate::error::TransportError;

pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireMessage {
    Request {
        #[serde(rename = "reqId")]
        req_id: String,
        #[serde(deserialize_with = "deserialize_bytes_flexible")]
        bytes: Vec<u8>,
    },
    Response {
        #[serde(rename = "reqId")]
        req_id: String,
        #[serde(default, deserialize_with = "deserialize_optional_bytes_flexible")]
        bytes: Option<Vec<u8>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<WireError>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireError {
    pub message: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Framing {
    LengthPrefixed,
    NdJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeMessage {
    pub framing: Framing,
    pub token: Option<String>,
}

pub fn encode_frame(msg: &WireMessage, framing: Framing) -> Result<Vec<u8>, TransportError> {
    let payload = serde_json::to_vec(msg).map_err(|e| TransportError::InvalidFraming(e.to_string()))?;
    if payload.len() > MAX_FRAME_SIZE {
        return Err(TransportError::InvalidFraming(format!("frame too large: {}", payload.len())));
    }
    match framing {
        Framing::LengthPrefixed => {
            let mut frame = Vec::with_capacity(4 + payload.len());
            frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            frame.extend_from_slice(&payload);
            Ok(frame)
        }
        Framing::NdJson => {
            let mut frame = payload;
            frame.push(b'\n');
            Ok(frame)
        }
    }
}

pub fn decode_frame(buf: &[u8], framing: Framing, offset: &mut usize) -> Result<WireMessage, TransportError> {
    match framing {
        Framing::LengthPrefixed => {
            if buf.len() < *offset + 4 {
                return Err(TransportError::InvalidFraming("incomplete length header".to_string()));
            }
            let len = u32::from_le_bytes([
                buf[*offset], buf[*offset + 1], buf[*offset + 2], buf[*offset + 3],
            ]) as usize;
            if len > MAX_FRAME_SIZE {
                return Err(TransportError::InvalidFraming(format!("frame too large: {len}")));
            }
            if buf.len() < *offset + 4 + len {
                return Err(TransportError::InvalidFraming("incomplete payload".to_string()));
            }
            *offset += 4;
            let payload = &buf[*offset..*offset + len];
            *offset += len;
            serde_json::from_slice(payload).map_err(|e| TransportError::InvalidFraming(e.to_string()))
        }
        Framing::NdJson => {
            let start = *offset;
            let end = buf[start..].iter().position(|&b| b == b'\n')
                .map(|i| start + i)
                .ok_or_else(|| TransportError::InvalidFraming("missing newline".to_string()))?;
            let payload = &buf[start..end];
            *offset = end + 1;
            serde_json::from_slice(payload).map_err(|e| TransportError::InvalidFraming(e.to_string()))
        }
    }
}

pub fn encode_handshake(msg: &HandshakeMessage) -> Result<Vec<u8>, TransportError> {
    let mut payload = serde_json::to_vec(msg).map_err(|e| TransportError::InvalidFraming(e.to_string()))?;
    payload.push(b'\n');
    Ok(payload)
}

pub fn decode_handshake(line: &[u8]) -> Result<HandshakeMessage, TransportError> {
    let msg: HandshakeMessage = serde_json::from_slice(line)
        .map_err(|e| TransportError::InvalidFraming(format!("invalid handshake: {e}")))?;
    match msg.framing {
        Framing::LengthPrefixed | Framing::NdJson => Ok(msg),
    }
}

fn deserialize_bytes_flexible<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Array(arr) => {
            arr.into_iter()
                .map(|v| v.as_u64().map(|n| n as u8).ok_or_else(|| D::Error::custom("byte array contains non-u8")))
                .collect()
        }
        serde_json::Value::Object(obj) => {
            let mut pairs: Vec<(usize, u8)> = Vec::with_capacity(obj.len());
            for (k, v) in obj.into_iter() {
                let idx: usize = k.parse().map_err(|_| D::Error::custom("non-numeric byte object key"))?;
                let byte = v.as_u64().map(|n| n as u8).ok_or_else(|| D::Error::custom("byte object value not u8"))?;
                pairs.push((idx, byte));
            }
            pairs.sort_by_key(|(i, _)| *i);
            let len = pairs.last().map(|(i, _)| i + 1).unwrap_or(0);
            let mut bytes = vec![0u8; len];
            for (i, b) in pairs {
                bytes[i] = b;
            }
            Ok(bytes)
        }
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err(D::Error::custom("bytes field must be array or numeric object")),
    }
}

fn deserialize_optional_bytes_flexible<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Array(arr) => {
            let bytes: Vec<u8> = arr.into_iter()
                .map(|v| v.as_u64().map(|n| n as u8).ok_or_else(|| D::Error::custom("byte array contains non-u8")))
                .collect::<Result<Vec<u8>, _>>()?;
            Ok(Some(bytes))
        }
        serde_json::Value::Object(obj) => {
            let mut pairs: Vec<(usize, u8)> = Vec::with_capacity(obj.len());
            for (k, v) in obj.into_iter() {
                let idx: usize = k.parse().map_err(|_| D::Error::custom("non-numeric byte object key"))?;
                let byte = v.as_u64().map(|n| n as u8).ok_or_else(|| D::Error::custom("byte object value not u8"))?;
                pairs.push((idx, byte));
            }
            pairs.sort_by_key(|(i, _)| *i);
            let len = pairs.last().map(|(i, _)| i + 1).unwrap_or(0);
            let mut bytes = vec![0u8; len];
            for (i, b) in pairs {
                bytes[i] = b;
            }
            Ok(Some(bytes))
        }
        _ => Err(D::Error::custom("bytes field must be array, numeric object, or null")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefixed_roundtrip() {
        let msg = WireMessage::Request {
            req_id: "r1".to_string(),
            bytes: b"hello".to_vec(),
        };
        let frame = encode_frame(&msg, Framing::LengthPrefixed).unwrap();
        let mut offset = 0usize;
        let decoded = decode_frame(&frame, Framing::LengthPrefixed, &mut offset).unwrap();
        match decoded {
            WireMessage::Request { req_id, bytes } => {
                assert_eq!(req_id, "r1");
                assert_eq!(bytes, b"hello");
            }
            _ => panic!("expected request"),
        }
        assert_eq!(offset, frame.len());
    }

    #[test]
    fn ndjson_roundtrip() {
        let msg = WireMessage::Response {
            req_id: "r2".to_string(),
            bytes: Some(b"world".to_vec()),
            error: None,
        };
        let frame = encode_frame(&msg, Framing::NdJson).unwrap();
        assert!(frame.ends_with(b"\n"));
        let mut offset = 0usize;
        let decoded = decode_frame(&frame, Framing::NdJson, &mut offset).unwrap();
        match decoded {
            WireMessage::Response { req_id, bytes, error } => {
                assert_eq!(req_id, "r2");
                assert_eq!(bytes.unwrap(), b"world");
                assert!(error.is_none());
            }
            _ => panic!("expected response"),
        }
    }

    #[test]
    fn decodes_bytes_as_numeric_object_like_ts_uint8array() {
        // First verify what serde serializes variant fields as
        let debug_msg = WireMessage::Request { req_id: "r3".to_string(), bytes: vec![1, 2, 3] };
        let debug_json = serde_json::to_string(&debug_msg).unwrap();
        // Use the same field naming that serde actually produces
        let payload = format!("{debug_json}\n");
        let mut offset = 0usize;
        let decoded = decode_frame(payload.as_bytes(), Framing::NdJson, &mut offset).unwrap();
        match decoded {
            WireMessage::Request { bytes, .. } => assert_eq!(bytes, vec![1, 2, 3]),
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn rejects_frame_too_large() {
        let big = WireMessage::Request {
            req_id: "x".to_string(),
            bytes: vec![0u8; MAX_FRAME_SIZE + 1],
        };
        let err = encode_frame(&big, Framing::LengthPrefixed).unwrap_err();
        assert!(matches!(err, TransportError::InvalidFraming(_)));
    }

    #[test]
    fn handshake_roundtrip() {
        let msg = HandshakeMessage { framing: Framing::LengthPrefixed, token: Some("tok".to_string()) };
        let line = encode_handshake(&msg).unwrap();
        assert!(line.ends_with(b"\n"));
        let decoded = decode_handshake(&line).unwrap();
        assert_eq!(decoded.framing, Framing::LengthPrefixed);
        assert_eq!(decoded.token, Some("tok".to_string()));
    }

    #[test]
    fn handshake_rejects_invalid_framing() {
        let line = br#"{"framing":"gzip","token":null}"#;
        let err = decode_handshake(line).unwrap_err();
        assert!(matches!(err, TransportError::InvalidFraming(_)));
    }
}
