//! Tauri uses the shared native tool component owner.
pub(super) use ai_phone_host_runtime::installer_tools::prepare;

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
    };
    fn home() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "aiphone-tools-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    #[ignore = "requires reviewed installer tools and source payload"]
    fn real_portable_tools_stage_source_without_host_python_node_or_network() {
        let source = PathBuf::from(
            std::env::var_os("AI_PHONE_TEST_INSTALLER_TOOLS").expect("reviewed tools path"),
        );
        let payload = PathBuf::from(
            std::env::var_os("AI_PHONE_TEST_SOURCE_PAYLOAD").expect("reviewed source payload"),
        );
        let home = home();
        let result = (|| -> Result<(), String> {
            let identity = super::super::download::hash(&source.join("manifest.json"))?;
            let host = super::super::platform_key(std::env::consts::OS, std::env::consts::ARCH);
            let runtime = prepare(
                &source,
                &home,
                &identity,
                &host,
                &super::super::available_bytes,
                &|| Ok(()),
            )?;
            let code = r#"import sys,ssl,sqlite3,json,pathlib,subprocess
sys.path.insert(0,sys.argv[1])
from client_host_entry import stage_source
home=pathlib.Path(sys.argv[3]);(home/'user/data').mkdir(parents=True)
(home/'user/data/chat-fixture').write_text('keep')
root=stage_source(pathlib.Path(sys.argv[2]),home)
sys.path.insert(0,str(root/'scripts'))
from installer_tools import select,verify
from runtime_config import runtime_env
component=pathlib.Path(sys.argv[4]).resolve();manifest=verify(component)
select(root,component);environment=runtime_env(root)
bins=[str((component/manifest['tools'][role]['executable']).parent) for role in ('python','node') if role in manifest['tools']]
assert environment['PATH'].split(':')[:len(bins)]==bins
assert (home/'user/data/chat-fixture').read_text()=='keep'
assert pathlib.Path(sys.prefix).resolve()==(component/'python').resolve()
node=None
if 'node' in manifest['tools']:
    node=subprocess.check_output(['node','--version'],env={'PATH':str(component/'node/bin')},text=True).strip()
    assert node=='v'+manifest['tools']['node']['version']
print(json.dumps({'python':sys.version.split()[0],'node':node,'ssl':ssl.OPENSSL_VERSION,'sqlite':sqlite3.sqlite_version,'staged_source':True,'chat_fixture_preserved':True}))
"#;
            let output = std::process::Command::new(&runtime.python)
                .env_clear()
                .env("PATH", "/no-host-tools")
                .args(["-I", "-S", "-B", "-c", code])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../scripts"))
                .arg(payload)
                .arg(home.join("deployment"))
                .arg(runtime.directory)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into());
            }
            println!("{}", String::from_utf8_lossy(&output.stdout));
            Ok(())
        })();
        fs::remove_dir_all(home).unwrap();
        assert!(result.is_ok(), "{result:?}");
    }
}
