use anyhow::Error;
use serde::Serialize;
use serde_json::{Map, Value, json};

#[derive(Debug, Serialize)]
pub struct JsonResponse {
    pub ok: bool,
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonError>,
}

#[derive(Debug, Serialize)]
pub struct JsonError {
    pub message: String,
}

pub fn success(data: Value) -> JsonResponse {
    JsonResponse {
        ok: true,
        data,
        error: None,
    }
}

pub fn from_error(error: &Error) -> JsonResponse {
    JsonResponse {
        ok: false,
        data: Value::Object(Map::new()),
        error: Some(JsonError {
            message: format!("{error:#}"),
        }),
    }
}

pub fn to_json_string(response: &JsonResponse) -> String {
    match serde_json::to_string_pretty(response) {
        Ok(payload) => payload,
        Err(error) => json!({
            "ok": false,
            "data": {},
            "error": { "message": format!("failed to serialize JSON output: {error}") }
        })
        .to_string(),
    }
}
