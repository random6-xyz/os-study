// runner: build the kernel, create a bootable disk image, and run it in QEMU.
//
// Usage:
//   cargo run                          -> observe mode (stream serial output until exit/timeout)
//   cargo run -- "marker1" "marker2"   -> pass when every marker appears in serial output
//
// Exit code mapping (isa-debug-exit): success 0x10 -> 33, failure 0x11 -> 35.

use std::{
    io::{BufRead, BufReader, Write},
    ops::{Deref, DerefMut},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

const KERNEL_PACKAGE: &str = "rand-kernel";
const KERNEL_TARGET: &str = "x86_64-unknown-none";
const QEMU_BIN: &str = "qemu-system-x86_64";

const QEMU_EXIT_SUCCESS: i32 = 0x10 * 2 + 1; // 33
const QEMU_EXIT_FAILURE: i32 = 0x11 * 2 + 1; // 35

fn default_timeout() -> Duration {
    let ms = std::env::var("RUNNER_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30_000);
    Duration::from_millis(ms)
}

// The workspace profile sets panic = "abort", so a panic would skip all
// Drop guards and could leave QEMU running. Never panic on I/O errors.
fn print_line(line: &str) {
    let _ = writeln!(std::io::stdout(), "{line}");
}

fn eprint_line(line: &str) {
    let _ = writeln!(std::io::stderr(), "{line}");
}

enum Verdict {
    Pass(String),
    Fail(String),
    Observe(String),
}

/// Kills the QEMU child on drop so no zombie process survives a panic or an
/// early exit (e.g. when the runner output is piped and the pipe breaks).
struct QemuGuard(Child);

impl Deref for QemuGuard {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.0
    }
}

impl DerefMut for QemuGuard {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for QemuGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn main() {
    let markers: Vec<String> = std::env::args().skip(1).collect();

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root not found");

    // 1. Build the kernel for the bare-metal target.
    let build_status = Command::new("cargo")
        .arg("build")
        .arg("-p")
        .arg(KERNEL_PACKAGE)
        .arg("--target")
        .arg(KERNEL_TARGET)
        .current_dir(workspace_root)
        .status()
        .expect("failed to spawn cargo build");
    if !build_status.success() {
        eprint_line("[runner] kernel build failed");
        std::process::exit(1);
    }

    // 2. Create a bootable disk image with the bootloader crate.
    let kernel_elf = workspace_root
        .join("target")
        .join(KERNEL_TARGET)
        .join("debug")
        .join(KERNEL_PACKAGE);
    let image_path = workspace_root.join("target").join("kernel.img");

    let boot = bootloader::BiosBoot::new(&kernel_elf);
    if let Err(e) = boot.create_disk_image(&image_path) {
        eprint_line(&format!("[runner] failed to create disk image: {e}"));
        std::process::exit(1);
    }

    // 3. Launch QEMU.
    let mut qemu = QemuGuard(
        Command::new(QEMU_BIN)
            .arg("-drive")
            .arg(format!("format=raw,file={}", image_path.display()))
            .args(["-serial", "stdio"])
            .args(["-device", "isa-debug-exit"])
            .args(["-no-reboot", "-display", "none"])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("failed to start qemu (is qemu-system-x86_64 installed?)"),
    );
    eprint_line(&format!("[runner] qemu pid: {}", qemu.id()));

    // 4. Stream serial output in a reader thread and buffer it.
    let buffer = Arc::new(Mutex::new(String::new()));
    {
        let buffer = buffer.clone();
        let stdout = qemu.stdout.take().expect("qemu stdout piped");
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let line = line.unwrap_or_default();
                // Ignore write errors: the pipe may be closed by the caller.
                print_line(&line);
                buffer.lock().unwrap().push_str(&line);
                buffer.lock().unwrap().push('\n');
            }
        });
    }

    // 5. Wait for QEMU exit, all markers, or timeout.
    let timeout = default_timeout();
    let deadline = Instant::now() + timeout;
    let verdict = loop {
        if let Some(status) = qemu.try_wait().expect("failed to wait for qemu") {
            break match status.code() {
                Some(QEMU_EXIT_SUCCESS) => Verdict::Pass("qemu exit code 33 (success)".into()),
                Some(QEMU_EXIT_FAILURE) => {
                    Verdict::Fail("qemu exit code 35 (kernel failure)".into())
                }
                Some(code) => Verdict::Fail(format!("unexpected qemu exit code: {code}")),
                None => Verdict::Fail("qemu terminated by signal".into()),
            };
        }

        let buffered = buffer.lock().unwrap();
        if !markers.is_empty() && markers.iter().all(|m| buffered.contains(m.as_str())) {
            break Verdict::Pass(format!("all markers found: {markers:?}"));
        }
        drop(buffered);

        if Instant::now() >= deadline {
            if markers.is_empty() {
                break Verdict::Observe(format!(
                    "kernel still running after {timeout:?} (no markers given)"
                ));
            }
            let buffered = buffer.lock().unwrap();
            let missing: Vec<&String> = markers
                .iter()
                .filter(|m| !buffered.contains(m.as_str()))
                .collect();
            break Verdict::Fail(format!(
                "timeout after {timeout:?}, markers not found: {missing:?}"
            ));
        }
        thread::sleep(Duration::from_millis(50));
    };

    // QemuGuard::drop kills the child here, even on early breaks above.
    match verdict {
        Verdict::Pass(reason) => {
            print_line(&format!("[runner] PASS: {reason}"));
        }
        Verdict::Fail(reason) => {
            print_line(&format!("[runner] FAIL: {reason}"));
            // `std::process::exit` skips destructors, so kill QEMU
            // explicitly here; otherwise the child keeps running and
            // holds the disk image file open.
            drop(qemu);
            std::process::exit(1);
        }
        Verdict::Observe(reason) => {
            print_line(&format!("[runner] OBSERVE: {reason}"));
        }
    }
}
