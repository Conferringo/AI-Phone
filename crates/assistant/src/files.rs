//! Shared file data plane for the harness and guest roots. GUI work remains device-use.
use crate::*;
use axum::extract::State;

use ai_phone_adb::{Client, Limits};
use std::path::{Component, Path};
use tokio::{fs, io::AsyncWriteExt};
type Result<T> = std::result::Result<T, String>;
const DESKTOP: &str = "ai-phone-desktop";
const IO_TIME: Duration = Duration::from_secs(600);
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn path(value: &str) -> Result<String> {
    if value.contains('\0') || value.len() > 4096 {
        return Err("无效路径".into());
    }
    let mut parts = Vec::new();
    for part in Path::new(value).components() {
        match part {
            Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            Component::RootDir | Component::CurDir => {}
            _ => return Err("无效路径".into()),
        }
    }
    Ok(format!("/{}", parts.join("/")))
}
fn name(value: &str) -> Result<&str> {
    if value.is_empty() || value == "." || value == ".." || value.contains(['/', '\\', '\0']) {
        return Err("无效文件名".into());
    }
    Ok(value)
}
fn join(base: &str, leaf: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), leaf)
}
fn modified(m: &std::fs::Metadata) -> u64 {
    m.modified()
        .ok()
        .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |v| v.as_millis() as u64)
}
#[derive(Clone)]
enum Store {
    Workspace(PathBuf),
    Phone(Client),
    Desktop,
}
impl Store {
    async fn open(app: &App, id: &str) -> Result<Self> {
        if id.is_empty() {
            let root = app.root.join("data/workspaces/admin");
            fs::create_dir_all(&root).await.map_err(err)?;
            return Ok(Self::Workspace(fs::canonicalize(root).await.map_err(err)?));
        }
        match crate::desktop::device_kind(app, id).await?.as_str() {
            "phone" => Ok(Self::Phone(
                Client::new(15037, crate::phone_system::target(app, id).await?.serial)
                    .map_err(err)?,
            )),
            "desktop" => Ok(Self::Desktop),
            _ => Err("设备不支持文件管理".into()),
        }
    }
    async fn local(&self, value: &str) -> Result<PathBuf> {
        let Self::Workspace(root) = self else {
            return Err("不是工作目录".into());
        };
        let p = path(value)?;
        let full = root.join(p.trim_start_matches('/'));
        let resolved = fs::canonicalize(&full).await.map_err(err)?;
        if !resolved.starts_with(root) {
            return Err("路径越过工作目录".into());
        }
        Ok(resolved)
    }
    async fn shell(&self, command: &str) -> Result<Vec<u8>> {
        match self {
            Self::Phone(client) => {
                let out = client
                    .shell(
                        command,
                        Limits {
                            stdout: 8 * 1024 * 1024,
                            stderr: 65536,
                        },
                        IO_TIME,
                    )
                    .await
                    .map_err(err)?;
                if out.code != 0 {
                    return Err(String::from_utf8_lossy(&out.stderr).into_owned());
                }
                Ok(out.stdout)
            }
            Self::Desktop => {
                let mut cmd = crate::desktop::docker();
                cmd.args(["exec", "-u", "0", DESKTOP, "sh", "-c", command]);
                crate::desktop::process(cmd, vec![], 8 * 1024 * 1024, 600).await
            }
            Self::Workspace(_) => Err("工作目录不运行系统命令".into()),
        }
    }
    async fn list(&self, value: &str) -> Result<wire::FileListing> {
        let mut directory = path(value)?;
        let mut entries = vec![];
        match self {
            Self::Workspace(root) => {
                let folder = self.local(&directory).await?;
                directory = format!(
                    "/{}",
                    folder.strip_prefix(root).map_err(err)?.to_string_lossy()
                );
                let mut read = fs::read_dir(folder).await.map_err(err)?;
                while let Some(item) = read.next_entry().await.map_err(err)? {
                    let m = item.metadata().await.map_err(err)?;
                    let name = item.file_name().to_string_lossy().into_owned();
                    entries.push(wire::FileEntry {
                        path: join(&directory, &name),
                        name,
                        directory: m.is_dir(),
                        size_bytes: m.len(),
                        modified_at_ms: modified(&m),
                        symlink: m.is_symlink(),
                    });
                }
            }
            Self::Phone(client) => {
                directory = String::from_utf8(
                    self.shell(&format!("cd {} && pwd -P", quote(&directory)))
                        .await?,
                )
                .map_err(err)?
                .trim_end_matches('\n')
                .into();
                for e in client
                    .list(&directory, Duration::from_secs(15))
                    .await
                    .map_err(err)?
                {
                    let kind = e.mode & 0o170000;
                    entries.push(wire::FileEntry {
                        path: join(&directory, &e.name),
                        name: e.name,
                        directory: kind == 0o040000,
                        size_bytes: e.bytes,
                        modified_at_ms: e.modified * 1000,
                        symlink: kind == 0o120000,
                    });
                }
            }
            Self::Desktop => {
                let output = self
                    .shell(&format!(
                        "cd {} && printf '%s\\0' \"$(pwd -P)\" && find . -mindepth 1 -maxdepth 1 -printf '%y\\0%s\\0%T@\\0%f\\0'",
                        quote(&directory)
                    ))
                    .await?;
                let fields: Vec<_> = output.split(|b| *b == 0).collect();
                directory = String::from_utf8(fields[0].to_vec()).map_err(err)?;
                for row in fields[1..fields.len() - 1].chunks_exact(4) {
                    let name = String::from_utf8(row[3].to_vec()).map_err(err)?;
                    entries.push(wire::FileEntry {
                        path: join(&directory, &name),
                        name,
                        directory: row[0] == b"d",
                        symlink: row[0] == b"l",
                        size_bytes: std::str::from_utf8(row[1])
                            .map_err(err)?
                            .parse()
                            .map_err(err)?,
                        modified_at_ms: (std::str::from_utf8(row[2])
                            .map_err(err)?
                            .parse::<f64>()
                            .map_err(err)?
                            * 1000.) as u64,
                    });
                }
            }
        }
        entries.sort_by(|a, b| b.directory.cmp(&a.directory).then(a.name.cmp(&b.name)));
        Ok(wire::FileListing {
            location: Some(wire::FileLocation {
                device_id: String::new(),
                path: directory,
            }),
            entries,
        })
    }
    async fn remove(&self, value: &str) -> Result<()> {
        let p = path(value)?;
        if p == "/" {
            return Err("不能删除根目录".into());
        }
        match self {
            Self::Workspace(_) => {
                let parent = Path::new(&p).parent().ok_or("无效路径")?.to_string_lossy();
                let leaf = Path::new(&p).file_name().ok_or("无效路径")?;
                let full = self.local(&parent).await?.join(leaf);
                let m = fs::symlink_metadata(&full).await.map_err(err)?;
                if m.is_dir() {
                    fs::remove_dir_all(full).await.map_err(err)
                } else {
                    fs::remove_file(full).await.map_err(err)
                }
            }
            _ => {
                self.shell(&format!("rm -rf -- {}", quote(&p))).await?;
                Ok(())
            }
        }
    }
    async fn mkdir(&self, value: &str) -> Result<()> {
        let p = path(value)?;
        match self {
            Self::Workspace(_) => {
                let parent = Path::new(&p).parent().ok_or("无效路径")?.to_string_lossy();
                fs::create_dir(
                    self.local(&parent)
                        .await?
                        .join(Path::new(&p).file_name().ok_or("无效路径")?),
                )
                .await
                .map_err(err)
            }
            _ => {
                self.shell(&format!("mkdir -- {}", quote(&p))).await?;
                Ok(())
            }
        }
    }
    async fn rename(&self, source: &str, target: &str) -> Result<()> {
        if path(source)? == "/" {
            return Err("不能移动根目录".into());
        }
        match self {
            Self::Workspace(_) => {
                let to = Path::new(target);
                let full = self
                    .local(&to.parent().ok_or("无效路径")?.to_string_lossy())
                    .await?
                    .join(to.file_name().ok_or("无效路径")?);
                if fs::try_exists(&full).await.map_err(err)? {
                    return Err("目标文件已存在".into());
                }
                fs::rename(self.local(source).await?, full)
                    .await
                    .map_err(err)
            }
            _ => {
                self.shell(&format!(
                    "test ! -e {to} && test ! -L {to} && mv -- {from} {to}",
                    to = quote(target),
                    from = quote(source)
                ))
                .await?;
                Ok(())
            }
        }
    }
    async fn copy_within(&self, source: &str, target: &str) -> Result<()> {
        match self {
            Self::Workspace(_) => {
                let p = Path::new(target);
                let to = self
                    .local(&p.parent().ok_or("无效路径")?.to_string_lossy())
                    .await?
                    .join(p.file_name().ok_or("无效路径")?);
                copy_local(&self.local(source).await?, &to).await
            }
            _ => {
                self.shell(&format!(
                    "test ! -e {to} && test ! -L {to} && cp -a -- {from} {to}",
                    to = quote(target),
                    from = quote(source)
                ))
                .await?;
                Ok(())
            }
        }
    }
    fn export<'a>(
        &'a self,
        source: &'a str,
        target: &'a Path,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Self::Workspace(_) => copy_local(&self.local(source).await?, target).await,
                Self::Desktop => {
                    let mut c = crate::desktop::docker();
                    c.args(["cp", &format!("{DESKTOP}:{source}")]).arg(target);
                    crate::desktop::process(c, vec![], 65536, 600).await?;
                    Ok(())
                }
                Self::Phone(client) => {
                    let parent = Path::new(source)
                        .parent()
                        .ok_or("无效路径")?
                        .to_string_lossy();
                    let leaf = Path::new(source)
                        .file_name()
                        .ok_or("不能复制根目录")?
                        .to_string_lossy();
                    let entry = client
                        .list(&parent, Duration::from_secs(15))
                        .await
                        .map_err(err)?
                        .into_iter()
                        .find(|e| e.name == leaf)
                        .ok_or("文件不存在")?;
                    pull_tree(client, source, target, entry.mode).await?;
                    Ok(())
                }
            }
        })
    }
    fn import<'a>(
        &'a self,
        source: &'a Path,
        target: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            match self {
                Self::Workspace(_) => {
                    let p = Path::new(target);
                    let full = self
                        .local(&p.parent().ok_or("无效路径")?.to_string_lossy())
                        .await?
                        .join(p.file_name().ok_or("无效路径")?);
                    copy_local(source, &full).await
                }
                Self::Desktop => {
                    self.shell(&format!(
                        "test ! -e {p} && test ! -L {p}",
                        p = quote(target)
                    ))
                    .await?;
                    let mut c = crate::desktop::docker();
                    c.arg("cp").arg(source).arg(format!("{DESKTOP}:{target}"));
                    crate::desktop::process(c, vec![], 65536, 600).await?;
                    Ok(())
                }
                Self::Phone(client) => {
                    let metadata = fs::symlink_metadata(source).await.map_err(err)?;
                    if metadata.is_dir() {
                        self.mkdir(target).await?;
                        let mut dir = fs::read_dir(source).await.map_err(err)?;
                        while let Some(item) = dir.next_entry().await.map_err(err)? {
                            self.import(
                                &item.path(),
                                &join(target, &item.file_name().to_string_lossy()),
                            )
                            .await?;
                        }
                    } else if metadata.is_file() {
                        self.shell(&format!(
                            "test ! -e {p} && test ! -L {p}",
                            p = quote(target)
                        ))
                        .await?;
                        let mut input = fs::File::open(source).await.map_err(err)?;
                        client
                            .push(
                                target,
                                &mut input,
                                (modified(&metadata) / 1000) as u32,
                                IO_TIME,
                            )
                            .await
                            .map_err(err)?;
                    } else {
                        return Err("仅支持复制普通文件和目录".into());
                    }
                    Ok(())
                }
            }
        })
    }
}
fn pull_tree<'a>(
    client: &'a Client,
    source: &'a str,
    target: &'a Path,
    mode: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        match mode & 0o170000 {
            0o040000 => {
                fs::create_dir(target).await.map_err(err)?;
                for child in client
                    .list(source, Duration::from_secs(15))
                    .await
                    .map_err(err)?
                {
                    pull_tree(
                        client,
                        &join(source, &child.name),
                        &target.join(child.name),
                        child.mode,
                    )
                    .await?;
                }
            }
            0o100000 => {
                let mut out = fs::File::create_new(target).await.map_err(err)?;
                client.pull(source, &mut out, IO_TIME).await.map_err(err)?;
            }
            _ => return Err("仅支持复制普通文件和目录；符号链接请打开后选择内容".into()),
        }
        Ok(())
    })
}
fn copy_local<'a>(
    source: &'a Path,
    target: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
    Box::pin(async move {
        let m = fs::symlink_metadata(source).await.map_err(err)?;
        if m.is_dir() {
            fs::create_dir(target).await.map_err(err)?;
            let mut read = fs::read_dir(source).await.map_err(err)?;
            while let Some(item) = read.next_entry().await.map_err(err)? {
                copy_local(&item.path(), &target.join(item.file_name())).await?;
            }
        } else if m.is_file() {
            let mut input = fs::File::open(source).await.map_err(err)?;
            let mut output = fs::File::create_new(target).await.map_err(err)?;
            tokio::io::copy(&mut input, &mut output)
                .await
                .map_err(err)?;
            output.flush().await.map_err(err)?;
            fs::set_permissions(target, m.permissions())
                .await
                .map_err(err)?;
        } else {
            return Err("仅支持复制普通文件和目录".into());
        }
        Ok(())
    })
}
pub async fn list(app: &App, location: wire::FileLocation) -> Result<wire::FileListing> {
    let mut result = Store::open(app, &location.device_id)
        .await?
        .list(&location.path)
        .await?;
    result.location.as_mut().unwrap().device_id = location.device_id;
    Ok(result)
}
async fn cable(app: &App, a: &str, b: &str) -> Result<Option<(Uuid, i64)>> {
    if a.is_empty() || b.is_empty() || a == b {
        return Ok(None);
    }
    sqlx::query_as("SELECT id,revision FROM device_links WHERE connected AND ((source_device_id=$1 AND target_device_id=$2) OR (source_device_id=$2 AND target_device_id=$1))")
        .bind(a).bind(b).fetch_optional(&app.db).await.map_err(err)?.map(Some).ok_or_else(||"请先连接数据线".into())
}
pub async fn manage(app: &App, r: wire::FileManageRequest) -> Result<wire::FileManageResult> {
    use wire::FileOperation as Op;
    let op = Op::try_from(r.operation).map_err(err)?;
    let mut results = vec![];
    if op == Op::Mkdir {
        let target = r.target.ok_or("缺少目标目录")?;
        let p = join(&path(&target.path)?, name(&r.name)?);
        Store::open(app, &target.device_id).await?.mkdir(&p).await?;
        return Ok(wire::FileManageResult {
            results: vec![wire::FileResult {
                source: None,
                target: Some(wire::FileLocation {
                    device_id: target.device_id,
                    path: p,
                }),
                error: String::new(),
            }],
        });
    }
    for source in r.sources {
        let mut target_result = None;
        let result: Result<()> = async {
            let p = path(&source.path)?;
            if p == "/" {
                return Err("请选择根目录内的文件".into());
            }
            let store = Store::open(app, &source.device_id).await?;
            if op == Op::Delete {
                return store.remove(&p).await;
            }
            let target = r.target.as_ref().ok_or("缺少目标目录")?;
            let leaf = if r.name.is_empty() {
                Path::new(&p)
                    .file_name()
                    .ok_or("无效文件名")?
                    .to_str()
                    .ok_or("无效文件名")?
            } else {
                name(&r.name)?
            };
            let to = join(&path(&target.path)?, leaf);
            target_result = Some(wire::FileLocation {
                device_id: target.device_id.clone(),
                path: to.clone(),
            });
            if source.device_id == target.device_id && (to == p || to.starts_with(&format!("{p}/")))
            {
                return Err("目标不能是文件本身或它的子目录".into());
            }
            let link = cable(app, &source.device_id, &target.device_id).await?;
            let destination = Store::open(app, &target.device_id).await?;
            if matches!(op, Op::Rename | Op::Move) && source.device_id == target.device_id {
                return store.rename(&p, &to).await;
            }
            if !matches!(op, Op::Copy | Op::Move) {
                return Err("无效文件操作".into());
            }
            if source.device_id == target.device_id {
                return store.copy_within(&p, &to).await;
            }
            if let Store::Workspace(_) = &store {
                return async {
                    destination.import(&store.local(&p).await?, &to).await?;
                    if op == Op::Move {
                        store.remove(&p).await?;
                    }
                    Ok(())
                }
                .await;
            }
            let staging = app
                .root
                .join("data/file-transfers")
                .join(Uuid::now_v7().to_string());
            fs::create_dir_all(&staging).await.map_err(err)?;
            let payload = staging.join(leaf);
            let transfer: Result<()> = async {
                store.export(&p, &payload).await?;
                if cable(app, &source.device_id, &target.device_id).await? != link {
                    return Err("数据线连接已变化，请重新选择文件".into());
                }
                destination.import(&payload, &to).await?;
                if cable(app, &source.device_id, &target.device_id).await? != link {
                    return Err("数据线连接已变化，请检查目标文件；未删除原文件".into());
                }
                if op == Op::Move {
                    store.remove(&p).await?;
                }
                Ok(())
            }
            .await;
            let cleanup = fs::remove_dir_all(staging).await.map_err(err);
            transfer?;
            cleanup
        }
        .await;
        results.push(wire::FileResult {
            source: Some(source),
            target: target_result,
            error: result.err().unwrap_or_default(),
        });
    }
    Ok(wire::FileManageResult { results })
}
pub async fn route(
    State(app): State<Shared>,
    axum::extract::Path(op): axum::extract::Path<String>,
    headers: HeaderMap,
    body: std::result::Result<Bytes, BytesRejection>,
) -> Response {
    let result = match op.as_str() {
        "list" => match decode::<wire::FileLocation>(&app, &headers, body) {
            Ok(r) => list(&app, r).await.map(|r| pb(StatusCode::OK, r)),
            Err(e) => return e.into(),
        },
        "manage" => match decode::<wire::FileManageRequest>(&app, &headers, body) {
            Ok(r) => manage(&app, r).await.map(|r| pb(StatusCode::OK, r)),
            Err(e) => return e.into(),
        },
        _ => return error(StatusCode::NOT_FOUND, "not_found", "操作不存在"),
    };
    result.unwrap_or_else(|e| error(StatusCode::CONFLICT, "file_operation", &e))
}

pub async fn upload(
    State(app): State<Shared>,
    headers: HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> Response {
    if let Err(e) = auth(&app, &headers) {
        return e.into();
    }
    let result: Result<wire::FileLocation> = async {
        let location = multipart
            .next_field()
            .await
            .map_err(err)?
            .ok_or("缺少上传目录")?;
        if location.name() != Some("location") {
            return Err("缺少上传目录".into());
        }
        let location =
            wire::FileLocation::decode(location.bytes().await.map_err(err)?).map_err(err)?;
        if !location.device_id.is_empty() {
            return Err("上传先保存到工作文件".into());
        }
        let store = Store::open(&app, "").await?;
        let folder = store.local(&location.path).await?;
        let mut field = multipart
            .next_field()
            .await
            .map_err(err)?
            .ok_or("请选择文件")?;
        let original = name(field.file_name().ok_or("缺少文件名")?)?.to_owned();
        let id = Uuid::now_v7().to_string();
        let (stem, ext) = original
            .rsplit_once('.')
            .map_or((original.as_str(), String::new()), |(a, b)| {
                (a, format!(".{b}"))
            });
        let leaf = format!("{stem} ({}){ext}", &id[24..]);
        let target = folder.join(&leaf);
        let temporary = folder.join(format!(".{id}.part"));
        let mut output = fs::File::create_new(&temporary).await.map_err(err)?;
        while let Some(chunk) = field.chunk().await.map_err(err)? {
            output.write_all(&chunk).await.map_err(err)?;
        }
        output.flush().await.map_err(err)?;
        drop(output);
        fs::rename(temporary, target).await.map_err(err)?;
        Ok(wire::FileLocation {
            device_id: String::new(),
            path: join(&path(&location.path)?, &leaf),
        })
    }
    .await;
    match result {
        Ok(v) => pb(StatusCode::OK, v),
        Err(e) => error(StatusCode::BAD_REQUEST, "file_upload", &e),
    }
}

pub async fn tool(
    app: &App,
    run: Uuid,
    name: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value> {
    fn location(v: &serde_json::Value) -> Result<wire::FileLocation> {
        Ok(wire::FileLocation {
            device_id: v["device_id"]
                .as_str()
                .ok_or("缺少device_id，工作文件使用空字符串")?
                .into(),
            path: v["path"].as_str().ok_or("缺少path")?.into(),
        })
    }
    async fn authorize(app: &App, run: Uuid, location: &wire::FileLocation) -> Result<()> {
        if !location.device_id.is_empty() {
            crate::desktop::file_authorized(app, run, &location.device_id).await?;
        }
        Ok(())
    }
    if name == "device_file_list" {
        let l = location(args)?;
        authorize(app, run, &l).await?;
        let result = list(app, l).await?;
        return Ok(
            serde_json::json!({"success":true,"path":result.location.map(|l|l.path),"files":result.entries.into_iter().map(|e|serde_json::json!({"name":e.name,"path":e.path,"directory":e.directory,"bytes":e.size_bytes,"modified_at_ms":e.modified_at_ms,"symlink":e.symlink})).collect::<Vec<_>>()}),
        );
    }
    let operation = match args["operation"].as_str() {
        Some("copy") => wire::FileOperation::Copy,
        Some("move") => wire::FileOperation::Move,
        Some("delete") => wire::FileOperation::Delete,
        Some("mkdir") => wire::FileOperation::Mkdir,
        Some("rename") => wire::FileOperation::Rename,
        _ => return Err("无效文件操作".into()),
    };
    let sources = args["sources"]
        .as_array()
        .ok_or("缺少sources")?
        .iter()
        .map(location)
        .collect::<Result<Vec<_>>>()?;
    let target = location(&args["target"])?;
    for l in sources.iter().chain(std::iter::once(&target)) {
        authorize(app, run, l).await?;
    }
    let result = manage(
        app,
        wire::FileManageRequest {
            operation: operation as i32,
            sources,
            target: Some(target),
            name: args["name"].as_str().unwrap_or("").into(),
        },
    )
    .await?;
    Ok(
        serde_json::json!({"success":result.results.iter().all(|r|r.error.is_empty()),"results":result.results.into_iter().map(|r|serde_json::json!({"source":r.source.map(|s|serde_json::json!({"device_id":s.device_id,"path":s.path})),"target":r.target.map(|s|serde_json::json!({"device_id":s.device_id,"path":s.path})),"error":r.error})).collect::<Vec<_>>()}),
    )
}
/// Keep chat attachment storage immutable while exposing an editable workspace copy.
pub async fn archive_attachment(app: &App, id: Uuid, name: &str, bytes: &[u8]) -> Result<()> {
    let folder = app.root.join("data/workspaces/admin/uploads");
    fs::create_dir_all(&folder).await.map_err(err)?;
    fs::write(folder.join(format!("{}-{name}", id.simple())), bytes)
        .await
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn folder() -> PathBuf {
        std::env::temp_dir().join(format!("aiphone-files-{}", Uuid::now_v7()))
    }
    #[tokio::test]
    async fn workspace_lists_and_copies_without_overwrite() {
        let root = folder();
        fs::create_dir_all(&root).await.unwrap();
        let store = Store::Workspace(fs::canonicalize(&root).await.unwrap());
        store.mkdir("/input").await.unwrap();
        fs::write(root.join("input/中文 notes.txt"), b"original")
            .await
            .unwrap();
        let listing = store.list("/input").await.unwrap();
        assert_eq!(listing.entries.len(), 1);
        assert_eq!(listing.entries[0].size_bytes, 8);
        assert!(listing.entries[0].modified_at_ms > 0);
        store.copy_within("/input", "/output").await.unwrap();
        assert_eq!(
            fs::read(root.join("output/中文 notes.txt")).await.unwrap(),
            b"original"
        );
        assert!(store.copy_within("/input", "/output").await.is_err());
        assert!(store.rename("/input", "/output").await.is_err());
        assert!(store.remove("/").await.is_err());
        store.rename("/output", "/renamed").await.unwrap();
        store.remove("/renamed").await.unwrap();
        assert!(!root.join("renamed").exists());
        fs::remove_dir_all(root).await.unwrap();
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn workspace_does_not_expose_host_paths_through_links() {
        let root = folder();
        fs::create_dir(&root).await.unwrap();
        std::os::unix::fs::symlink("/", root.join("host")).unwrap();
        let store = Store::Workspace(fs::canonicalize(&root).await.unwrap());
        assert!(store.list("/host").await.is_err());
        store.remove("/host").await.unwrap();
        assert!(Path::new("/").exists());
        fs::remove_dir(root).await.unwrap();
    }
    #[test]
    fn paths_and_names() {
        assert_eq!(path("/a/b/../c").unwrap(), "/a/c");
        assert!(name("..").is_err());
        assert!(name("a/b").is_err());
        assert!(path("a\0b").is_err());
    }
    #[cfg(feature = "file-manager-device-qa")]
    #[tokio::test]
    async fn real_phone_and_desktop_file_roundtrip() {
        let root = folder();
        fs::create_dir(&root).await.unwrap();
        let phone = Store::Phone(Client::new(15037, "emulator-5564".into()).unwrap());
        let desktop = Store::Desktop;
        let phone_root = phone.list("/").await.unwrap();
        let desktop_root = desktop.list("/").await.unwrap();
        assert!(phone_root.entries.iter().any(|e| e.name == "data"));
        assert!(desktop_root.entries.iter().any(|e| e.name == "etc"));
        let id = Uuid::now_v7();
        let remote = format!("/data/local/tmp/aiphone-files-{id}");
        let computer = format!("/tmp/aiphone-files-{id}");
        fs::create_dir(root.join("source")).await.unwrap();
        fs::create_dir(root.join("source/nested")).await.unwrap();
        let payload: Vec<u8> = (0..180_031).map(|n| (n % 251) as u8).collect();
        fs::write(root.join("source/nested/中文 data.bin"), &payload)
            .await
            .unwrap();
        let start = std::time::Instant::now();
        phone.import(&root.join("source"), &remote).await.unwrap();
        let entries = phone.list(&join(&remote, "nested")).await.unwrap();
        assert_eq!(entries.entries[0].size_bytes, payload.len() as u64);
        phone
            .export(&remote, &root.join("from-phone"))
            .await
            .unwrap();
        desktop
            .import(&root.join("from-phone"), &computer)
            .await
            .unwrap();
        desktop
            .export(&computer, &root.join("from-desktop"))
            .await
            .unwrap();
        assert_eq!(
            fs::read(root.join("from-desktop/nested/中文 data.bin"))
                .await
                .unwrap(),
            payload
        );
        phone
            .copy_within(
                &join(&remote, "nested/中文 data.bin"),
                &join(&remote, "copy.bin"),
            )
            .await
            .unwrap();
        assert!(
            phone
                .copy_within(&join(&remote, "copy.bin"), &join(&remote, "copy.bin"))
                .await
                .is_err()
        );
        phone
            .rename(&join(&remote, "copy.bin"), &join(&remote, "renamed.bin"))
            .await
            .unwrap();
        desktop
            .copy_within(&computer, &format!("{computer}-copy"))
            .await
            .unwrap();
        desktop.remove(&format!("{computer}-copy")).await.unwrap();
        phone.remove(&remote).await.unwrap();
        let back = format!("{remote}-back");
        phone
            .import(&root.join("from-desktop"), &back)
            .await
            .unwrap();
        phone
            .export(&back, &root.join("back-to-phone"))
            .await
            .unwrap();
        assert_eq!(
            fs::read(root.join("back-to-phone/nested/中文 data.bin"))
                .await
                .unwrap(),
            payload
        );
        phone.remove(&back).await.unwrap();
        desktop.remove(&computer).await.unwrap();
        fs::remove_dir_all(root).await.unwrap();
        eprintln!(
            "real roots: phone={} desktop={}; 180031-byte nested binary phone→harness→desktop verified; collision/rename/delete verified; {} ms",
            phone_root.entries.len(),
            desktop_root.entries.len(),
            start.elapsed().as_millis()
        );
    }
}
