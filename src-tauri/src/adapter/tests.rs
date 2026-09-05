use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

use super::openai_compatible::OpenAiCompatible;
use super::presets::{DEEPSEEK, OPENAI};
use super::*;

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

fn config() -> ChannelConfig {
    ChannelConfig {
        base_url: OPENAI.base_url.into(),
        api_key: "test-key".into(),
        models: vec![],
        model_mapping: HashMap::new(),
        extra: json!({}),
    }
}

#[test]
fn factory_selects_supported_presets_and_rejects_unknown_types() {
    assert!(get_adapter("openai", &client()).is_ok());
    assert!(get_adapter("deepseek", &client()).is_ok());
    assert!(matches!(
        get_adapter("claude", &client()),
        Err(AppError::InvalidInput(_))
    ));
    assert_eq!(DEEPSEEK.base_url, "https://api.edgefn.net/v1");
    assert_eq!(DEEPSEEK.models, &["DeepSeekV4Pro"]);
}

#[test]
fn joins_endpoint_without_duplicate_slash() {
    assert_eq!(
        OpenAiCompatible::endpoint("https://api.openai.com/v1/", "/models"),
        "https://api.openai.com/v1/models"
    );
}

#[test]
fn model_mapping_prefers_user_mapping_then_preset_then_original() {
    let adapter = OpenAiCompatible::new(client(), &DEEPSEEK);
    let mut config = config();
    assert_eq!(
        adapter.resolve_model(&config, "DeepSeekV4Pro"),
        "DeepSeek-V4-Pro-0813"
    );
    assert_eq!(
        adapter.resolve_model(&config, "custom-model"),
        "custom-model"
    );
    config
        .model_mapping
        .insert("DeepSeekV4Pro".into(), "user-model".into());
    assert_eq!(
        adapter.resolve_model(&config, "DeepSeekV4Pro"),
        "user-model"
    );
}

#[test]
fn request_mode_overrides_body_without_mutating_original() {
    for preset in [&OPENAI, &DEEPSEEK] {
        let adapter = OpenAiCompatible::new(client(), preset);
        let mut config = config();
        config
            .model_mapping
            .insert("alias".into(), "upstream".into());
        let request = ProxyRequest {
            model: "alias".into(),
            body: json!({"model": "ignored", "stream": true, "messages": [{"role": "user", "content": "hello"}]}),
        };
        let body = adapter.build_body(&config, &request, false).unwrap();
        assert_eq!(body["model"], "upstream");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"], request.body["messages"]);
        assert_eq!(request.body["model"], "ignored");
        assert_eq!(request.body["stream"], true);
        assert_eq!(
            adapter.build_body(&config, &request, true).unwrap()["stream"],
            true
        );
        assert!(adapter
            .build_body(
                &config,
                &ProxyRequest {
                    model: "x".into(),
                    body: json!([])
                },
                false
            )
            .is_err());
    }
}

#[test]
fn provider_headers_and_test_strategies_remain_distinct() {
    let mut config = config();
    config.extra = json!({"organization": "org", "project": "project"});
    let openai = OpenAiCompatible::new(client(), &OPENAI);
    let request = openai.test_request(&config).build().unwrap();
    assert_eq!(request.method(), reqwest::Method::GET);
    assert_eq!(request.url().path(), "/v1/models");
    assert_eq!(request.headers()["OpenAI-Organization"], "org");
    assert_eq!(request.headers()["OpenAI-Project"], "project");
    let chat = openai.chat_request(&config, &json!({})).build().unwrap();
    assert_eq!(chat.headers()["OpenAI-Organization"], "org");
    assert_eq!(chat.headers()["OpenAI-Project"], "project");

    config.base_url = DEEPSEEK.base_url.into();
    let deepseek = OpenAiCompatible::new(client(), &DEEPSEEK);
    let request = deepseek.test_request(&config).build().unwrap();
    assert_eq!(request.method(), reqwest::Method::POST);
    assert_eq!(
        request.url().as_str(),
        "https://api.edgefn.net/v1/chat/completions"
    );
    assert_eq!(request.headers()[AUTHORIZATION], "Bearer test-key");
    assert_eq!(request.headers()[CONTENT_TYPE], "application/json");
    assert!(!request.headers().contains_key("OpenAI-Organization"));
    let body: Value = serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
    assert_eq!(body["model"], "DeepSeek-V4-Pro-0813");
    assert_eq!(body["max_tokens"], 1);
    assert_eq!(body["stream"], false);
}

#[test]
fn extracts_usage_and_handles_missing_usage() {
    let usage = OpenAiCompatible::extract_usage(&json!({
        "usage": {"prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20}
    }))
    .unwrap();
    assert_eq!(usage.prompt_tokens, 12);
    assert_eq!(usage.completion_tokens, 8);
    assert_eq!(usage.total_tokens, 20);
    assert!(OpenAiCompatible::extract_usage(&json!({})).is_none());
    assert!(OpenAiCompatible::extract_usage(&json!({"usage": {}})).is_none());
}

/// 只监听本机，验证真实 HTTP 收发；不访问供应商、不需要 API Key。
fn mock_response(
    status: u16,
    content_type: &str,
    body: &str,
) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let response = format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let task = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("mock server accept failed: {error}"),
            }
        };
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 1024];
        loop {
            let n = socket.read(&mut buffer).unwrap();
            assert!(n > 0, "request ended before body was complete");
            bytes.extend_from_slice(&buffer[..n]);
            if let Some(end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        socket.write_all(response.as_bytes()).unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (url, task)
}

#[test]
fn forward_preserves_status_body_and_usage() {
    tauri::async_runtime::block_on(async {
        for (status, body) in [
            (
                200,
                r#"{"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}"#,
            ),
            (429, r#"{"error":{"message":"rate limited"}}"#),
            (502, "bad gateway"),
        ] {
            let (url, server) = mock_response(status, "application/json", body);
            let mut config = config();
            config.base_url = url;
            let adapter = get_adapter("openai", &client()).unwrap();
            let response = adapter
                .forward(
                    &ProxyRequest {
                        model: "m".into(),
                        body: json!({"stream": true}),
                    },
                    &config,
                )
                .await
                .unwrap();
            assert_eq!(response.status, status);
            if status == 200 {
                assert_eq!(response.usage.unwrap().total_tokens, 5);
            } else {
                assert!(response.usage.is_none());
                assert_eq!(
                    response.body,
                    serde_json::from_str::<Value>(body).unwrap_or(json!(body))
                );
            }
            let request = server.join().unwrap();
            assert!(request.starts_with("POST /v1/chat/completions"));
            let body: Value =
                serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
            assert_eq!(body["stream"], false);
        }
    });
}

#[test]
fn forward_stream_leaves_sse_response_unconsumed() {
    tauri::async_runtime::block_on(async {
        let body = "data: {\"choices\":[]}\n\ndata: [DONE]\n\n";
        let (url, server) = mock_response(200, "text/event-stream", body);
        let mut config = config();
        config.base_url = url;
        let response = get_adapter("deepseek", &client())
            .unwrap()
            .forward_stream(
                &ProxyRequest {
                    model: "DeepSeekV4Pro".into(),
                    body: json!({"stream": false}),
                },
                &config,
            )
            .await
            .unwrap();
        assert_eq!(response.headers()[CONTENT_TYPE], "text/event-stream");
        assert_eq!(response.text().await.unwrap(), body);
        let request = server.join().unwrap();
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        assert_eq!(body["stream"], true);
        assert_eq!(body["model"], "DeepSeek-V4-Pro-0813");
    });
}

#[test]
fn channel_test_reports_upstream_rejection() {
    tauri::async_runtime::block_on(async {
        let (url, server) = mock_response(
            401,
            "application/json",
            r#"{"error":{"message":"invalid key"}}"#,
        );
        let mut config = config();
        config.base_url = url;
        let result = get_adapter("openai", &client())
            .unwrap()
            .test(&config)
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.message.contains("401"));
        assert!(result.message.contains("invalid key"));
        assert!(server.join().unwrap().starts_with("GET /v1/models"));
    });
}

#[test]
#[ignore = "需要 DEEPSEEK_API_KEY，并会真实联网和消耗额度"]
fn streams_live_response() {
    tauri::async_runtime::block_on(async {
        let config = ChannelConfig {
            base_url: std::env::var("DEEPSEEK_BASE_URL")
                .unwrap_or_else(|_| DEEPSEEK.base_url.into()),
            api_key: std::env::var("DEEPSEEK_API_KEY").expect("请先设置 DEEPSEEK_API_KEY"),
            models: vec![],
            model_mapping: HashMap::new(),
            extra: json!({}),
        };
        let request = ProxyRequest {
            model: std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| DEEPSEEK.models[0].into()),
            body: json!({"messages": [{"role": "user", "content": "请依次输出1到5"}], "max_tokens": 100}),
        };
        let adapter = get_adapter("deepseek", &reqwest::Client::new()).unwrap();
        let mut response = adapter.forward_stream(&request, &config).await.unwrap();
        assert!(response.status().is_success());
        assert!(response.headers()[CONTENT_TYPE]
            .to_str()
            .unwrap()
            .contains("text/event-stream"));
        let mut received = Vec::new();
        while let Some(chunk) = response.chunk().await.unwrap() {
            std::io::stdout().write_all(&chunk).unwrap();
            std::io::stdout().flush().unwrap();
            received.extend_from_slice(&chunk);
        }
        assert!(String::from_utf8_lossy(&received).contains("data:"));
    });
}
