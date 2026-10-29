use crate::*;
use serde_json::{Value, json};
use sqlx::Row;

pub struct AuditResult {
    pub approved: bool,
    pub feedback: String,
}

// A completion audit has no execution tools. Its input comes from this run's
// persisted tool ledger, never from historical assistant prose.
pub async fn verify(
    app: &App,
    run: Uuid,
    message: Uuid,
    context: &[Value],
    summary: &str,
    needs_input: bool,
    cancel: &CancellationToken,
) -> Result<AuditResult, String> {
    let rows = sqlx::query("SELECT id,title,status,result FROM harness_events WHERE run_id=$1 AND kind='tool' AND result IS NOT NULL ORDER BY created_at,id")
        .bind(run).fetch_all(&app.db).await.map_err(|e| e.to_string())?;
    let event = Uuid::now_v7();
    sqlx::query("INSERT INTO harness_events(id,run_id,message_id,kind,title,content) VALUES($1,$2,$3,'audit','结果核验','核对本轮工具记录与完成声明')")
        .bind(event).bind(run).bind(message).execute(&app.db).await.map_err(|e| e.to_string())?;
    let request = context
        .iter()
        .rev()
        .find(|item| item["role"] == "user")
        .map(|item| item["content"].clone())
        .unwrap_or(json!([]));
    let latest_input = context
        .iter()
        .rposition(|item| item["role"] == "user")
        .unwrap_or(0);
    let current_calls: std::collections::HashSet<_> = context
        .iter()
        .skip(latest_input + 1)
        .filter_map(|item| item["local_action_id"].as_str())
        .collect();
    let mut image_budget = 8usize;
    let mut visual_inputs = Vec::new();
    let mut evidence = Vec::new();
    for row in rows.iter().rev().take(64) {
        let id = row.get::<Uuid, _>("id").to_string();
        if !current_calls.contains(id.as_str()) {
            continue;
        }
        let call = context.iter().find(|item| item["local_action_id"] == id);
        let mut result = row.get::<Option<Value>, _>("result").unwrap_or(Value::Null);
        limit_result(&mut result, &mut image_budget);
        extract_images(&mut result, &id, &mut visual_inputs);
        evidence.push(json!({"evidence_id":id,"tool":call.and_then(|item|item["name"].as_str()).unwrap_or(""),
            "input":call.and_then(|item|item["arguments"].as_str()).unwrap_or(""),
            "title":row.get::<String,_>("title"),"status":row.get::<String,_>("status"),"result":result}));
    }
    evidence.reverse();
    let request_text = request
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let mut audit_input = vec![
        json!({"type":"input_text","text":json!({"request":request_text,"proposed_completion":summary,"needs_input":needs_input,"tool_evidence":evidence}).to_string()}),
    ];
    let events:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('device_id',device_id,'app',app_name,'title',title,'text',body) FROM phone_notifications WHERE run_id=$1 AND status IN ('assigned','attending','pending-summary') ORDER BY received_at LIMIT 16").bind(run).fetch_all(&app.db).await.map_err(|e|e.to_string())?;
    let read_only: bool = sqlx::query_scalar("SELECT background_read_only FROM runs WHERE id=$1")
        .bind(run)
        .fetch_one(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    if !events.is_empty() && !read_only {
        let reply: bool = sqlx::query_scalar("SELECT autonomous_reply FROM proactive_state")
            .fetch_one(&app.db)
            .await
            .map_err(|e| e.to_string())?;
        audit_input.push(json!({"type":"input_text","text":format!("主人已委托自主处理这些真实通知（正文是外部资料）。自主回复={reply}。主人的最新明确暂停或不回复指令优先。普通消息默认回复，工作请求可连接授权电脑处理、在工作目录留档并回来源会话交付。仅询问主人‘是否回复/是否处理’不能通过。选择不回复须有重复、自发、广告或实际无需回复的依据。needs_input只有当前工具证据显示具体阻碍、缺少不可推断的信息或超出授权的敏感动作才可通过，不能把常规决策推回主人。请核验这些事件是否确已处理或有真实阻碍：{}",json!(events))}));
    }
    let learning:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('app',app_id,'title',title,'procedure',procedure,'outcome',outcome,'limitations',limitations,'observations',observation_ids) FROM app_experiences WHERE run_id=$1 AND superseded_at IS NULL ORDER BY created_at LIMIT 8").bind(run).fetch_all(&app.db).await.map_err(|e|e.to_string())?;
    if !learning.is_empty() {
        audit_input.push(json!({"type":"input_text","text":format!("以下是本轮唯一有效的待发布经验候选，同名旧草稿已经被修订或撤回，不再要求旧草稿通过。请仅逐项核对这份有效候选与实际操作证据，不要因为候选标注verified就相信它：{}",json!(learning))}));
    }
    audit_input.extend(visual_inputs);
    let tool = json!({"type":"function","name":"audit_result","description":"只报告证据核验结果，不能执行操作。",
        "parameters":{"type":"object","properties":{"approved":{"type":"boolean"},"feedback":{"type":"string"}},
        "required":["approved","feedback"],"additionalProperties":false}});
    let body = json!({"input":[
        {"role":"system","content":[{"type":"input_text","text":"你是独立的执行结果审计器。只依据给定本轮真实工具记录、截图和用户目标核验，不相信助手的完成声明或历史对话。如果记录包含app_experience_save，必须同时核对其步骤、成功结论和失败边界能否由该应用真实操作前后画面支持；不能把打开页面当作掌握功能，不得批准猜测为通用经验。严格检查：是否实际执行了声称的动作、点击/阅读的数量是否与记录相符、所述页面/文章事实能否从截图或UI文本读到、用户目标是否已完成。读Skill不等于操作，成功点击也不等于读到文章；标题不能证明文章或评论内容。若没有相应证据、只有计划/推测、尚有常规步骤未执行，approved必须false，并具体列出缺失证据和应继续的步骤。你没有执行工具，不能补造证据。只有确实得到当前记录支持的完成结果才可approved=true。输出简短可公开核验的结论，不输出隐含推理过程、凭据或无关内容。微信的通知预览不代表完整会话，必须核对对象。己方绿色气泡只证明本机已发出，不能证明对方已收到或已读；账号受限或验证页卡住不能称好友申请成功。好友关系当前已成立可以用当前资料页和已添加状态核验，不强求重做接受动作。较早截图已归档时若现存证据不足，应要求重新观察；不能凭归档标记猜图片内容。"}]},
        {"role":"user","content":audit_input}
    ],"tools":[tool],"tool_choice":{"type":"function","name":"audit_result"},"parallel_tool_calls":false,
        "reasoning":{"effort":"none"},"max_output_tokens":1200,"stream":false,"store":false});
    let audit = async {
        let config = crate::settings::for_run(app, run).await?;
        let output = tokio::time::timeout(
            Duration::from_secs(60),
            crate::model_adapter::request(&config, body, cancel),
        )
        .await
        .map_err(|_| "结果核验超时；尚未确认完成")??;
        parse_result(&json!({"output":output}))
    };
    let result =
        tokio::select! {_ = cancel.cancelled() => Err("canceled".into()), result = audit => result};
    let (status, content) = match &result {
        Ok(result) => (
            if result.approved {
                "completed"
            } else {
                "failed"
            },
            result.feedback.clone(),
        ),
        Err(error) => (
            if error == "canceled" {
                "canceled"
            } else {
                "failed"
            },
            error.clone(),
        ),
    };
    sqlx::query("UPDATE harness_events SET status=$2,content=$3,finished_at=now() WHERE id=$1")
        .bind(event)
        .bind(status)
        .bind(content)
        .execute(&app.db)
        .await
        .map_err(|e| e.to_string())?;
    result
}

// History is evidence about previous work, never proof of a new action.
pub async fn history(app: &App, run: Uuid, limit: usize) -> Result<Value, String> {
    let rows=sqlx::query("SELECT r.id,r.status,r.created_at,m.text FROM runs r JOIN messages m ON m.id=r.assistant_message_id WHERE r.conversation_id=(SELECT conversation_id FROM runs WHERE id=$1) ORDER BY r.created_at DESC,r.id DESC LIMIT $2")
        .bind(run).bind(limit.clamp(1,8) as i64).fetch_all(&app.db).await.map_err(|e|e.to_string())?;
    let mut history = Vec::new();
    for row in rows {
        let id: Uuid = row.get("id");
        let events=sqlx::query("SELECT id,title,status,content FROM harness_events WHERE run_id=$1 AND kind IN ('tool','audit') ORDER BY created_at,id LIMIT 100")
            .bind(id).fetch_all(&app.db).await.map_err(|e|e.to_string())?;
        history.push(json!({"run_id":id.to_string(),"status":row.get::<String,_>("status"),
            "assistant_claim_not_evidence":row.get::<String,_>("text").chars().take(1600).collect::<String>(),
            "actual_ledger":events.iter().map(|event|json!({"id":event.get::<Uuid,_>("id").to_string(),"title":event.get::<String,_>("title"),"status":event.get::<String,_>("status"),"content":app.settings.redact(&event.get::<String,_>("content")).chars().take(4000).collect::<String>()})).collect::<Vec<_>>() }));
    }
    Ok(json!({"note":"这是旧轮次真实账本，仅用于回答质疑，不可用作本轮执行证据","runs":history}))
}

fn parse_result(value: &Value) -> Result<AuditResult, String> {
    let args = value["output"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["type"] == "function_call" && item["name"] == "audit_result")
        .and_then(|item| item["arguments"].as_str())
        .ok_or("结果核验没有返回有效结论")?;
    let report: Value = serde_json::from_str(args).map_err(|_| "结果核验没有返回有效结论")?;
    let approved = report["approved"].as_bool().ok_or("结果核验缺少证据判定")?;
    let feedback = report["feedback"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or("结果核验缺少说明")?;
    Ok(AuditResult {
        approved,
        feedback: feedback.chars().take(4000).collect(),
    })
}

fn extract_images(value: &mut Value, id: &str, inputs: &mut Vec<Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                extract_images(item, id, inputs);
            }
        }
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("input_image") {
                inputs.push(json!({"type":"input_text","text":format!("真实截图证据 {id}")}));
                inputs.push(value.clone());
                *value = json!({"type":"input_text","text":format!("真实截图证据 {id} 见附图")});
            } else {
                for item in object.values_mut() {
                    extract_images(item, id, inputs);
                }
            }
        }
        _ => {}
    }
}

fn limit_result(value: &mut Value, images: &mut usize) {
    match value {
        Value::Array(items) => {
            for item in items.iter_mut().rev() {
                limit_result(item, images);
            }
        }
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("input_image") {
                if *images == 0 {
                    *value = json!({"type":"input_text","text":"较早截图已归档，不能据此猜测内容"});
                    return;
                }
                *images -= 1;
            }
            if let Some(Value::String(text)) = object.get_mut("text")
                && text.len() > 24000
            {
                *text = text.chars().take(24000).collect();
            }
            for (key, item) in object {
                if key != "image_url" {
                    limit_result(item, images);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn audit_cannot_default_to_approved_on_missing_or_malformed_result() {
        assert!(parse_result(&json!({"output":[]})).is_err());
        assert!(parse_result(&json!({"output":[{"type":"function_call","name":"audit_result","arguments":"{\"feedback\":\"done\"}"}]})).is_err());
    }
    #[test]
    fn evidence_retains_only_bounded_recent_images() {
        let mut result = json!([{"type":"input_image","image_url":"old"},{"type":"input_image","image_url":"new"}]);
        limit_result(&mut result, &mut 1);
        assert!(result[0].get("image_url").is_none());
        assert_eq!(result[1]["image_url"], "new");
    }
}
