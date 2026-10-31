//! The execution engine owns evidence and tools; this module owns model transport.
use crate::{settings::ModelConfig, *};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::time::Instant;

pub async fn request(
    config: &ModelConfig,
    mut body: Value,
    cancel: &CancellationToken,
) -> Result<Vec<Value>, String> {
    body["model"] = json!(config.model);
    body["stream"] = json!(true);
    body["store"] = json!(false);
    let effort = body["reasoning"]["effort"]
        .as_str()
        .unwrap_or("high")
        .to_owned();
    // DeepSeek-specific reasoning levels are not sent to arbitrary compatible endpoints.
    if config.provider != "deepseek" {
        body.as_object_mut()
            .ok_or("模型请求格式错误")?
            .remove("reasoning");
    }
    let chat_protocol = config.protocol == "chat-completions";
    let messages_protocol = config.protocol == "anthropic-messages";
    if chat_protocol {
        body = crate::chat_completions::request_body(
            &body,
            matches!(
                config.provider.as_str(),
                "deepseek" | "kimi" | "kimi-cn" | "glm" | "zai"
            ),
        )?;
        configure_chat(&mut body, config, &effort);
    } else if messages_protocol {
        body = crate::anthropic_messages::request_body(&body)?;
    }
    let client = config.client()?;
    let mut request = client
        .post(config.endpoint()?)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&body);
    if !config.api_key.is_empty() {
        request = if messages_protocol {
            request.header("x-api-key", &config.api_key)
        } else {
            request.bearer_auth(&config.api_key)
        };
    }
    if messages_protocol {
        request = request.header("anthropic-version", "2023-06-01");
    }
    let response = tokio::select! {_ = cancel.cancelled()=>return Err("canceled".into()),r=request.send()=>r.map_err(|_|"模型网络连接失败，请检查地址、代理与服务状态")?};
    if !response.status().is_success() {
        // Inspect only bounded error codes; never expose provider messages or echoed input.
        let status = response.status().as_u16();
        if status == 400 {
            return Err(
                "模型拒绝请求（HTTP 400）；请核对协议、工具往返与上下文格式。配置和任务已保留"
                    .into(),
            );
        }
        if status == 402 {
            return Err("模型服务要求付款（HTTP 402）；请检查服务商余额或套餐".into());
        }
        if status == 429 && quota_exhausted(response, cancel).await? {
            return Err("模型额度已耗尽，请检查服务商余额；任务和记录已保留".into());
        }
        return Err(format!(
            "模型服务返回 HTTP {}；请检查模型 ID、密钥和服务权限",
            status
        ));
    }
    if !response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        })
    {
        return Err("模型未返回 text/event-stream；请检查 API 地址与协议".into());
    }
    let mut stream = response.bytes_stream();
    let mut parser = ResponseStream::default();
    let mut messages_parser = crate::anthropic_messages::MessagesStream::default();
    let mut chat_parser = crate::chat_completions::ChatStream::default();
    loop {
        let chunk = tokio::select! {_ = cancel.cancelled()=>return Err("canceled".into()),chunk=stream.next()=>chunk};
        match chunk {
            Some(Ok(chunk)) => {
                if let Some(output) = if chat_protocol {
                    chat_parser.feed(&chunk)?
                } else if messages_protocol {
                    messages_parser.feed(&chunk)?
                } else {
                    parser.feed(&chunk)?
                } {
                    return Ok(output);
                }
            }
            Some(Err(_)) => return Err("模型流中断，尚未执行后续操作".into()),
            None => return Err("模型未返回完整结果，尚未执行后续操作".into()),
        }
    }
}
async fn quota_exhausted(
    response: reqwest::Response,
    cancel: &CancellationToken,
) -> Result<bool, String> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    // The HTTP status is already known; optional diagnostics cannot hold the
    // worker for the normal 180-second generation timeout.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while let Some(Ok(chunk)) = tokio::select! {
        _ = cancel.cancelled() => return Err("canceled".into()),
        _ = tokio::time::sleep_until(deadline) => None,
        chunk = stream.next() => chunk,
    } {
        if bytes.len() + chunk.len() > 8192 {
            break;
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(serde_json::from_slice::<Value>(&bytes)
        .ok()
        .is_some_and(|value| {
            ["insufficient_quota", "billing_hard_limit_reached"]
                .contains(&value["error"]["code"].as_str().unwrap_or(""))
        }))
}

fn configure_chat(body: &mut Value, config: &ModelConfig, effort: &str) {
    let fast = effort == "none";
    if config.provider == "deepseek" {
        let forced = body["tool_choice"] == "required" || body["tool_choice"].is_object();
        body["thinking"] = json!({"type":if fast||forced{"disabled"}else{"enabled"}});
        if !fast && !forced {
            body["reasoning_effort"] = json!(effort);
        }
    }
    if matches!(config.provider.as_str(), "kimi" | "kimi-cn" | "glm" | "zai") {
        body["thinking"] = json!({"type":if fast{"disabled"}else{"enabled"}});
    }
    if matches!(config.provider.as_str(), "glm" | "zai") {
        if let Some(name) = body["tool_choice"]["function"]["name"]
            .as_str()
            .map(str::to_owned)
            && let Some(tools) = body["tools"].as_array_mut()
        {
            tools.retain(|tool| tool["function"]["name"] == name);
        }
        if body["tools"].is_array() && body["tool_choice"] != "none" {
            body["tool_choice"] = json!("auto");
        }
        body.as_object_mut().unwrap().remove("parallel_tool_calls");
    }
}
#[derive(Default)]
struct ResponseStream {
    sse: crate::model_sse::SseDecoder,
}
impl ResponseStream {
    fn feed(&mut self, chunk: &[u8]) -> Result<Option<Vec<Value>>, String> {
        for data in self.sse.feed(chunk)? {
            if data.is_empty() || data == "[DONE]" {
                continue;
            }
            let event: Value = serde_json::from_str(&data).map_err(|_| "模型流格式无效")?;
            match event["type"].as_str() {
                Some("response.completed") => {
                    return event["response"]["output"]
                        .as_array()
                        .cloned()
                        .map(Some)
                        .ok_or("模型返回缺少完整输出".into());
                }
                Some("response.failed" | "response.incomplete" | "error") => {
                    return Err("模型未完成本轮回复；未将部分输出作为操作执行".into());
                }
                _ => {}
            }
        }
        Ok(None)
    }
}
fn text(output: &[Value]) -> String {
    output
        .iter()
        .filter(|v| v["type"] == "message")
        .flat_map(|v| v["content"].as_array().into_iter().flatten())
        .filter_map(|v| v["text"].as_str())
        .collect::<Vec<_>>()
        .join("")
}
async fn check(
    config: &ModelConfig,
    name: &str,
    body: Value,
    expected: &str,
) -> wire::CapabilityCheck {
    let start = Instant::now();
    let cancel = CancellationToken::new();
    let result =
        tokio::time::timeout(Duration::from_secs(55), request(config, body, &cancel)).await;
    let (passed, detail) = match result {
        Ok(Ok(output)) if text(&output).contains(expected) => (true, "通过".into()),
        Ok(Ok(_)) => (false, "已收到输出，但实际内容未通过验证".into()),
        Ok(Err(e)) => (false, e),
        Err(_) => (false, "测试超过 55 秒，请检查网络或模型速度".into()),
    };
    wire::CapabilityCheck {
        name: name.into(),
        passed,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
pub async fn probe(config: &ModelConfig) -> Vec<wire::CapabilityCheck> {
    let nonce = Uuid::new_v4().simple().to_string();
    let text_body = json!({"input":[{"role":"user","content":[{"type":"input_text","text":format!("Reply exactly with this code: {nonce}")}]}],"max_output_tokens":256,"reasoning":{"effort":"none"}});
    let image_body = json!({"input":[{"role":"user","content":[{"type":"input_text","text":"Read the four digits in this image. Reply only with the digits."},{"type":"input_image","image_url":format!("data:image/png;base64,{}",STANDARD.encode(include_bytes!("../fixtures/vision-probe.png")))}]}],"max_output_tokens":256,"reasoning":{"effort":"none"}});
    let (text_check, image_check, tool_check) = tokio::join!(
        check(config, "文字与流式输出", text_body, &nonce),
        check(config, "图片识别", image_body, "7319"),
        tool_probe(config)
    );
    vec![text_check, image_check, tool_check]
}
async fn tool_probe(config: &ModelConfig) -> wire::CapabilityCheck {
    let start = Instant::now();
    let nonce = Uuid::new_v4().simple().to_string();
    let cancel = CancellationToken::new();
    let tool = json!({"type":"function","name":"diagnostic_echo","description":"Returns a diagnostic result; no external effects.","parameters":{"type":"object","properties":{"code":{"type":"string"}},"required":["code"],"additionalProperties":false}});
    let initial = json!({"role":"user","content":[{"type":"input_text","text":format!("Call diagnostic_echo with code {nonce}. After the result, reply with only the returned receipt.")} ]});
    let work = async {
        let output=request(config,json!({"input":[initial.clone()],"tools":[tool],"tool_choice":{"type":"function","name":"diagnostic_echo"},"parallel_tool_calls":false,"reasoning":{"effort":"none"},"max_output_tokens":512}),&cancel).await?;
        let calls: Vec<_> = output
            .iter()
            .filter(|v| v["type"] == "function_call")
            .collect();
        if calls.len() != 1 || calls[0]["name"] != "diagnostic_echo" {
            return Err("模型没有正确调用测试工具".into());
        }
        let call = calls[0];
        let args: Value = serde_json::from_str(call["arguments"].as_str().unwrap_or(""))
            .map_err(|_| "工具参数不是有效 JSON")?;
        if args["code"] != nonce || call["call_id"].as_str().unwrap_or("").is_empty() {
            return Err("工具参数或调用标识不匹配".into());
        }
        let receipt = Uuid::new_v4().simple().to_string();
        let result = json!({"type":"function_call_output","call_id":call["call_id"],"output":json!({"receipt":receipt}).to_string()});
        let mut input = vec![initial];
        input.extend(output);
        input.push(result);
        let final_output = request(
            config,
            json!({"input":input,"max_output_tokens":256,"reasoning":{"effort":"none"}}),
            &cancel,
        )
        .await?;
        if !text(&final_output).contains(&receipt) {
            return Err("模型未读取工具的实际返回结果".into());
        }
        Ok::<(), String>(())
    };
    let (passed, detail) = match tokio::time::timeout(Duration::from_secs(110), work).await {
        Ok(Ok(())) => (true, "调用参数、结果回传与后续读取均通过".into()),
        Ok(Err(e)) => (false, e),
        Err(_) => (false, "工具往返测试超时".into()),
    };
    wire::CapabilityCheck {
        name: "工具调用与结果读取".into(),
        passed,
        detail,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deepseek_forced_tools_disable_thinking_without_removing_the_tool_constraint() {
        let config = ModelConfig {
            provider: "deepseek".into(),
            protocol: "chat-completions".into(),
            api_key: String::new(),
            base_url: "https://fixture.invalid".into(),
            model: "fixture".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        for choice in [
            json!("required"),
            json!({"type":"function","function":{"name":"observe"}}),
        ] {
            let mut body = json!({"tool_choice":choice});
            configure_chat(&mut body, &config, "high");
            assert_eq!(body["thinking"]["type"], "disabled");
            assert_eq!(body["tool_choice"], choice);
            assert!(body.get("reasoning_effort").is_none());
        }
    }

    #[test]
    fn glm_keeps_the_requested_tool_boundary_and_explicit_no_tools() {
        let config = ModelConfig {
            provider: "glm".into(),
            protocol: "chat-completions".into(),
            api_key: String::new(),
            base_url: "https://fixture.invalid".into(),
            model: "fixture".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let mut body = json!({"tools":[{"function":{"name":"observe"}},{"function":{"name":"tap"}}],"tool_choice":{"function":{"name":"observe"}},"parallel_tool_calls":false});
        configure_chat(&mut body, &config, "high");
        assert_eq!(body["tools"].as_array().unwrap().len(), 1);
        assert_eq!(body["tools"][0]["function"]["name"], "observe");
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["thinking"]["type"], "enabled");
        body["tool_choice"] = json!("none");
        configure_chat(&mut body, &config, "none");
        assert_eq!(body["tool_choice"], "none");
        assert_eq!(body["thinking"]["type"], "disabled");
        assert!(body.get("parallel_tool_calls").is_none());
    }

    #[tokio::test]
    async fn native_messages_http_auth_image_and_tool_round_trip() {
        use axum::{Json, Router, http::HeaderMap, routing::post};
        fn events(tool: bool) -> String {
            let block = if tool {
                json!({"type":"tool_use","id":"native-call","name":"observe","input":{}})
            } else {
                json!({"type":"text","text":"actual-native-receipt"})
            };
            [json!({"type":"message_start","message":{"role":"assistant"}}),json!({"type":"content_block_start","index":0,"content_block":block}),json!({"type":"content_block_stop","index":0}),json!({"type":"message_delta","delta":{"stop_reason":if tool{"tool_use"}else{"end_turn"}}}),json!({"type":"message_stop"})].iter().map(|v|format!("data: {v}\n\n")).collect()
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new().route(
            "/v1/messages",
            post(|headers: HeaderMap, Json(body): Json<Value>| async move {
                assert_eq!(headers["x-api-key"], "fixture-secret");
                assert_eq!(headers["anthropic-version"], "2023-06-01");
                assert!(!headers.contains_key("authorization"));
                assert!(body.get("input").is_none());
                assert!(body.get("reasoning").is_none());
                assert_eq!(body["system"], "runtime-policy");
                let tool = body["messages"].as_array().unwrap().iter().any(|v| {
                    v["content"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p["type"] == "tool_result")
                });
                if tool {
                    assert_eq!(body["messages"][1]["content"][0]["id"], "native-call");
                    assert_eq!(
                        body["messages"][2]["content"][0]["tool_use_id"],
                        "native-call"
                    );
                    assert_eq!(
                        body["messages"][2]["content"][0]["content"][0]["text"],
                        "actual-result"
                    );
                } else {
                    assert_eq!(
                        body["messages"][0]["content"][1]["source"]["media_type"],
                        "image/png"
                    );
                    assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
                }
                ([("content-type", "text/event-stream")], events(!tool))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = ModelConfig {
            provider: "anthropic".into(),
            protocol: "anthropic-messages".into(),
            api_key: "fixture-secret".into(),
            base_url: format!("http://{address}"),
            model: "fixture".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let mut input = vec![
            json!({"role":"user","content":[{"type":"input_text","text":"inspect"},{"type":"input_image","image_url":"data:image/png;base64,Zml4dHVyZQ=="}]}),
        ];
        let first=request(&config,json!({"instructions":"runtime-policy","input":input,"tools":[{"type":"function","name":"observe","description":"inspect","parameters":{"type":"object"}}]}),&CancellationToken::new()).await.unwrap();
        assert_eq!(first[0]["call_id"], "native-call");
        input.extend(first);
        input.push(
            json!({"type":"function_call_output","call_id":"native-call","output":"actual-result"}),
        );
        let projected = crate::model_context::for_chat_api(&input);
        let second = request(
            &config,
            json!({"instructions":"runtime-policy","input":projected}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        server.abort();
        assert_eq!(text(&second), "actual-native-receipt");
    }

    async fn error_fixture(
        status: StatusCode,
        stalled: bool,
    ) -> (ModelConfig, tokio::task::JoinHandle<()>) {
        use axum::{Router, body::Body, routing::post};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new().route(
            "/v1/responses",
            post(move || async move {
                let body = if stalled {
                    Body::from_stream(
                        futures_util::stream::once(async {
                            Ok::<_, std::io::Error>(Bytes::from_static(b"{"))
                        })
                        .chain(futures_util::stream::pending()),
                    )
                } else {
                    Body::from("{\"error\":{\"message\":\"private provider details\"}}")
                };
                (status, body)
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (
            ModelConfig {
                api_key: String::new(),
                base_url: format!("http://{address}/v1"),
                model: "fixture".into(),
                provider: "compatible".into(),
                protocol: "responses".into(),
                proxy_url: String::new(),
                verified_at_ms: 0,
            },
            server,
        )
    }
    #[tokio::test]
    async fn payment_required_reports_billing_without_disclosing_provider_payload() {
        let (config, server) = error_fixture(StatusCode::PAYMENT_REQUIRED, false).await;
        let result = request(&config, json!({"input":[]}), &CancellationToken::new())
            .await
            .unwrap_err();
        server.abort();
        assert!(result.contains("HTTP 402"));
        assert!(result.contains("余额"));
        assert!(!result.contains("private provider details"));
        assert!(crate::agent_loop::model_retry_delay(&result, 1).is_none());
    }
    #[tokio::test]
    async fn a_stalled_rate_limit_body_cannot_hold_the_agent_for_the_model_timeout() {
        let (config, server) = error_fixture(StatusCode::TOO_MANY_REQUESTS, true).await;
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            request(&config, json!({"input":[]}), &CancellationToken::new()),
        )
        .await;
        server.abort();
        let error = result
            .expect("an error-body inspection has its own short deadline")
            .unwrap_err();
        assert!(error.contains("HTTP 429"));
        assert_eq!(crate::agent_loop::model_retry_delay(&error, 1), Some(2000));
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_stalled_rate_limit_body() {
        use axum::{Router, body::Body, http::StatusCode, routing::post};
        let ready = Arc::new(tokio::sync::Notify::new());
        let handler_ready = ready.clone();
        let router = Router::new().route(
            "/v1/responses",
            post(move || {
                let ready = handler_ready.clone();
                async move {
                    let chunks = futures_util::stream::once(async move {
                        ready.notify_one();
                        Ok::<_, std::io::Error>(Bytes::from_static(b"{"))
                    })
                    .chain(futures_util::stream::pending());
                    (StatusCode::TOO_MANY_REQUESTS, Body::from_stream(chunks))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = ModelConfig {
            api_key: String::new(),
            base_url: format!("http://{address}/v1"),
            model: "fixture".into(),
            provider: "compatible".into(),
            protocol: "responses".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let mut task =
            tokio::spawn(async move { request(&config, json!({"input":[]}), &token).await });
        tokio::time::timeout(Duration::from_secs(2), ready.notified())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_millis(300), &mut task).await;
        task.abort();
        server.abort();
        assert_eq!(
            result
                .expect("cancel must not wait for the provider error body")
                .unwrap(),
            Err("canceled".into())
        );
    }
    #[tokio::test]
    async fn chat_protocol_round_trip_through_a_real_local_http_endpoint() {
        use axum::{Json, Router, routing::post};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router=Router::new().route("/v1/chat/completions",post(|Json(value):Json<Value>| async move {
            assert_eq!(value["model"],"fixture-model");
            assert_eq!(value["stream"],true);
            assert!(value.get("input").is_none());
            assert_eq!(value["thinking"]["type"],"enabled");
            assert_eq!(value["reasoning_effort"],"high");
            let text=value["messages"].as_array().unwrap().iter().find(|m|m["role"]=="tool");
            let delta=if let Some(result)=text {
                assert_eq!(result["tool_call_id"],"fixture-call");
                assert_eq!(value["messages"][1]["reasoning_content"],"private-replay");
                json!({"content":format!("Receipt: {}",result["content"].as_str().unwrap())})
            } else {
                assert_eq!(value["messages"][0]["content"][1]["image_url"]["url"],"data:image/png;base64,fixture");
                json!({"reasoning_content":"private-replay","tool_calls":[{"index":0,"id":"fixture-call","type":"function","function":{"name":"diagnostic_echo","arguments":"{\"code\":\"receipt\"}"}}]})
            };
            let reason=if text.is_some(){"stop"}else{"tool_calls"};
            ([ ("content-type","text/event-stream") ],format!("data: {}\n\ndata: [DONE]\n\n",json!({"choices":[{"index":0,"delta":delta,"finish_reason":reason}]})))
        }));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = ModelConfig {
            api_key: String::new(),
            base_url: format!("http://{address}/v1"),
            model: "fixture-model".into(),
            provider: "deepseek".into(),
            protocol: "chat-completions".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let input = json!({"role":"user","content":[{"type":"input_text","text":"inspect"},{"type":"input_image","image_url":"data:image/png;base64,fixture"}]});
        let output = request(
            &config,
            json!({"input":[input.clone()]}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(output[0]["name"], "diagnostic_echo");
        let mut context = vec![input];
        context.extend(output);
        context.push(json!({"type":"function_call_output","call_id":"fixture-call","output":"actual-local-result"}));
        let output = request(&config, json!({"input":context}), &CancellationToken::new())
            .await
            .unwrap();
        assert!(text(&output).contains("actual-local-result"));
        server.abort();
    }
    #[test]
    fn stream_accepts_split_utf8_and_completed_without_done() {
        let mut p = ResponseStream::default();
        let bytes="data: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"text\":\"成功\"}]}}\r\n\r\n".as_bytes();
        let cut = bytes.iter().position(|b| *b > 127).unwrap() + 1;
        assert!(p.feed(&bytes[..cut]).unwrap().is_none());
        assert_eq!(p.feed(&bytes[cut..]).unwrap().unwrap()[0]["text"], "成功");
    }
    #[test]
    fn responses_support_standard_multiline_event_data() {
        let mut parser = ResponseStream::default();
        let event = b": heartbeat\r\nevent: response.completed\r\ndata: {\"type\":\"response.completed\",\r\ndata: \"response\":{\"output\":[{\"type\":\"message\",\"content\":[{\"text\":\"receipt\"}]}]}}\r\n\r\n";
        let output = parser.feed(event).unwrap().unwrap();
        assert_eq!(output[0]["content"][0]["text"], "receipt");
    }
    #[tokio::test]
    async fn successful_http_with_a_non_stream_body_is_not_model_success() {
        use axum::{Router, routing::post};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = Router::new().route(
            "/responses",
            post(|| async {
                (
                    [("content-type", "application/json")],
                    "data: {\"type\":\"response.completed\",\"response\":{\"output\":[]}}\n\n",
                )
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let config = ModelConfig {
            api_key: String::new(),
            base_url: format!("http://{address}"),
            model: "fixture".into(),
            provider: "compatible".into(),
            protocol: "responses".into(),
            proxy_url: String::new(),
            verified_at_ms: 0,
        };
        let result = request(&config, json!({"input":[]}), &CancellationToken::new()).await;
        server.abort();
        assert!(result.unwrap_err().contains("text/event-stream"));
    }
    #[tokio::test]
    async fn full_endpoint_addresses_are_not_appended_twice() {
        use axum::{Router, routing::post};
        for (protocol, suffix, body) in [
            (
                "responses",
                "responses",
                "data: {\"type\":\"response.completed\",\"response\":{\"output\":[{\"type\":\"message\",\"content\":[{\"text\":\"receipt\"}]}]}}\n\n",
            ),
            (
                "chat-completions",
                "chat/completions",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"receipt\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let router = Router::new().route(
                &format!("/v1/{suffix}"),
                post(move || async move {
                    ([("content-type", "text/event-stream; charset=utf-8")], body)
                }),
            );
            let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
            let config = ModelConfig {
                api_key: String::new(),
                base_url: format!("http://{address}/v1/{suffix}"),
                model: "fixture".into(),
                provider: "compatible".into(),
                protocol: protocol.into(),
                proxy_url: String::new(),
                verified_at_ms: 0,
            };
            let output = request(&config, json!({"input":[]}), &CancellationToken::new()).await;
            server.abort();
            assert_eq!(text(&output.unwrap()), "receipt");
        }
    }
    #[test]
    fn partial_and_failed_streams_never_produce_tools() {
        let mut p = ResponseStream::default();
        assert!(
            p.feed(
                b"data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{}\"}\n\n"
            )
            .unwrap()
            .is_none()
        );
        assert!(
            p.feed(b"data: {\"type\":\"response.incomplete\"}\n\n")
                .is_err()
        );
    }
    #[tokio::test]
    #[ignore = "real provider capability test; uses private local configuration, no phone or chat writes"]
    async fn live_model_capabilities() {
        let root = std::env::var("AI_PHONE_LIVE_TEST_ROOT").expect("explicit live test root");
        let store = crate::settings::SettingsStore::open(std::path::Path::new(&root)).unwrap();
        let config = store.get(store.current()).unwrap();
        let checks = probe(&config).await;
        for check in &checks {
            println!(
                "{}: {} ({}ms) {}",
                check.name, check.passed, check.duration_ms, check.detail
            );
        }
        assert!(
            checks.iter().all(|c| c.passed),
            "Live capability test failed"
        );
    }
}
