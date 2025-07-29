use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub cmd: Command,
    pub request_id: String,
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u32,
}

fn default_protocol_version() -> u32 {
    PROTOCOL_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Find {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        vis: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unimplemented: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo: Option<bool>,
    },
    Doc {
        path: String,
        #[serde(default)]
        include_docs: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        vis: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unimplemented: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo: Option<bool>,
    },
    Ls {
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        expansion: Option<String>,
        #[serde(default)]
        impl_parents: bool,
        #[serde(default)]
        include_docs: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        vis: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unimplemented: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo: Option<bool>,
    },
    Export {
        path: String,
        #[serde(default)]
        bundle: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        expansion: Option<String>,
        #[serde(default)]
        include_docs: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        vis: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unimplemented: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo: Option<bool>,
        #[serde(default)]
        impl_parents: bool,
        #[serde(default)]
        with_source: bool,
    },
    Reindex {
        #[serde(skip_serializing_if = "Option::is_none")]
        features: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    Status {
        #[serde(skip_serializing_if = "Option::is_none")]
        vis: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unimplemented: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        todo: Option<bool>,
    },
    Diag,
    Bench {
        #[serde(default = "default_queries")]
        queries: u32,
        #[serde(default = "default_warmup")]
        warmup: u32,
        #[serde(default = "default_duration")]
        duration: u32,
    },
}

fn default_queries() -> u32 {
    200
}

fn default_warmup() -> u32 {
    100
}

fn default_duration() -> u32 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Success(SuccessEnvelope),
    Decision(DecisionEnvelope),
    Error(ErrorEnvelope),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessEnvelope {
    pub ok: bool,
    pub request_id: String,
    pub protocol_version: u32,
    pub data: serde_json::Value,
    #[serde(default)]
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Metrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEnvelope {
    pub ok: bool,
    pub request_id: String,
    pub protocol_version: u32,
    pub decision_required: DecisionInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionInfo {
    pub reason: String,
    pub content_len: usize,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorEnvelope {
    pub ok: bool,
    pub request_id: String,
    pub protocol_version: u32,
    pub err: String,
    pub err_code: ErrorCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metrics {
    pub elapsed_ms: u64,
    pub bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    InvalidArg,
    NotFound,
    DaemonUnavailable,
    IndexMismatch,
    InternalError,
    ProtocolError,
}

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid protocol version: {0}")]
    InvalidProtocolVersion(u32),
    
    #[error("Message too large: {0} bytes")]
    MessageTooLarge(usize),
}

impl Response {
    pub fn success(request_id: String, data: serde_json::Value) -> Self {
        Response::Success(SuccessEnvelope {
            ok: true,
            request_id,
            protocol_version: PROTOCOL_VERSION,
            data,
            truncated: false,
            metrics: None,
        })
    }

    pub fn error(request_id: String, err: String, err_code: ErrorCode) -> Self {
        Response::Error(ErrorEnvelope {
            ok: false,
            request_id,
            protocol_version: PROTOCOL_VERSION,
            err,
            err_code,
        })
    }

    pub fn decision(request_id: String, reason: String, content_len: usize, options: Vec<String>) -> Self {
        Response::Decision(DecisionEnvelope {
            ok: true,
            request_id,
            protocol_version: PROTOCOL_VERSION,
            decision_required: DecisionInfo {
                reason,
                content_len,
                options,
            },
        })
    }
}

pub fn serialize_message<T: Serialize>(msg: &T) -> Result<String, ProtocolError> {
    let json = serde_json::to_string(msg)?;
    if json.contains('\n') {
        return Err(ProtocolError::MessageTooLarge(json.len()));
    }
    Ok(json)
}

pub fn deserialize_message<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, ProtocolError> {
    Ok(serde_json::from_str(line)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = Request {
            cmd: Command::Find {
                name: Some("State".to_string()),
                path: None,
                kind: Some("struct".to_string()),
                vis: Some("public".to_string()),
                unimplemented: None,
                todo: None,
            },
            request_id: "test-id".to_string(),
            protocol_version: 1,
        };

        let json = serialize_message(&req).unwrap();
        assert!(!json.contains('\n'));
        
        let parsed: Request = deserialize_message(&json).unwrap();
        assert_eq!(parsed.request_id, "test-id");
    }

    #[test]
    fn test_response_envelopes() {
        let success = Response::success(
            "req-1".to_string(),
            serde_json::json!({"count": 42}),
        );
        
        let json = serialize_message(&success).unwrap();
        let parsed: Response = deserialize_message(&json).unwrap();
        
        match parsed {
            Response::Success(env) => {
                assert_eq!(env.ok, true);
                assert_eq!(env.request_id, "req-1");
            }
            _ => panic!("Expected success envelope"),
        }
    }
}