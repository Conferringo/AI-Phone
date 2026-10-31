//! Presentation of operational notices is separate from model-authored dialogue.
//! Legacy records are projected, never rewritten or deleted.
use crate::wire::RunNoticeKind;
use serde_json::Value;
use sqlx::{PgPool, Row};

pub fn kind(status: &str, error: &str, state: &Value) -> RunNoticeKind {
    if error.is_empty() {
        return RunNoticeKind::Unspecified;
    }
    match state["notice_kind"].as_str() {
        Some("system_failure") => return RunNoticeKind::SystemFailure,
        Some("needs_input") => return RunNoticeKind::NeedsInput,
        _ => {}
    }
    if legacy_failure(status, error, state) {
        RunNoticeKind::SystemFailure
    } else if status == "paused" {
        RunNoticeKind::NeedsInput
    } else {
        RunNoticeKind::Unspecified
    }
}

fn legacy_failure(status: &str, error: &str, state: &Value) -> bool {
    let issue = error.strip_prefix("需要补充：").unwrap_or(error);
    let model_failure = state["model_retries"].as_u64().unwrap_or(0) > 0
        && state["last_issue"].as_str() == Some(issue);
    let execution_failure = state["stalls"].as_u64().unwrap_or(0) >= 3
        || state["observation_recoveries"].as_u64().unwrap_or(0) >= 4
        || error.starts_with("需要核对：")
        || error == "你已接管手机，任务已暂停。";
    status == "failed" || model_failure || execution_failure
}

/// Cancellation used to replace runs.error after the pause was published. The
/// recorded counter and exact last_issue reconstruct that legacy notice without
/// treating arbitrary assistant prose containing an error as system output.
pub fn legacy_pause(text: &str, state: &Value) -> bool {
    if state.get("notice_kind").is_some() {
        return false;
    }
    let Some(issue) = state["last_issue"].as_str() else {
        return false;
    };
    let prefix = if state["observation_recoveries"].as_u64().unwrap_or(0) >= 4 {
        "已重新获取手机画面4次仍未恢复，任务保留并暂停。"
    } else if state["stalls"].as_u64().unwrap_or(0) >= 3 {
        "连续三轮没有新增执行进展，任务已暂停。"
    } else {
        return false;
    };
    text == format!("{prefix}{issue}")
}

pub fn message(
    role: &str,
    text: &str,
    run_error: &str,
    kind: RunNoticeKind,
    recorded_retry: bool,
) -> (String, String) {
    let exact_fault = kind == RunNoticeKind::SystemFailure
        && (text == run_error || Some(text) == run_error.strip_prefix("需要补充："));
    if role == "assistant" && !text.is_empty() && (recorded_retry || exact_fault) {
        (String::new(), text.into())
    } else {
        (text.into(), String::new())
    }
}

pub fn event_kind(kind: &str, title: &str, status: &str) -> String {
    if kind == "summary" && title == "连接重试" && status == "failed" {
        "system_error".into()
    } else {
        kind.into()
    }
}

pub async fn background_error(
    db: &PgPool,
    device: &str,
    error: &str,
) -> Result<String, sqlx::Error> {
    if error.is_empty()
        || matches!(
            error,
            "已停止自主探索" | "你已接管手机，自主探索已暂停" | "你已接管电脑，自主探索已暂停"
        )
    {
        return Ok(String::new());
    }
    let run = sqlx::query("SELECT status,loop_state FROM runs WHERE device_id=$1 AND origin IN ('exploration','desktop-exploration','notification') AND error=$2 ORDER BY created_at DESC,id DESC LIMIT 1")
        .bind(device).bind(error).fetch_optional(db).await?;
    if run.as_ref().is_some_and(|run| {
        kind(run.get("status"), error, &run.get::<Value, _>("loop_state"))
            == RunNoticeKind::NeedsInput
    }) {
        return Ok(String::new());
    }
    Ok(error.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn provider_errors_are_not_dialogue_and_legacy_data_is_not_rewritten() {
        let text = "模型服务返回 HTTP 402；请检查模型 ID、密钥和服务权限";
        let error = format!("需要补充：{text}");
        let state = json!({"model_retries":1,"last_issue":text});
        let classified = kind("paused", &error, &state);
        assert_eq!(classified, RunNoticeKind::SystemFailure);
        assert_eq!(
            message("assistant", text, &error, classified, false),
            (String::new(), text.into())
        );
        assert_eq!(state["last_issue"], text);
        assert_eq!(
            message(
                "assistant",
                "已读取表格，正在处理。",
                &error,
                classified,
                false
            ),
            ("已读取表格，正在处理。".into(), String::new())
        );
        assert_eq!(
            message("user", text, &error, classified, true),
            (text.into(), String::new())
        );
    }

    #[test]
    fn explicit_classification_wins_over_legacy_counters_and_exact_failures_are_retained() {
        assert_eq!(
            kind(
                "paused",
                "question",
                &json!({"notice_kind":"needs_input","stalls":3})
            ),
            RunNoticeKind::NeedsInput
        );
        assert_eq!(
            kind(
                "queued",
                "HTTP 429",
                &json!({"notice_kind":"system_failure"})
            ),
            RunNoticeKind::SystemFailure
        );
        assert_eq!(
            kind("failed", "transport error", &json!({})),
            RunNoticeKind::SystemFailure
        );
        assert_eq!(
            kind("paused", "no progress", &json!({"stalls":3})),
            RunNoticeKind::SystemFailure
        );
        assert_eq!(
            kind("paused", "需要核对：unknown", &json!({})),
            RunNoticeKind::SystemFailure
        );
        assert_eq!(
            kind("paused", "你已接管手机，任务已暂停。", &json!({})),
            RunNoticeKind::SystemFailure
        );
        assert_eq!(
            kind(
                "paused",
                "a real question",
                &json!({"model_retries":1,"last_issue":"different fault"})
            ),
            RunNoticeKind::NeedsInput
        );
        assert_eq!(
            kind("canceled", "ordinary state", &json!({})),
            RunNoticeKind::Unspecified
        );
    }

    #[test]
    fn canceled_legacy_pauses_require_exact_persisted_provenance() {
        let issue = "工具phone_observe失败：fixture";
        let recoveries = json!({"stalls":1,"observation_recoveries":4,"last_issue":issue});
        let text = format!("已重新获取手机画面4次仍未恢复，任务保留并暂停。{issue}");
        assert!(legacy_pause(&text, &recoveries));
        assert_eq!(
            kind("canceled", "模式已暂停", &recoveries),
            RunNoticeKind::SystemFailure
        );
        assert!(!legacy_pause("这是用户询问为何观察失败。", &recoveries));
        assert!(!legacy_pause(
            &text,
            &json!({"stalls":1,"observation_recoveries":4,"last_issue":"different"})
        ));
        assert!(!legacy_pause(
            &text,
            &json!({"notice_kind":"needs_input","stalls":1,"observation_recoveries":4,"last_issue":issue})
        ));
        assert!(!legacy_pause(
            &text,
            &json!({"stalls":1,"observation_recoveries":3,"last_issue":issue})
        ));
        assert!(!legacy_pause(&text, &json!({"observation_recoveries":4})));
        let stalled = format!("连续三轮没有新增执行进展，任务已暂停。{issue}");
        assert!(legacy_pause(
            &stalled,
            &json!({"stalls":3,"last_issue":issue})
        ));
    }

    #[test]
    fn questions_stay_dialogue_and_old_retry_notices_survive_run_continuation() {
        let text = "请提供收件人。";
        let error = format!("需要补充：{text}");
        let classified = kind("paused", &error, &json!({"notice_kind":"needs_input"}));
        assert_eq!(classified, RunNoticeKind::NeedsInput);
        assert_eq!(
            message("assistant", text, &error, classified, false),
            (text.into(), String::new())
        );
        assert_eq!(
            message(
                "assistant",
                "previous 402",
                "",
                RunNoticeKind::Unspecified,
                true
            ),
            (String::new(), "previous 402".into())
        );
        assert_eq!(
            kind("running", "", &json!({"notice_kind":"system_failure"})),
            RunNoticeKind::Unspecified
        );
        assert_eq!(event_kind("summary", "连接重试", "failed"), "system_error");
        assert_eq!(event_kind("tool", "连接重试", "failed"), "tool");
        assert_eq!(event_kind("summary", "连接重试", "completed"), "summary");
    }
}
