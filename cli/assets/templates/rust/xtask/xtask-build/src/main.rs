use cargo_metadata::{Artifact, Message};
use log::{debug, error, info, warn};
use std::{
    env, io,
    panic::resume_unwind as silent_panic,
    process::{self, Command},
};

fn main() {
    env_logger::init_from_env("CARGO_XTASK_LOG");

    let args = env::args().skip(1).collect();
    build(args)
}

fn build(args: Vec<String>) {
    wasm_opt(cargo_build(args))
}

fn cargo_build(args: Vec<String>) -> Vec<u8> {
    let release = args
        .iter()
        .all(|arg| {
            !(arg.starts_with("--profile") || arg.starts_with("-r") || arg.starts_with("--release"))
        })
        .then(|| "--release");

    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut command = Command::new(cargo);
    command
        .args([
            "build",
            "--target=wasm32-unknown-unknown",
            "--message-format=json-render-diagnostics",
        ])
        .args(release)
        .args(args)
        .stderr(process::Stdio::inherit());

    debug!(target: "cargo-build", "running build command: {:?}", command);

    let output = command.output().unwrap();
    if !output.status.success() {
        error!(target: "cargo_build", "build command failed with status: {:?}", output.status);
        silent_panic(Box::new(output.status));
    }

    output.stdout
}

fn wasm_opt(cargo_output: Vec<u8>) {
    let mut skip = false;
    for msg in Message::parse_stream(cargo_output.as_slice()) {
        let wasm_module = match msg.expect("parsing `cargo build` output") {
            Message::CompilerArtifact(Artifact {
                executable: Some(m),
                ..
            }) if matches!(m.extension(), Some("wasm")) => m,
            _ => continue,
        };

        if !skip {
            skip = !wasm_opt_once(wasm_module.as_str());
        }
        if skip {
            debug!("skipping wasm-opt size optimizations for: {}", wasm_module);
        }
    }
}

/// returns false if `wasm-opt` wasn't found
fn wasm_opt_once(wasm_module: &str) -> bool {
    let mut command = Command::new("wasm-opt");
    command.args([
        "-Oz",
        "--strip-debug",
        "--strip-producers",
        "--zero-filled-memory",
        wasm_module,
        "--output",
        wasm_module,
    ]);
    debug!(target: "wasm-opt", "running wasm-opt command: {:?}", command);

    command.status().map(|status| {
            if !status.success() {
                error!(target: "wasm-opt", "wasm-opt command failed with status: {:?}", status);
                silent_panic(Box::new(status))
            } else {
                true
            }
        }).or_else(|e| match e.kind() {
            io::ErrorKind::NotFound => {
                warn!(target: "wasm-opt", "wasm-opt wasn't found, skipping size optimizations, cartridge size may be too large");
                info!(target: "wasm-opt",
                    "wasm-opt is a part of the binaryen toolchain, \
                    which you can install from https://github.com/WebAssembly/binaryen/releases/"
                );
                Ok(false)
            }
            _ => Err(e),
        }).unwrap()
}
