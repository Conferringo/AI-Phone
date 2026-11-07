use crate::{
    Error, Result, runtime_environment,
    wsl::backend::{Backend, process::Process, token},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};
use tokio::sync::watch;

const ROLES: [&str; 7] = [
    "emulator",
    "runner",
    "media",
    "assistant",
    "channels",
    "web",
    "lan",
];
fn text(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or(Error::Invalid("source_service_path"))
}
fn binary(root: &Path, role: &str) -> PathBuf {
    if root.join("runtime/services").is_dir() {
        root.join("runtime/services/bin").join(role)
    } else {
        root.join("target/release").join(format!("ai-phone-{role}"))
    }
}
struct Specification {
    command: Vec<String>,
    environment: Vec<(&'static str, String)>,
}
fn specification(root: &Path, role: &str) -> Result<Specification> {
    let runner = || text(&binary(root, "runner"));
    let (command, environment) = match role {
        "emulator" => (
            vec![
                "/bin/sh".into(),
                text(&root.join("scripts/android-emulator.sh"))?,
            ],
            vec![],
        ),
        "runner" => (
            vec![runner()?],
            vec![("AI_PHONE_OCR", text(&binary(root, "vision"))?)],
        ),
        "media" => (vec![runner()?, "--media".into()], vec![]),
        "lan" => (vec![runner()?, "--lan".into()], vec![]),
        "assistant" => (vec![text(&binary(root, "assistant"))?], vec![]),
        "channels" => (
            vec![text(&binary(root, "assistant"))?, "--channels".into()],
            vec![],
        ),
        "web" => {
            let bundled = root.join("runtime/manifest.json").is_file();
            let command = if bundled {
                vec![
                    text(&root.join("runtime/bin/bun"))?,
                    text(&root.join("runtime/web/apps/web/server.js"))?,
                ]
            } else {
                vec![
                    text(&root.join(".tools/bin/bun"))?,
                    "run".into(),
                    "--cwd".into(),
                    text(&root.join("apps/web"))?,
                    "start".into(),
                ]
            };
            (
                command,
                vec![("HOSTNAME", "127.0.0.1".into()), ("PORT", "17860".into())],
            )
        }
        _ => return Err(Error::Invalid("source_service_role")),
    };
    Ok(Specification {
        command,
        environment,
    })
}
async fn launch(backend: &mut Backend<'_, Process>, role: &str) -> Result<Value> {
    let root = backend.root;
    match role {
        "emulator" => {
            let program = Path::new(&backend.env["ANDROID_HOME"]).join("platform-tools/adb");
            backend
                .checked(
                    backend.command(program, ["-P", "15037", "start-server"]),
                    vec![],
                )
                .await?;
        }
        "assistant" => {
            backend.containers().await?;
            backend
                .checked(
                    backend.command(
                        "docker",
                        [
                            "compose",
                            "-f",
                            "compose.yaml",
                            "-f",
                            "compose.dev.yaml",
                            "up",
                            "-d",
                            "--wait",
                            "postgres",
                            "redis",
                            "ocr",
                        ],
                    ),
                    vec![],
                )
                .await?;
            for flag in ["--prepare-database", "--migrate"] {
                backend
                    .checked(backend.command(binary(root, "assistant"), [flag]), vec![])
                    .await?;
            }
        }
        _ => {}
    }
    if ["assistant", "runner", "channels"].contains(&role) {
        token(
            root,
            if role == "runner" {
                "phone-runner"
            } else {
                role
            },
        )?;
    }
    let spec = specification(root, role)?;
    backend
        .unit(role, "start", spec.command, &spec.environment)
        .await
}
async fn execute(
    root: &Path,
    operation: &str,
    role: Option<&str>,
    stop: watch::Receiver<bool>,
) -> Result<Value> {
    let user =
        PathBuf::from(std::env::var_os("HOME").ok_or(Error::Invalid("source_service_home"))?);
    let program = std::env::current_exe()?;
    let mut env: BTreeMap<_, _> = std::env::vars().collect();
    env.extend(runtime_environment::resolve(root, &user)?);
    let mut backend = Backend {
        root,
        program: program.as_os_str(),
        env,
        execute: Process(stop),
    };
    match (operation, role) {
        ("launch", Some(role)) => launch(&mut backend, role).await,
        ("start", None) => {
            if root.join("data/restore-quarantine.json").try_exists()? {
                return Err(Error::Invalid("restored_state_quarantined"));
            }
            let mut states = Vec::new();
            for role in ROLES {
                states.push(backend.unit(role, "status", vec![], &[]).await?);
            }
            backend
                .native(
                    &[
                        "--managed-engine".into(),
                        root.as_os_str().into(),
                        user.into_os_string(),
                        "start".into(),
                    ],
                    vec![],
                )
                .await?;
            for (role, state) in ROLES.into_iter().zip(states) {
                if !state.is_null() && state.get("running").and_then(Value::as_bool) != Some(false)
                {
                    continue;
                }
                launch(&mut backend, role).await?;
                if role == "emulator" {
                    for script in ["android-ready.sh", "android-install-input.sh"] {
                        backend
                            .checked(
                                backend.command("/bin/sh", [root.join("scripts").join(script)]),
                                vec![],
                            )
                            .await?;
                    }
                }
            }
            Ok(json!({"started": true}))
        }
        ("stop", None) => {
            for role in ROLES.into_iter().rev() {
                backend.unit(role, "status", vec![], &[]).await?;
            }
            for role in ROLES.into_iter().rev() {
                backend.unit(role, "stop", vec![], &[]).await?;
            }
            Ok(json!({"stopped": true}))
        }
        _ => Err(Error::Invalid("source_service_arguments")),
    }
}
pub(crate) async fn run(arguments: &[OsString]) -> Result<()> {
    let [root, operation, rest @ ..] = arguments else {
        return Err(Error::Invalid("source_service_arguments"));
    };
    let operation = operation
        .to_str()
        .ok_or(Error::Invalid("source_service_arguments"))?;
    let role = match rest {
        [] => None,
        [role] => Some(
            role.to_str()
                .filter(|role| ROLES.contains(role))
                .ok_or(Error::Invalid("source_service_role"))?,
        ),
        _ => return Err(Error::Invalid("source_service_arguments")),
    };
    let root = Path::new(root).canonicalize()?;
    let (send, stop) = watch::channel(false);
    let signal = tokio::spawn(async move {
        crate::component::interrupt().await?;
        send.send_replace(true);
        Ok::<_, std::io::Error>(())
    });
    let result = execute(&root, operation, role, stop).await;
    signal.abort();
    let _ = signal.await;
    println!("{}", result?);
    Ok(())
}
