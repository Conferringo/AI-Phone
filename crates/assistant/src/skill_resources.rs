//! PostgreSQL owns Skill text; /skills is a recoverable, atomically published projection.
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use std::path::{Path, PathBuf};
use uuid::Uuid;

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}

async fn publication_lock(root: &Path, id: &str) -> Result<std::fs::File, String> {
    if !valid_id(id) {
        return Err("Skill资源编号无效".into());
    }
    let folder = root.join("data/skill-locks");
    let id = id.to_owned();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(folder.join(format!("{id}.lock")))
            .map_err(|e| e.to_string())?;
        file.lock().map_err(|e| e.to_string())?;
        Ok(file)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn resource_folder(root: &Path, id: &str) -> Result<PathBuf, String> {
    let base = root.join("data/runtime-skills");
    std::fs::create_dir_all(&base).map_err(|e| e.to_string())?;
    sync_directory(&root.join("data"))?;
    let folder = base.join(id);
    let staging = root.join("data/skill-staging").join(id);
    restore_staging(&staging, &folder, &base)?;
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    sync_directory(&base)?;
    if std::fs::symlink_metadata(&folder)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Skill资源目录不能是符号链接".into());
    }
    Ok(folder)
}

fn restore_staging(staging: &Path, folder: &Path, base: &Path) -> Result<(), String> {
    if !folder.exists() && staging.is_dir() {
        std::fs::rename(staging, folder).map_err(|e| e.to_string())?;
        sync_directory(base)?;
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), String> {
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .sync_all()
        .map_err(|e| e.to_string())
}

fn atomic_text(folder: &Path, content: &str) -> Result<(), String> {
    use std::io::Write;
    let destination = folder.join("SKILL.md");
    let regular = std::fs::symlink_metadata(&destination).is_ok_and(|metadata| metadata.is_file());
    if regular && std::fs::read(&destination).is_ok_and(|bytes| bytes == content.as_bytes()) {
        return Ok(());
    }
    let temporary = folder.join(format!(".SKILL-{}.tmp", Uuid::now_v7()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(content.as_bytes())
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        std::fs::rename(&temporary, &destination).map_err(|e| e.to_string())?;
        sync_directory(folder)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Serialize this local projection, reading committed text after acquiring its lock.
/// The blocking writer owns that lock until every file operation has finished,
/// including when its async caller is canceled. Disk I/O never holds a SQL transaction.
pub(super) async fn publish(db: &PgPool, root: &Path, id: &str) -> Result<PgRow, String> {
    let lock = publication_lock(root, id).await?;
    let row = sqlx::query("SELECT * FROM runtime_skills WHERE id=$1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Skill不存在")?;
    let root = root.to_owned();
    let id = id.to_owned();
    let content: String = row.get("content");
    tokio::task::spawn_blocking(move || {
        let _lock = lock;
        let folder = resource_folder(&root, &id)?;
        atomic_text(&folder, &content)
    })
    .await
    .map_err(|e| e.to_string())??;
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
