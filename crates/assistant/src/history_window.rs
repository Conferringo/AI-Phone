//! Bounded read snapshots. Ordinals locate a page; they are not commit cursors.
use crate::{Uuid, wire};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::BTreeSet;

pub(crate) const MAX_PAGE: u32 = 80;
const MAX_WATCH: usize = 8;
const MAX_RECEIPTS: usize = 32;

#[derive(Debug)]
pub(crate) struct Request {
    limit: u32,
    before: Option<Uuid>,
    watch: Vec<Uuid>,
    receipts: Vec<Uuid>,
}

fn identifiers(values: &[String], maximum: usize) -> Result<Vec<Uuid>, &'static str> {
    if values.len() > maximum {
        return Err("history_window_limit");
    }
    values
        .iter()
        .map(|value| Uuid::parse_str(value).map_err(|_| "history_window_identifier"))
        .collect::<Result<BTreeSet<_>, _>>()
        .map(|values| values.into_iter().collect())
}

impl Request {
    pub(crate) fn parse(value: &wire::ChatHistoryRequest) -> Result<Option<Self>, &'static str> {
        let before = if value.before_message_id.is_empty() {
            None
        } else {
            Some(
                Uuid::parse_str(&value.before_message_id)
                    .map_err(|_| "history_window_identifier")?,
            )
        };
        let watch = identifiers(&value.watch_message_ids, MAX_WATCH)?;
        let receipts = identifiers(&value.receipt_request_ids, MAX_RECEIPTS)?;
        if value.limit == 0 {
            return if before.is_some() || !watch.is_empty() || !receipts.is_empty() {
                Err("history_window_limit")
            } else {
                Ok(None)
            };
        }
        if value.limit > MAX_PAGE {
            return Err("history_window_limit");
        }
        Ok(Some(Self {
            limit: value.limit,
            before,
            watch,
            receipts,
        }))
    }
}

pub(crate) async fn transaction(pool: &PgPool) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
    pool.begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .await
}

async fn snapshot(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    input: Request,
) -> Result<wire::ChatState, sqlx::Error> {
    let current_run = crate::http::CURRENT_RUN;
    let statement = format!(
        "WITH current_run AS MATERIALIZED ({current_run}),
        anchor AS (
            SELECT ordinal FROM messages WHERE conversation_id=$1 AND id=$3
        ),
        page AS MATERIALIZED (
            SELECT id,ordinal FROM messages
            WHERE conversation_id=$1 AND ordinal <= COALESCE(
                (SELECT ordinal-1 FROM anchor), 9223372036854775807::bigint)
            ORDER BY ordinal DESC LIMIT ($2::bigint + 1)
        ),
        page_ids AS (
            SELECT id,ordinal FROM page ORDER BY ordinal DESC LIMIT $2
        ),
        heads AS (
            SELECT assistant_message_id AS id FROM current_run
            UNION ALL
            SELECT assistant_message_id FROM runs WHERE conversation_id=$1 AND origin='user' AND status IN ('queued','running','paused')
            UNION ALL
            SELECT assistant_message_id FROM runs WHERE conversation_id=$1 AND origin='exploration' AND status IN ('queued','running','paused')
            UNION ALL
            SELECT assistant_message_id FROM runs WHERE conversation_id=$1 AND origin='notification' AND status IN ('queued','running')
            UNION ALL
            SELECT assistant_message_id FROM runs WHERE conversation_id=$1 AND origin='device-handoff' AND status IN ('queued','running','paused')
        ),
        selected AS (
            SELECT id FROM page_ids
            UNION SELECT id FROM heads
            UNION SELECT unnest($4::uuid[])
            UNION SELECT id FROM messages WHERE conversation_id=$1 AND role='user' AND request_id=ANY($5)
        )
        SELECT current_run.*,
            ($3::uuid IS NULL OR EXISTS(SELECT 1 FROM anchor)) anchor_ok,
            COALESCE((SELECT ordinal FROM messages WHERE conversation_id=$1 ORDER BY ordinal DESC LIMIT 1),0) revision,
            (SELECT count(*) FROM page) > $2 has_older,
            ARRAY(SELECT id FROM page_ids ORDER BY ordinal) page_ids,
            ARRAY(SELECT id FROM selected) selected_ids
        FROM (SELECT 1) snapshot LEFT JOIN current_run ON true"
    );
    let row = sqlx::query(&statement)
        .bind(conversation)
        .bind(i64::from(input.limit))
        .bind(input.before)
        .bind(&input.watch)
        .bind(&input.receipts)
        .fetch_one(&mut **tx)
        .await?;
    if !row.get::<bool, _>("anchor_ok") {
        return Err(sqlx::Error::Protocol("history_anchor_scope".into()));
    }
    let page: Vec<Uuid> = row.get("page_ids");
    let window = wire::ChatMessageWindow {
        kind: if input.before.is_some() {
            wire::ChatWindowKind::Before
        } else {
            wire::ChatWindowKind::Tail
        } as i32,
        next_before_message_id: page.first().map(Uuid::to_string).unwrap_or_default(),
        has_older: row.get("has_older"),
        page_message_ids: page.iter().map(Uuid::to_string).collect(),
    };
    let ids: Vec<Uuid> = row.get("selected_ids");
    let messages =
        crate::http::chat_messages(tx, conversation, crate::http::MessageSelection::Ids(&ids))
            .await?;
    let run = row.get::<Option<Uuid>, _>("id").map(|_| &row);
    Ok(crate::http::chat_state(
        conversation,
        row.get::<i64, _>("revision") as u64,
        messages,
        Some(window),
        run,
    ))
}

pub(crate) async fn read(
    pool: &PgPool,
    conversation: Uuid,
    input: Request,
) -> Result<wire::ChatState, sqlx::Error> {
    let mut tx = transaction(pool).await?;
    let value = snapshot(&mut tx, conversation, input).await?;
    tx.commit().await?;
    Ok(value)
}

#[cfg(test)]
#[path = "history_window_tests.rs"]
mod tests;
