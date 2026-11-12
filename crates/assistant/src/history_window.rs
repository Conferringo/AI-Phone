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

async fn anchor(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    before: Option<Uuid>,
) -> Result<Option<i64>, sqlx::Error> {
    let Some(before) = before else {
        return Ok(None);
    };
    let value =
        sqlx::query_scalar("SELECT ordinal FROM messages WHERE id=$1 AND conversation_id=$2")
            .bind(before)
            .bind(conversation)
            .fetch_optional(&mut **tx)
            .await?;
    value
        .map(Some)
        .ok_or_else(|| sqlx::Error::Protocol("history_anchor_scope".into()))
}

async fn page(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    input: &Request,
) -> Result<(Vec<Uuid>, bool), sqlx::Error> {
    let before = anchor(tx, conversation, input.before).await?;
    // Two statement shapes retain indexed keyset plans; no nullable-OR predicate.
    let sql = match before {
        Some(_) => {
            "SELECT id FROM messages WHERE conversation_id=$1 AND ordinal<$3 ORDER BY ordinal DESC LIMIT $2"
        }
        None => "SELECT id FROM messages WHERE conversation_id=$1 ORDER BY ordinal DESC LIMIT $2",
    };
    let query = sqlx::query_scalar(sql)
        .bind(conversation)
        .bind(i64::from(input.limit) + 1);
    let query = match before {
        Some(before) => query.bind(before),
        None => query,
    };
    let mut rows: Vec<Uuid> = query.fetch_all(&mut **tx).await?;
    let limit = input.limit as usize;
    let has_older = rows.len() > limit;
    rows.truncate(limit);
    rows.reverse();
    Ok((rows, has_older))
}

async fn heads(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    current_head: Option<Uuid>,
) -> Result<Vec<Uuid>, sqlx::Error> {
    // Each lane has an explicit LIMIT. Paused notification heads are not a lane
    // union: only the one selected current header may add such a historical head.
    let mut ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT assistant_message_id FROM (
          (SELECT assistant_message_id FROM runs WHERE conversation_id=$1 AND status IN ('queued','running','paused') AND origin NOT IN ('notification','exploration','desktop-exploration') ORDER BY created_at DESC,id DESC LIMIT 1)
          UNION
          (SELECT assistant_message_id FROM runs WHERE conversation_id=$1 AND status IN ('queued','running','paused') AND origin IN ('exploration','desktop-exploration') ORDER BY created_at DESC,id DESC LIMIT 1)
          UNION
          (SELECT assistant_message_id FROM runs WHERE conversation_id=$1 AND status IN ('queued','running') AND origin='notification' ORDER BY created_at DESC,id DESC LIMIT 1)
        ) selected_heads"
    ).bind(conversation).fetch_all(&mut **tx).await?;
    ids.extend(current_head);
    Ok(ids)
}

async fn receipts(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    input: &Request,
) -> Result<Vec<Uuid>, sqlx::Error> {
    if input.receipts.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_scalar(
        "SELECT id FROM messages WHERE conversation_id=$1 AND role='user' AND request_id=ANY($2)",
    )
    .bind(conversation)
    .bind(&input.receipts)
    .fetch_all(&mut **tx)
    .await
}

async fn selection(
    tx: &mut Transaction<'_, Postgres>,
    conversation: Uuid,
    input: &Request,
    current_head: Option<Uuid>,
) -> Result<(Vec<Uuid>, wire::ChatMessageWindow), sqlx::Error> {
    let (page, has_older) = page(tx, conversation, input).await?;
    let window = wire::ChatMessageWindow {
        kind: if input.before.is_some() {
            wire::ChatWindowKind::Before
        } else {
            wire::ChatWindowKind::Tail
        } as i32,
        next_before_message_id: page.first().map(Uuid::to_string).unwrap_or_default(),
        has_older,
        page_message_ids: page.iter().map(Uuid::to_string).collect(),
    };
    let mut ids: BTreeSet<Uuid> = page.into_iter().collect();
    ids.extend(heads(tx, conversation, current_head).await?);
    ids.extend(input.watch.iter().copied());
    ids.extend(receipts(tx, conversation, input).await?);
    Ok((ids.into_iter().collect(), window))
}

pub(crate) async fn read(
    pool: &PgPool,
    conversation: Uuid,
    input: Request,
) -> Result<wire::ChatState, sqlx::Error> {
    let mut tx = transaction(pool).await?;
    let run = crate::http::current_run(&mut tx, conversation).await?;
    let current_head = run.as_ref().map(|row| row.get("assistant_message_id"));
    let (ids, window) = selection(&mut tx, conversation, &input, current_head).await?;
    let value =
        crate::http::state_selected(&mut tx, conversation, Some(&ids), Some(window), run).await?;
    tx.commit().await?;
    Ok(value)
}

#[cfg(test)]
#[path = "history_window_tests.rs"]
mod tests;
