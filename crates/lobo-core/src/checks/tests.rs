use super::*;
use std::time::Duration;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{header, method, path},
};

fn tool_reply(arguments: Value, finish: &str) -> Value {
    json!({"choices":[{"finish_reason":finish,"message":{"tool_calls":[{"type":"function","function":{"name":"get_weather","arguments":arguments}}]}}]})
}
#[test]
fn validate_tool_call_table() {
    for (body, error) in [
        (
            tool_reply(json!("{\"location\":\"Bali\"}"), "tool_calls"),
            "",
        ),
        (
            tool_reply(json!({"location":"Bali"}), "tool_calls"),
            "not a JSON string",
        ),
        (tool_reply(json!("Bali"), "tool_calls"), "not a JSON object"),
        (
            json!({"choices":[{"finish_reason":"stop","message":{"content":"hi"}}]}),
            "no tool_calls",
        ),
        (tool_reply(json!("{}"), "stop"), "finish_reason"),
        (json!({"choices":[]}), "no choices"),
    ] {
        let result = validate_tool_call(&serde_json::to_vec(&body).unwrap());
        if error.is_empty() {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().to_string().contains(error));
        }
    }
    assert!(
        validate_tool_call(b"{")
            .unwrap_err()
            .to_string()
            .contains("bad json")
    );
}

fn chunk(content: &str) -> String {
    format!(
        "data: {}\n\n",
        json!({"choices":[{"delta":{"content":content},"finish_reason":null}]})
    )
}
fn end() -> &'static str {
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
}
#[tokio::test]
async fn read_stream_table() {
    for (input, error) in [
        (chunk("hel") + &chunk("lo") + end(), ""),
        (
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":null},\"finish_reason\":null}]}\n\n".to_owned()
                + &chunk("hello") + end(),
            "",
        ),
        (
            "data: {\"choices\":[{\"delta\":{\"content\":42}}]}\n\n".into(),
            "malformed",
        ),
        (chunk("hi"), "without [DONE]"),
        (
            chunk("hi") + "data: {\"error\":{\"message\":\"boom\"}}\n\n",
            "error event",
        ),
        (chunk("hi") + "data: {oops\n\n", "malformed"),
        (chunk("hi") + "data: [DONE]\n\n", "finish_reason"),
        (end().into(), "empty streamed reply"),
        ("x".repeat(1 << 20), "token too long"),
    ] {
        let result = read_stream(input.as_bytes()).await;
        if error.is_empty() {
            assert_eq!(result.unwrap(), "hello");
        } else {
            assert!(result.unwrap_err().to_string().contains(error), "{error}");
        }
    }
}

#[tokio::test]
async fn chat_request_shape() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer key"))
        .respond_with(|r: &Request| {
            let body = r.body_json::<Value>().unwrap();
            assert_eq!(body["stream"], true);
            assert_eq!(body["model"], "q8");
            assert_eq!(body["temperature"], 0.2);
            assert_eq!(body["max_tokens"], 200);
            ResponseTemplate::new(200).set_body_string(chunk("hello") + end())
        })
        .expect(1)
        .mount(&s)
        .await;
    let hc = crate::http::client(Duration::from_secs(3));
    assert_eq!(
        chat(&hc, &format!("{}/v1", s.uri()), "key", "q8")
            .await
            .unwrap(),
        "hello"
    );
}

#[tokio::test]
async fn tool_call_request_shape() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer key"))
        .respond_with(|r: &Request| {
            let body = r.body_json::<Value>().unwrap();
            assert_eq!(body["stream"], false);
            assert_eq!(body["tool_choice"], "auto");
            assert_eq!(body["tools"][0]["function"]["name"], "get_weather");
            ResponseTemplate::new(200)
                .set_body_json(tool_reply(json!("{\"location\":\"Bali\"}"), "tool_calls"))
        })
        .expect(1)
        .mount(&s)
        .await;
    let hc = crate::http::client(Duration::from_secs(3));
    validate_tool_call(
        &tool_call(&hc, &format!("{}/v1", s.uri()), "key", "q8")
            .await
            .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn request_error_includes_http_status() {
    let s = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).set_body_string(" unavailable\n"))
        .mount(&s)
        .await;
    let hc = crate::http::client(Duration::from_secs(3));
    assert_eq!(
        chat(&hc, &s.uri(), "key", "q8")
            .await
            .unwrap_err()
            .to_string(),
        "HTTP 503: unavailable"
    );
}
