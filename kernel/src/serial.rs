use core::fmt::{self, Write};

use spin::Mutex;
use uart_16550::{Config, Uart16550Tty, backend::PioBackend};

type SerialPort = Uart16550Tty<PioBackend>;

static SERIAL1: Mutex<Option<SerialPort>> = Mutex::new(None);

pub fn init() {
    let serial = unsafe {
        Uart16550Tty::new_port(0x3f8, Config::default()).expect("failed to initialize COM1")
    };

    *SERIAL1.lock() = Some(serial);
}

pub fn write(args: fmt::Arguments<'_>) {
    let mut serial = SERIAL1.lock();

    if serial.is_none() {
        let initialized = unsafe {
            Uart16550Tty::new_port(0x3f8, Config::default()).expect("failed to initialize COM1")
        };

        *serial = Some(initialized);
    }

    if let Some(serial) = serial.as_mut() {
        let _ = serial.write_fmt(args);
    }
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {{
        $crate::serial::write(format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! serial_println {
    () => {{
        $crate::serial::write(format_args!("\n"));
    }};
    ($($arg:tt)*) => {{
        $crate::serial::write(
            format_args!("{}\n", format_args!($($arg)*))
        );
    }};
}
