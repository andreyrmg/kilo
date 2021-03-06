use std::cmp;
use std::env;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::path;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

const CTRL: u8 = 0x1f;
const CTRL_Q: u8 = CTRL & b'q';

#[derive(PartialEq)]
pub enum Key {
    Char(u8),
    Escape,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Delete,
}

struct Editor {
    term: target::Terminal,
    screen_rows: usize,
    screen_cols: usize,
    cursor_row: usize,
    cursor_col: usize,
    rows: Vec<String>,
}

impl Editor {
    fn new() -> Result<Editor, io::Error> {
        let term = target::Terminal::new_raw_mode()?;
        let (rows, cols) = term.get_window_size()?;
        Ok(Editor {
            term: term,
            screen_rows: rows as usize,
            screen_cols: cols as usize,
            cursor_row: 0,
            cursor_col: 0,
            rows: vec![],
        })
    }

    fn open<P>(&mut self, path: P) -> Result<(), io::Error>
    where
        P: AsRef<path::Path>,
    {
        let file = fs::File::open(path)?;
        let reader = io::BufReader::new(file);
        self.rows = reader.lines().collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }

    fn move_cursor(&mut self, key: Key) {
        match key {
            Key::Left => if self.cursor_col != 0 {
                self.cursor_col -= 1
            },
            Key::Right => if self.cursor_col != self.screen_cols - 1 {
                self.cursor_col += 1
            },
            Key::Up => if self.cursor_row != 0 {
                self.cursor_row -= 1
            },
            Key::Down => if self.cursor_row != self.screen_rows - 1 {
                self.cursor_row += 1
            },
            _ => (),
        }
    }

    fn draw_rows(&mut self) {
        for y in 0..self.screen_rows {
            if y >= self.rows.len() {
                if self.rows.is_empty() && y == self.screen_rows / 3 {
                    let welcome = format!("Kilo editor -- version {}", VERSION);
                    let len = welcome.len().min(self.screen_cols);
                    let mut padding = (self.screen_cols - len) / 2;
                    if padding > 0 {
                        self.term.push('~');
                        padding -= 1;
                    }
                    for _ in 0..padding {
                        self.term.push(' ');
                    }
                    self.term.push_str(&welcome[..len]);
                } else {
                    self.term.push('~');
                }
            } else {
                let len = cmp::min(self.rows[y].len(), self.screen_cols);
                self.term.push_str(&self.rows[y][..len])
            }
            self.term.erase_in_line();
            if y < self.screen_rows - 1 {
                self.term.push_str("\r\n");
            }
        }
    }

    fn refresh_screen(&mut self) -> Result<(), io::Error> {
        self.term.begin();

        self.term.hide_cursor();
        self.term.move_cursor();

        self.draw_rows();

        self.term
            .move_cursor_at(self.cursor_row + 1, self.cursor_col + 1);
        self.term.show_cursor();

        self.term.end()?;
        Ok(())
    }

    fn run(&mut self) -> Result<(), io::Error> {
        loop {
            self.refresh_screen()?;
            let key = self.term.read_key()?;
            match key {
                Key::Up | Key::Down | Key::Right | Key::Left => self.move_cursor(key),
                Key::PageUp | Key::PageDown => {
                    let times = self.screen_rows;
                    for _ in 0..times {
                        self.move_cursor(if key == Key::PageUp {
                            Key::Up
                        } else {
                            Key::Down
                        });
                    }
                }
                Key::Home => self.cursor_col = 0,
                Key::End => self.cursor_col = self.screen_cols - 1,
                Key::Char(CTRL_Q) => {
                    self.term.begin();
                    self.term.erase_in_display();
                    self.term.move_cursor();
                    self.term.end()?;
                    return Ok(());
                }
                _ => (),
            }
        }
    }
}

mod platform {

    #[cfg(unix)]
    pub mod unix {
        extern crate libc;

        use super::super::Key;
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

                let bad_cpr =
                    || io::Error::new(io::ErrorKind::Other, format!("bad CPR: {:?}", buf));
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

                raw.c_iflag &=
                    !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
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

            pub fn read_key(&self) -> Result<Key, io::Error> {
                let stdin = self.stdin.lock();
                let mut bytes = stdin.bytes().filter(|x| {
                    x.as_ref()
                        .err()
                        .and_then(io::Error::raw_os_error)
                        .map(|raw_os_error| raw_os_error != libc::EAGAIN)
                        .unwrap_or(true)
                });
                loop {
                    if let Some(next) = bytes.next() {
                        let b = next?;
                        if b != b'\x1b' {
                            return Ok(Key::Char(b));
                        }
                        if let Some(next) = bytes.next() {
                            match next? {
                                b'[' => {
                                    if let Some(next) = bytes.next() {
                                        match next? {
                                            b'A' => return Ok(Key::Up),
                                            b'B' => return Ok(Key::Down),
                                            b'C' => return Ok(Key::Right),
                                            b'D' => return Ok(Key::Left),
                                            b'H' => return Ok(Key::Home),
                                            b'F' => return Ok(Key::End),
                                            b @ b'0'...b'9' => if let Some(next) = bytes.next() {
                                                if next? == b'~' {
                                                    match b {
                                                        b'1' | b'7' => return Ok(Key::Home),
                                                        b'3' => return Ok(Key::Delete),
                                                        b'4' | b'8' => return Ok(Key::End),
                                                        b'5' => return Ok(Key::PageUp),
                                                        b'6' => return Ok(Key::PageDown),
                                                        _ => (),
                                                    }
                                                }
                                            },
                                            _ => (),
                                        }
                                    }
                                }
                                b'O' => {
                                    if let Some(next) = bytes.next() {
                                        match next? {
                                            b'H' => return Ok(Key::Home),
                                            b'F' => return Ok(Key::End),
                                            _ => (),
                                        }
                                    }
                                }
                                _ => (),
                            }
                        }
                        return Ok(Key::Escape);
                    }
                }
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

            pub fn move_cursor_at(&mut self, row: usize, col: usize) {
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
}

#[cfg(unix)]
use platform::unix as target;

fn main() -> Result<(), io::Error> {
    let mut editor = Editor::new()?;

    let mut args = env::args();
    if let Some(path) = args.nth(1) {
        editor.open(path)?;
    }

    editor.run()
}
