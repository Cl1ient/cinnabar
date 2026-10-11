//! Runs the capture command with bounded fixture processes instead of a client or Tracy.
#[cfg(unix)]
#[test]
fn exited_client_reports_startup_failure_instead_of_waiting_for_the_collector() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(temp.path().join("target/debug")).unwrap();
    for (path, source) in [
        (
            bin.join("make"),
            r#"#!/usr/bin/env python3
import json, pathlib
print(json.dumps({'reason':'compiler-artifact','target':{'name':'bedrock-client','kind':['bin']},'executable':str(pathlib.Path('target/debug/bedrock-client').resolve())}))
"#,
        ),
        (
            temp.path().join("target/debug/bedrock-client"),
            "#!/bin/sh\nexit 37\n",
        ),
        (bin.join("tracy-csvexport"), "#!/bin/sh\nexit 0\n"),
        (
            bin.join("tracy-capture"),
            "#!/usr/bin/env python3\nimport signal,sys\nif '--help' not in sys.argv: signal.pause()\n",
        ),
    ] {
        fs::write(&path, source).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_devtool"))
        .current_dir(temp.path())
        .env("PROFILE", "dev")
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .args(["capture", "--seconds", "1", "--out", "output"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    let error = String::from_utf8_lossy(&result.stderr);
    assert!(
        error.contains("client exited") && error.contains("37"),
        "lost startup failure: {error}"
    );
}

#[cfg(unix)]
#[test]
fn capture_connects_to_its_new_client_when_the_default_port_is_occupied() {
    use std::{fs, net::TcpListener, os::unix::fs::PermissionsExt, process::Command};
    const DEFAULT_TRACY_PORT: u16 = 8086;
    let _unrelated = match TcpListener::bind(("127.0.0.1", DEFAULT_TRACY_PORT)) {
        Ok(listener) => Some(listener),
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => None,
        Err(error) => panic!("cannot occupy the default Tracy port: {error}"),
    };
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(temp.path().join("target/debug")).unwrap();
    assert!(
        Command::new("python3")
            .args(["-c", "import os; os.mkfifo('client-ready')"])
            .current_dir(temp.path())
            .status()
            .unwrap()
            .success()
    );
    for (path, source) in [
        (
            bin.join("make"),
            r#"#!/usr/bin/env python3
import json, pathlib
print(json.dumps({'reason':'compiler-artifact','target':{'name':'bedrock-client','kind':['bin']},'executable':str(pathlib.Path('target/debug/bedrock-client').resolve())}))
"#,
        ),
        (
            temp.path().join("target/debug/bedrock-client"),
            r#"#!/usr/bin/env python3
import os, signal, socket
port = int(os.environ['TRACY_PORT'])
assert port != int(os.environ['UNRELATED_TRACY_PORT'])
with socket.socket() as server:
    server.settimeout(10)
    server.bind(('127.0.0.1', port))
    server.listen(1)
    with open('client-ready', 'w') as ready:
        ready.write('ready')
    connection, _ = server.accept()
    with connection:
        connection.sendall(b'new-capture-client')
        signal.pause()
"#,
        ),
        (
            bin.join("tracy-capture"),
            r#"#!/usr/bin/env python3
import os, pathlib, socket, sys
if '--help' in sys.argv:
    sys.exit(0)
port = int(sys.argv[sys.argv.index('-p') + 1])
assert port != int(os.environ['UNRELATED_TRACY_PORT'])
with open('client-ready') as ready:
    assert ready.read() == 'ready'
connection = socket.create_connection(('127.0.0.1', port), timeout=5)
with connection:
    identity = connection.recv(128)
assert identity == b'new-capture-client'
pathlib.Path(sys.argv[sys.argv.index('-o') + 1]).write_bytes(identity)
"#,
        ),
        (
            bin.join("tracy-csvexport"),
            r#"#!/usr/bin/env python3
import sys
if '--help' in sys.argv:
    sys.exit(0)
if '-u' in sys.argv:
    print('name,thread,ns_since_start\npresent_frames,1,0\npresent_frames,1,16666667')
else:
    print('name,total_ns,max_ns\nfixture,100,100')
"#,
        ),
    ] {
        fs::write(&path, source).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_devtool"))
        .current_dir(temp.path())
        .env("PROFILE", "dev")
        .env("TRACY_PORT", DEFAULT_TRACY_PORT.to_string())
        .env("UNRELATED_TRACY_PORT", DEFAULT_TRACY_PORT.to_string())
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .args(["capture", "--seconds", "1", "--out", "output"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        fs::read(temp.path().join("output/capture.tracy")).unwrap(),
        b"new-capture-client"
    );
    assert!(temp.path().join("output/summary.json").is_file());
}

#[cfg(unix)]
#[test]
fn capture_launches_the_emitted_executable_under_cargo_output_overrides() {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};
    for (profile, target_dir, triple) in [
        ("dev", true, false),
        ("play", false, true),
        ("release", true, true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let bin = temp.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let directory = if profile == "dev" { "debug" } else { profile };
        let stale = temp
            .path()
            .join("target")
            .join(directory)
            .join("bedrock-client");
        fs::create_dir_all(stale.parent().unwrap()).unwrap();
        for (path, source) in [
            (stale, "#!/bin/sh\nexit 99\n"),
            (
                bin.join("make"),
                r#"#!/usr/bin/env python3
import json, os, pathlib
root = pathlib.Path(os.environ.get('CARGO_TARGET_DIR', 'target'))
if 'CARGO_BUILD_TARGET' in os.environ:
    root /= os.environ['CARGO_BUILD_TARGET']
profile = os.environ['PROFILE']
executable = (root / ('debug' if profile == 'dev' else profile) / 'bedrock-client').resolve()
executable.parent.mkdir(parents=True, exist_ok=True)
executable.write_text('#!/bin/sh\nexit 37\n')
executable.chmod(0o755)
print(json.dumps({'reason':'compiler-artifact','target':{'name':'mod-host','kind':['bin']},'executable':'wrong-helper'}))
print(json.dumps({'reason':'compiler-artifact','target':{'name':'bedrock-client','kind':['bin']},'executable':str(executable)}))
"#,
            ),
            (bin.join("tracy-csvexport"), "#!/bin/sh\nexit 0\n"),
            (
                bin.join("tracy-capture"),
                "#!/usr/bin/env python3\nimport signal,sys\nif '--help' not in sys.argv: signal.pause()\n",
            ),
        ] {
            fs::write(&path, source).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_devtool"));
        command
            .current_dir(temp.path())
            .env("PROFILE", profile)
            .env_remove("CARGO_TARGET_DIR")
            .env_remove("CARGO_BUILD_TARGET")
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .args(["capture", "--seconds", "1", "--out", "output"]);
        if target_dir {
            command.env("CARGO_TARGET_DIR", temp.path().join("custom-output"));
        }
        if triple {
            command.env("CARGO_BUILD_TARGET", "fixture-target");
        }
        let result = command.output().unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(
            error.contains("client exited") && error.contains("37"),
            "profile={profile}: {error}"
        );
    }
}
