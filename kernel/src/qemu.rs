use x86_64::instructions::{hlt, port::Port};

const QEMU_EXIT_PORT: u16 = 0xf4;

// `Success`/`exit_success` are reserved for later test runs where the
// kernel intentionally finishes with a positive result.
#[allow(dead_code)]
#[repr(u32)]
#[derive(Clone, Copy, Debug)]
pub enum ExitCode {
    Success = 0x10,
    Failure = 0x11,
}

pub fn exit(code: ExitCode) -> ! {
    unsafe {
        let mut port = Port::<u32>::new(QEMU_EXIT_PORT);
        port.write(code as u32);
    }

    halt_loop()
}

#[allow(dead_code)]
pub fn exit_success() -> ! {
    exit(ExitCode::Success)
}

pub fn exit_failure() -> ! {
    exit(ExitCode::Failure)
}

fn halt_loop() -> ! {
    loop {
        hlt();
    }
}
