//! The bundled, installation-owned Lima engine; no host Docker context is adopted.
use super::{Backend, Execute, text};
use crate::{Error, Result, filesystem, managed_engine};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn definition(home: &Path, folder: &Path, policy: &Value, memory_mb: u64) -> Result<Value> {
    let arch = policy["arch"]
        .as_str()
        .ok_or(Error::Invalid("container_host_policy"))?;
    let image = policy["image"]["sha256"]
        .as_str()
        .ok_or(Error::Invalid("container_host_policy"))?;
    let packages = quote(&text(&folder.join("packages"))?);
    Ok(json!({
        "minimumLimaVersion":"2.2.1", "vmType":"vz", "arch":arch,
        "cpus":4, "memory":format!("{}GiB", (memory_mb+2048).div_ceil(1024).max(4)), "disk":"64GiB",
        "images":[{"location":folder.join("image.iso"),"arch":arch,"digest":format!("sha256:{image}")}],
        "mountType":"virtiofs", "mounts":[{"location":home,"writable":true}],
        "containerd":{"system":false,"user":false}, "propagateProxyEnv":false,
        "ssh":{"loadDotSSHPubKeys":false,"forwardAgent":false},
        "env":{"HTTP_PROXY":"","HTTPS_PROXY":"","ALL_PROXY":"","http_proxy":"","https_proxy":"","all_proxy":""},
        "provision":[{"mode":"dependency","skipDefaultDependencyResolution":true,"script":"#!/bin/sh\nexit 0\n"},
            {"mode":"system","script":format!("#!/bin/sh\nset -eu\npackages={packages}\napk --no-network add \"$packages\"/*.apk\nrc-service docker start\n")}],
        "probes":[{"script":"#!/bin/sh\nsudo docker info >/dev/null 2>&1\n","hint":"The local container engine must be ready"}],
        "hostResolver":{"hosts":{"host.docker.internal":"host.lima.internal"}},
        "portForwards":[{"guestPortRange":[17860,17869],"ignore":true}]
    }))
}

fn wrapper(root: &Path, command: &Path, state: &Path) -> Result<String> {
    let defaults = [
        ("AI_PHONE_CLASH_DESKTOP_DIR", "linux"),
        ("AI_PHONE_DESKTOP_CPUS", "2"),
        ("AI_PHONE_DESKTOP_MEMORY", "2560m"),
        ("AI_PHONE_CONTAINER_PROXY", ""),
        ("AI_PHONE_POSTGRES_IMAGE", ""),
        ("AI_PHONE_REDIS_IMAGE", ""),
        ("AI_PHONE_DESKTOP_IMAGE", ""),
        ("AI_PHONE_WORKSPACE_IMAGE", ""),
        ("AI_PHONE_IMAGE_PULL_POLICY", "missing"),
    ];
    let args = defaults
        .map(|(key, value)| format!("{key}=\"${{{key}:-{value}}}\""))
        .join(" ");
    Ok(format!(
        "#!/bin/sh\nexport LIMA_HOME={}\nexec {} shell --workdir {} ai-phone sudo env {args} docker \"$@\"\n",
        quote(&text(state)?),
        quote(&text(command)?),
        quote(&text(root)?)
    ))
}

pub(super) async fn start<E: Execute>(
    backend: &mut Backend<'_, E>,
    home: &Path,
    user: &Path,
) -> Result<()> {
    let root = backend.root;
    let folder = root.join("runtime/container-host");
    let policies: Value =
        serde_json::from_slice(&fs::read(root.join("config/container-host-runtime.json"))?)
            .map_err(|_| Error::Invalid("container_host_policy"))?;
    let policy = &policies["hosts"]["macos-arm64"];
    let command = folder.join("lima/bin/limactl");
    let state = managed_engine::state_directory(root, home, user)?;
    managed_engine::claim_state(&root.join("data").canonicalize()?, &state)?;
    backend.env.insert("LIMA_HOME".into(), text(&state)?);
    let body = backend
        .checked(
            backend.command(
                &command,
                [
                    "list",
                    "--format",
                    "{{if eq .Name \"ai-phone\"}}{{.Status}}{{end}}",
                ],
            ),
            vec![],
        )
        .await?;
    let status = std::str::from_utf8(&body)
        .map_err(|_| Error::Invalid("container_host_status"))?
        .trim();
    match status {
        "Running" => {}
        "Stopped" => {
            backend
                .checked(
                    backend.command(&command, ["start", "--tty=false", "ai-phone"]),
                    vec![],
                )
                .await?;
        }
        "" => {
            let profile: Value =
                serde_json::from_slice(&fs::read(root.join("data/deployment/profile.json"))?)
                    .map_err(|_| Error::Invalid("host_profile"))?;
            let memory = profile["desktop"]["memory_mb"]
                .as_u64()
                .ok_or(Error::Invalid("host_profile"))?;
            let file = home.join("container-host.yaml");
            filesystem::publish_private(
                home,
                "container-host.yaml",
                &serde_json::to_vec(&definition(home, &folder, policy, memory)?)
                    .map_err(|_| Error::Invalid("container_host_policy"))?,
                false,
            )?;
            backend
                .checked(
                    backend.command(
                        &command,
                        [
                            std::ffi::OsStr::new("start"),
                            std::ffi::OsStr::new("--tty=false"),
                            std::ffi::OsStr::new("--name=ai-phone"),
                            file.as_os_str(),
                        ],
                    ),
                    vec![],
                )
                .await?;
        }
        _ => return Err(Error::Invalid("container_host_status")),
    }
    let bin = home.join("bin");
    filesystem::directory(&bin, true)?;
    let path = bin.join("docker");
    let body = wrapper(root, &command, &state)?;
    if path.try_exists()? && fs::read_to_string(&path)? != body {
        // Validate the existing installation selection before replacing its wrapper.
        if managed_engine::environment(root, user)?.is_empty() {
            return Err(Error::Invalid("managed_engine_owner"));
        }
    }
    filesystem::publish_private(&bin, "docker", body.as_bytes(), false)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    managed_engine::save(root, user, home, &state, &command, &bin)?;
    backend
        .env
        .extend(crate::runtime_environment::resolve(root, user)?);
    backend
        .checked(backend.command(&path, ["info"]), vec![])
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_definition_keeps_proxy_and_mount_contract() {
        let value = definition(
            Path::new("/home/App Data"),
            Path::new("/home/App Data/releases/id/runtime/container-host"),
            &json!({"arch":"aarch64","image":{"sha256":"abc"}}),
            2560,
        )
        .unwrap();
        assert_eq!(value["memory"], "5GiB");
        assert_eq!(value["propagateProxyEnv"], false);
        assert_eq!(value["mounts"][0]["location"], "/home/App Data");
        assert!(
            value["provision"][1]["script"]
                .as_str()
                .unwrap()
                .contains("apk --no-network")
        );
        assert_eq!(value["portForwards"][0]["ignore"], true);
    }
    #[test]
    fn wrapper_preserves_arguments_and_quotes_installation_paths() {
        let body = wrapper(
            Path::new("/App's Data/releases/id"),
            Path::new("/App's Data/limactl"),
            Path::new("/private/vms/app"),
        )
        .unwrap();
        assert!(body.contains("'\\''"));
        assert!(body.ends_with("docker \"$@\"\n"));
        assert!(body.contains("AI_PHONE_DESKTOP_MEMORY=\"${AI_PHONE_DESKTOP_MEMORY:-2560m}\""));
        assert!(!body.contains("python"));
    }
}
