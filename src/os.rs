pub mod unix {
    extern crate libc;

    use std::error::Error;
    use std::ffi::CStr;
    use std::fmt;
    use std::mem;

    #[derive(Debug)]
    pub struct SysError(i32, String);

    impl SysError {
        fn last(s: &str) -> SysError {
            unsafe {
                let err_code = errno();
                let err_string = libc::strerror(err_code);
                SysError(
                    err_code,
                    s.to_string() + ": " + &CStr::from_ptr(err_string).to_string_lossy(),
                )
            }
        }
    }
    impl fmt::Display for SysError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{} (code: {})", self.1, self.0)
        }
    }

    impl Error for SysError {}

    fn errno() -> i32 {
        unsafe { *libc::__errno_location() }
    }

    pub fn read() -> Result<Option<u8>, SysError> {
        let mut c = unsafe { mem::uninitialized() };
        let res =
            unsafe { libc::read(libc::STDIN_FILENO, &mut c as *mut _ as *mut libc::c_void, 1) };
        match res {
            1 => Ok(Some(c)),
            0 => Ok(None),
            -1 if errno() == libc::EAGAIN => Ok(None),
            _ => Err(SysError::last("read")),
        }
    }

    pub fn read_match(b: u8) -> Result<(), SysError> {
        let mut c: u8 = unsafe { mem::uninitialized() };
        let res =
            unsafe { libc::read(libc::STDIN_FILENO, &mut c as *mut _ as *mut libc::c_void, 1) };
        match res {
            1 if c == b => Ok(()),
            1 => Err(SysError(
                libc::EIO,
                format!("read_match: expected b'{}' but read b'{}'", b, c),
            )),
            0 => Err(SysError(
                libc::EIO,
                format!("read_match: expected b'{}' but read nothing", b),
            )),
            _ => Err(SysError::last("read_match")),
        }
    }

    fn read_exact() -> Result<u8, SysError> {
        let mut c = unsafe { mem::uninitialized() };
        let res =
            unsafe { libc::read(libc::STDIN_FILENO, &mut c as *mut _ as *mut libc::c_void, 1) };
        match res {
            1 => Ok(c),
            0 => Err(SysError(libc::EIO, "read_exact: read nothing".to_string())),
            _ => Err(SysError::last("read_exact")),
        }
    }

    pub fn write(buf: &str) -> Result<(), SysError> {
        let res = unsafe { libc::write(libc::STDOUT_FILENO, buf.as_ptr() as *const _, buf.len()) };
        match res {
            -1 => Err(SysError::last("write")),
            _ => Ok(()),
        }
    }

    fn tcgetattr() -> Result<libc::termios, SysError> {
        let mut termios = unsafe { mem::uninitialized() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut termios) } == 0 {
            Ok(termios)
        } else {
            Err(SysError::last("tcgetattr"))
        }
    }

    fn tcsetattr(termios: &libc::termios) -> Result<(), SysError> {
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, termios) } == 0 {
            Ok(())
        } else {
            Err(SysError::last("tcsetattr"))
        }
    }

    #[macro_use]
    mod vt100 {
        use super::{read_exact, read_match, write, SysError};

        macro_rules! csi {
            ($cmd:expr) => {
                concat!("\x1b[", $cmd)
            };
        }
        macro_rules! cursor_forward {
            ($n:expr) => {
                csi!(concat!($n, "C"))
            };
        }
        macro_rules! cursor_down {
            ($n:expr) => {
                csi!(concat!($n, "B"))
            };
        }
        macro_rules! cursor_position {
            () => {
                csi!("H")
            };
        }
        macro_rules! erase_in_display {
            () => {
                csi!("2J")
            };
        }
        macro_rules! erase_in_line {
            () => {
                csi!("K")
            };
        }
        macro_rules! report_device_status {
            (active_position) => {
                csi!(concat!("6n"))
            };
        }
        macro_rules! set_mode {
            (hide_cursor) => {
                csi!("?25l")
            };
            (show_cursor) => {
                csi!("?25h")
            };
        }

        pub fn get_cursor_position() -> Result<(u16, u16), SysError> {
            write(concat!(
                cursor_forward!(999),
                cursor_down!(999),
                report_device_status!(active_position)
            ))?;
            read_match(b'\x1b')?;
            read_match(b'[')?;
            let mut rows = 0u16;
            loop {
                match read_exact()? {
                    b';' => break,
                    b => rows = rows * 10 + (b - b'0') as u16,
                }
            }
            let mut cols = 0u16;
            loop {
                match read_exact()? {
                    b'R' => break,
                    b => cols = cols * 10 + (b - b'0') as u16,
                }
            }
            Ok((rows, cols))
        }
    }

    pub struct Terminal {
        orig: libc::termios,
        buf: String,
    }

    impl Terminal {
        pub fn new_raw_mode() -> Result<Terminal, SysError> {
            let orig = tcgetattr()?;

            let mut raw = orig;

            raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
            raw.c_oflag &= !libc::OPOST;
            raw.c_cflag |= libc::CS8;
            raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
            raw.c_cc[libc::VMIN] = 0;
            raw.c_cc[libc::VTIME] = 1;

            tcsetattr(&raw)?;

            Ok(Terminal {
                orig: orig,
                buf: String::new(),
            })
        }

        pub fn get_window_size(&self) -> Result<(u16, u16), SysError> {
            let ws: libc::winsize = unsafe { mem::uninitialized() };
            if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &ws) } == -1 {
                write(concat!(cursor_forward!(999), cursor_down!(999)))?;
                vt100::get_cursor_position()
            } else {
                Ok((ws.ws_row, ws.ws_col))
            }
        }

        pub fn begin(&mut self) {
            self.buf = String::new();
        }

        pub fn end(&self) -> Result<(), SysError> {
            write(&self.buf)
        }

        pub fn erase_in_display(&mut self) {
            self.buf.push_str(erase_in_display!());
        }

        pub fn erase_in_line(&mut self) {
            self.buf.push_str(erase_in_line!());
        }

        pub fn hide_cursor(&mut self) {
            self.buf.push_str(set_mode!(hide_cursor))
        }

        pub fn show_cursor(&mut self) {
            self.buf.push_str(set_mode!(show_cursor))
        }

        pub fn move_cursor(&mut self) {
            self.buf.push_str(cursor_position!())
        }

        pub fn push(&mut self, ch: char) {
            self.buf.push(ch);
        }

        pub fn push_str(&mut self, s: &str) {
            self.buf.push_str(s);
        }
    }

    impl Drop for Terminal {
        fn drop(&mut self) {
            tcsetattr(&self.orig).unwrap();
        }
    }

}
#[cfg(unix)]
pub use self::unix as target;
