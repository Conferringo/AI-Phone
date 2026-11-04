//! Test-only SDK process: compiled by lifecycle tests, never a shipped service.
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    time::Duration,
};
fn root() -> PathBuf {
    std::env::current_dir().unwrap()
}
fn value(args: &[String], key: &str) -> String {
    args[args.iter().position(|v| v == key).unwrap() + 1].clone()
}
fn read(socket: &mut TcpStream) -> String {
    let mut length = [0; 4];
    socket.read_exact(&mut length).unwrap();
    let size = usize::from_str_radix(std::str::from_utf8(&length).unwrap(), 16).unwrap();
    let mut bytes = vec![0; size];
    socket.read_exact(&mut bytes).unwrap();
    String::from_utf8(bytes).unwrap()
}
fn bytes(socket: &mut TcpStream, data: &[u8]) {
    let _ = socket.write_all(data);
}
fn serve(mut socket: TcpStream) {
    let first = read(&mut socket);
    if first == "host:version" {
        bytes(&mut socket, b"OKAY00040045");
        return;
    }
    if !root().join("emulator.live").exists() {
        bytes(&mut socket, b"FAIL0007offline");
        return;
    }
    assert!(first.starts_with("host:transport:emulator-"));
    bytes(&mut socket, b"OKAY");
    let service = read(&mut socket);
    let mode = fs::read_to_string(root().join("mode")).unwrap_or_default();
    if service == "root:" {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root().join("root.requests"))
            .unwrap();
        file.write_all(b"root\n").unwrap();
        if mode == "unknown-root" {
            return;
        }
        bytes(
            &mut socket,
            b"OKAYadbd cannot run as root in production builds\n",
        );
        return;
    }
    bytes(&mut socket, b"OKAY");
    match service.as_str() {
        "shell:getprop sys.boot_completed" => {
            bytes(&mut socket, if mode == "never-boot" { b"0" } else { b"1" })
        }
        "shell:id -u" => bytes(
            &mut socket,
            if (mode == "unknown-root" && !root().join("root.requests").exists())
                || mode == "denied-root"
            {
                b"2000"
            } else {
                b"0"
            },
        ),
        "exec:screencap -p" => bytes(
            &mut socket,
            include_bytes!("../../../phone-runner/fixtures/ocr-probe.png"),
        ),
        _ => panic!("unexpected fixture service"),
    }
}
fn adb(args: &[String]) {
    if args.iter().any(|v| v == "nodaemon") {
        let address = value(args, "-L");
        let port = address
            .strip_prefix("tcp:")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let server = TcpListener::bind(("127.0.0.1", port)).unwrap();
        fs::write(root().join("adb.pid"), std::process::id().to_string()).unwrap();
        server.set_nonblocking(true).unwrap();
        while root().exists() && !root().join("adb.exit").exists() {
            match server.accept() {
                Ok((socket, _)) => {
                    socket.set_nonblocking(false).unwrap();
                    std::thread::spawn(move || serve(socket));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => panic!("fixture accept: {error}"),
            }
        }
    } else {
        assert!(args.ends_with(&["emu".into(), "kill".into()]));
        let port = value(args, "-s")
            .strip_prefix("emulator-")
            .unwrap()
            .parse::<u16>()
            .unwrap();
        if TcpStream::connect(("127.0.0.1", port)).is_err() {
            std::process::exit(1);
        }
        fs::write(root().join("exit"), b"stop").unwrap();
    }
}
fn emulator(args: &[String]) {
    fs::write(root().join("emulator.args"), args.join("\n")).unwrap();
    for key in [
        "ANDROID_AVD_HOME",
        "ANDROID_USER_HOME",
        "ANDROID_HOME",
        "HOME",
        "ADB_SERVER_SOCKET",
    ] {
        fs::write(
            root().join(format!("env-{key}")),
            std::env::var(key).unwrap(),
        )
        .unwrap();
    }
    let console = value(args, "-port").parse::<u16>().unwrap();
    let grpc = value(args, "-grpc").parse::<u16>().unwrap();
    fs::write(root().join("emulator.live"), std::process::id().to_string()).unwrap();
    if fs::read_to_string(root().join("mode")).unwrap_or_default() == "slow-console" {
        std::thread::sleep(Duration::from_millis(800));
    }
    let _sockets =
        [console, console + 1, grpc].map(|p| TcpListener::bind(("127.0.0.1", p)).unwrap());
    while root().exists() && !root().join("exit").exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let name = std::env::current_exe().unwrap();
    if name.file_stem().unwrap() == "adb" {
        adb(&args)
    } else {
        emulator(&args)
    }
}
