//! PostgreSQL owns Skill text; /skills is a recoverable, atomically published projection.
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

async fn resource_folder(root: &Path, id: &str) -> Result<PathBuf, String> {
    if !valid_id(id) {
        return Err("Skill资源编号无效".into());
    }
    let base = root.join("data/runtime-skills");
    tokio::fs::create_dir_all(&base)
        .await
        .map_err(|e| e.to_string())?;
    sync_directory(&root.join("data")).await?;
    let folder = base.join(id);
    let staging = root.join("data/skill-staging").join(id);
    if !folder.exists() && staging.is_dir() {
        tokio::fs::rename(&staging, &folder)
            .await
            .map_err(|e| e.to_string())?;
        sync_directory(&base).await?;
    }
    tokio::fs::create_dir_all(&folder)
        .await
        .map_err(|e| e.to_string())?;
    sync_directory(&base).await?;
    if tokio::fs::symlink_metadata(&folder)
        .await
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Skill资源目录不能是符号链接".into());
    }
    Ok(folder)
}

async fn sync_directory(path: &Path) -> Result<(), String> {
    tokio::fs::File::open(path)
        .await
        .map_err(|e| e.to_string())?
        .sync_all()
        .await
        .map_err(|e| e.to_string())
}

async fn atomic_text(folder: &Path, content: &str) -> Result<(), String> {
    let destination = folder.join("SKILL.md");
    let regular = tokio::fs::symlink_metadata(&destination)
        .await
        .is_ok_and(|metadata| metadata.is_file());
    if regular
        && tokio::fs::read(&destination)
            .await
            .is_ok_and(|bytes| bytes == content.as_bytes())
    {
        return Ok(());
    }
    let temporary = folder.join(format!(".SKILL-{}.tmp", Uuid::now_v7()));
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await
            .map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        tokio::fs::rename(&temporary, &destination)
            .await
            .map_err(|e| e.to_string())?;
        sync_directory(folder).await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

/// Publish only a committed row. The lock orders concurrent publishers/edits;
/// this transaction never changes the authoritative text, so rollback is harmless.
pub(super) async fn publish(db: &PgPool, root: &Path, id: &str) -> Result<PgRow, String> {
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;
    let row =
        sqlx::query("SELECT * FROM runtime_skills WHERE id=$1 AND deleted_at IS NULL FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("Skill不存在")?;
    let folder = resource_folder(root, id).await?;
    atomic_text(&folder, row.get::<String, _>("content").as_str()).await?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(row)
}

pub(super) async fn commit(
    tx: Transaction<'_, Postgres>,
    db: &PgPool,
    root: &Path,
    id: &str,
) -> Result<PgRow, String> {
    if let Err(error) = tx.commit().await {
        // COMMIT may have succeeded before the connection failed. Re-read durable
        // state; never overwrite the resource with the caller's previous version.
        let _ = publish(db, root, id).await;
        return Err(format!(
            "Skill保存结果待核对，请重新读取后核对版本：{error}"
        ));
    }
    publish(db, root, id)
        .await
        .map_err(|error| format!("Skill正文已保存，资源同步失败；修复后重新读取即可恢复：{error}"))
}

pub(super) async fn reconcile(db: &PgPool, root: &Path) -> Result<(), String> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM runtime_skills WHERE deleted_at IS NULL ORDER BY id")
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())?;
    for id in ids {
        publish(db, root, &id).await?;
    }
    Ok(())
}

/// Complete staged resources before creating their durable database reference.
pub(super) async fn seal_staging(folder: &Path) -> Result<(), String> {
    let folder = folder.to_owned();
    tokio::task::spawn_blocking(move || {
        fn sync_tree(path: &Path) -> std::io::Result<()> {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    sync_tree(&entry.path())?;
                } else {
                    std::fs::File::open(entry.path())?.sync_all()?;
                }
            }
            std::fs::File::open(path)?.sync_all()
        }
        sync_tree(&folder)?;
        if let Some(parent) = folder.parent() {
            std::fs::File::open(parent)?.sync_all()?;
            if let Some(data) = parent.parent() {
                std::fs::File::open(data)?.sync_all()?;
            }
        }
        Ok::<_, std::io::Error>(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
