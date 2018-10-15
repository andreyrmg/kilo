pub mod unix {
    extern crate libc;

    use std::io;
    use std::io::prelude::*;
    use std::mem;

    fn tcgetattr() -> Result<libc::termios, io::Error> {
        let mut termios = unsafe { mem::uninitialized() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut termios) } == 0 {
            Ok(termios)
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn tcsetattr(termios: &libc::termios) -> Result<(), io::Error> {
        if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, termios) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[macro_use]
    mod vt100 {
        use std::io;
        use std::io::prelude::*;
        use std::str;

        macro_rules! csi {
            ($cmd:expr) => {
                concat!("\x1b[", $cmd)
            };
            ($fmt:expr, $($args:tt)*) => {
                format!(concat!("\x1b[", $fmt), $($args)*)
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
            ($row:expr, $col:expr) => {
                csi!("{};{}H", $row, $col)
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

        pub fn get_cursor_position(
            stdin: io::StdinLock,
            mut stdout: io::StdoutLock,
        ) -> Result<(u16, u16), io::Error> {
            stdout.write_all(
                concat!(
                    cursor_forward!(999),
                    cursor_down!(999),
                    report_device_status!(active_position)
                ).as_bytes(),
            )?;
            stdout.flush()?;

            let mut buf = vec![];
            let read = stdin.take(2 + 5 + 1 + 5 + 1).read_until(b'R', &mut buf)?;

            let bad_cpr = || io::Error::new(io::ErrorKind::Other, format!("bad CPR: {:?}", buf));
            if read < 5 || read > 2 + 5 + 1 + 5 {
                return Err(bad_cpr());
            }
            if buf[0] != b'\x1b' || buf[1] != b'[' {
                return Err(bad_cpr());
            }
            let mid = buf.iter().position(|&b| b == b';').ok_or_else(bad_cpr)?;
            let rows = unsafe {
                str::from_utf8_unchecked(&buf[2..mid])
                    .parse()
                    .map_err(|_| bad_cpr())?
            };
            let cols = unsafe {
                str::from_utf8_unchecked(&buf[mid + 1..read - 1])
                    .parse()
                    .map_err(|_| bad_cpr())?
            };

            return Ok((rows, cols));
        }
    }

    pub struct Terminal {
        orig: libc::termios,
        stdin: io::Stdin,
        stdout: io::Stdout,
        buf: String,
    }

    impl Terminal {
        pub fn new_raw_mode() -> Result<Terminal, io::Error> {
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
                stdin: io::stdin(),
                stdout: io::stdout(),
                buf: String::new(),
            })
        }

        pub fn get_window_size(&self) -> Result<(u16, u16), io::Error> {
            let ws: libc::winsize = unsafe { mem::uninitialized() };
            if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &ws) } == -1 {
                vt100::get_cursor_position(self.stdin.lock(), self.stdout.lock())
            } else {
                Ok((ws.ws_row, ws.ws_col))
            }
        }

        pub fn read_key(&self) -> Option<Result<u8, io::Error>> {
            let stdin = self.stdin.lock();
            let mut bytes = stdin.bytes().skip_while(|x| {
                x.as_ref()
                    .err()
                    .and_then(io::Error::raw_os_error)
                    .map(|raw_os_error| raw_os_error == libc::EAGAIN)
                    .unwrap_or(false)
            });
            bytes.next()
        }

        pub fn begin(&mut self) {
            self.buf = String::new();
        }

        pub fn end(&self) -> Result<(), io::Error> {
            let mut stdout = io::stdout();
            stdout.write_all(self.buf.as_bytes())?;
            stdout.flush()
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

        pub fn move_cursor_at(&mut self, row: u32, col: u32) {
            self.buf.push_str(&cursor_position!(row, col))
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
