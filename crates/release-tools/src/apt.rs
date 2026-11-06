//! Maintainer-only APT publication. Private signing keys never enter the output.
use crate::Result;
use flate2::{Compression, GzBuilder};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
};

const INDEX: [&str; 3] = ["Packages", "Packages.gz", "Release"];
const SIGNED: [&str; 4] = [
    "InRelease",
    "Release.gpg",
    "ai-phone-archive-key.asc",
    "ai-phone.sources",
];

fn capture(command: &mut Command) -> Result<Vec<u8>> {
    let output = command.output()?;
    if !output.status.success() {
        std::io::stderr().write_all(&output.stderr)?;
        return Err(format!("{} failed", command.get_program().to_string_lossy()).into());
    }
    Ok(output.stdout)
}

fn records(folder: &Path, names: &[&str]) -> Result<Vec<Value>> {
    names.iter().map(|name| {
        let bytes = fs::read(folder.join(name))?;
        Ok(json!({"file": name, "size": bytes.len(), "sha256": format!("{:x}", Sha256::digest(&bytes))}))
    }).collect()
}

fn archive() -> Command {
    let mut command = Command::new("apt-ftparchive");
    command.args(["--md5=no", "--sha1=no", "--sha512=no"]);
    command
}

fn index(packages: &[PathBuf], output: &Path) -> Result<Value> {
    let packages = packages
        .iter()
        .map(fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    fs::create_dir_all(output)?;
    let stage = tempfile::tempdir()?;
    let mut names = Vec::new();
    for package in &packages {
        let name = package.file_name().ok_or("Missing package filename")?;
        symlink(package, stage.path().join(name))?;
        names.push(name.to_str().ok_or("Non-UTF8 package filename")?);
    }
    let packages_index = capture(archive().args(["packages", "."]).current_dir(stage.path()))?;
    for name in SIGNED {
        match fs::remove_file(output.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::write(output.join("Packages"), &packages_index)?;
    let mut gzip = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::default());
    gzip.write_all(&packages_index)?;
    fs::write(output.join("Packages.gz"), gzip.finish()?)?;
    let release = capture(
        archive()
            .args([
                "-o",
                "APT::FTPArchive::Release::Origin=AI Phone",
                "-o",
                "APT::FTPArchive::Release::Label=AI Phone",
                "release",
                ".",
            ])
            .current_dir(output),
    )?;
    fs::write(output.join("Release"), release)?;
    Ok(json!({"packages": names, "metadata": records(output, &INDEX)?}))
}

struct Signing {
    base_url: String,
    key: String,
    home: PathBuf,
}
impl Signing {
    fn command(&self) -> Command {
        let mut command = Command::new("gpg");
        command.arg("--homedir").arg(&self.home).args([
            "--batch",
            "--yes",
            "--local-user",
            &self.key,
        ]);
        command
    }

    fn sign(&self, output: &Path) -> Result<Value> {
        capture(
            self.command()
                .arg("--output")
                .arg(output.join("InRelease"))
                .arg("--clearsign")
                .arg(output.join("Release")),
        )?;
        capture(
            self.command()
                .args(["--armor", "--output"])
                .arg(output.join("Release.gpg"))
                .arg("--detach-sign")
                .arg(output.join("Release")),
        )?;
        let key = String::from_utf8(capture(self.command().args([
            "--export-options",
            "export-minimal",
            "--armor",
            "--export",
            &self.key,
        ]))?)?;
        fs::write(output.join("ai-phone-archive-key.asc"), &key)?;
        let packages = fs::read_to_string(output.join("Packages"))?;
        let architectures: BTreeSet<_> = packages
            .lines()
            .filter_map(|line| line.strip_prefix("Architecture: "))
            .collect();
        let embedded = key
            .lines()
            .map(|line| format!(" {}", if line.is_empty() { "." } else { line }))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            output.join("ai-phone.sources"),
            format!(
                "Types: deb\nURIs: {}/\nSuites: ./\nArchitectures: {}\nSigned-By:\n{embedded}\n",
                self.base_url.trim_end_matches('/'),
                architectures.into_iter().collect::<Vec<_>>().join(" ")
            ),
        )?;
        Ok(json!({"metadata": records(output, &SIGNED)?}))
    }
}

pub(super) fn run(mut arguments: impl Iterator<Item = OsString>) -> Result<Value> {
    if arguments.next().as_deref() != Some(OsStr::new("apt")) {
        return Err("Expected: release-tools apt index|sign|build".into());
    }
    let operation = arguments.next().ok_or("Missing APT operation")?;
    let mut output = None;
    let mut base_url = None;
    let mut key = None;
    let mut home = None;
    let mut packages = Vec::new();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--output") => {
                output = Some(PathBuf::from(
                    arguments.next().ok_or("Missing --output value")?,
                ))
            }
            Some("--base-url") => {
                base_url = Some(
                    arguments
                        .next()
                        .ok_or("Missing --base-url value")?
                        .into_string()
                        .map_err(|_| "Non-UTF8 channel URL")?,
                )
            }
            Some("--key") => {
                key = Some(
                    arguments
                        .next()
                        .ok_or("Missing --key value")?
                        .into_string()
                        .map_err(|_| "Non-UTF8 signing fingerprint")?,
                )
            }
            Some("--gnupg-home") => {
                home = Some(PathBuf::from(
                    arguments.next().ok_or("Missing --gnupg-home value")?,
                ))
            }
            Some(text) if text.starts_with('-') => return Err("Unknown APT option".into()),
            _ => packages.push(PathBuf::from(argument)),
        }
    }
    let output = output.ok_or("Missing --output")?;
    if operation == "index" {
        if packages.is_empty() || base_url.is_some() || key.is_some() || home.is_some() {
            return Err("Index requires packages and --output".into());
        }
        return index(&packages, &output);
    }
    let signing = Signing {
        base_url: base_url.ok_or("Missing --base-url")?,
        key: key.ok_or("Missing --key")?,
        home: home.ok_or("Missing --gnupg-home")?,
    };
    match operation.to_str() {
        Some("sign") if packages.is_empty() => signing.sign(&output),
        Some("build") if !packages.is_empty() => {
            let mut result = index(&packages, &output)?;
            result["metadata"].as_array_mut().unwrap().extend(
                signing.sign(&output)?["metadata"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned(),
            );
            Ok(result)
        }
        _ => Err("Expected APT index|sign|build with matching arguments".into()),
    }
}
