//! One process-wide, bounded reader for stdin lines.
//!
//! Every interactive builtin reads its answer through this module, so there is
//! exactly one owner of stdin. A single reader thread, started on first use,
//! reads one line each time a caller asks for one, and holds the stdin lock
//! only while that read is in progress. It never reads ahead, so the REPL and
//! the POSIX runner, which read stdin directly, still get every line after it.
//!
//! Lines are capped at [`MAX_LINE_BYTES`]. An over-long line is read to its
//! newline and discarded, and the caller gets [`LineError::TooLong`]; the next
//! read then gets the line after it.
//!
//! A timed read that gives up does not discard anything. A line typed after a
//! timed-out prompt answers the next prompt.

use std::io::BufRead;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use anyhow::Result;

/// Longest line, in bytes, excluding its line terminator.
pub(crate) const MAX_LINE_BYTES: usize = 64 * 1024;

/// Why an item is not a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LineError {
    /// A line longer than [`MAX_LINE_BYTES`]. Its bytes were discarded.
    TooLong,
    /// End of input. Nothing follows it.
    Eof,
    /// A read failure. Nothing follows it.
    Io(String),
}

type Item = Result<String, LineError>;

/// Read the next item from `reader`.
///
/// Strips `\n` and `\r\n`. A final line without a newline is returned as a
/// line. Once end of input has been reported, further calls report it again.
pub(crate) fn read_item<R: BufRead>(reader: &mut R) -> Item {
    let mut line: Vec<u8> = Vec::new();
    // Set once the line passes the cap. The rest of the line is then consumed
    // without being stored, so memory stays bounded by the cap.
    let mut too_long = false;
    loop {
        let buf = match reader.fill_buf() {
            Ok(buf) => buf,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(LineError::Io(e.to_string())),
        };
        if buf.is_empty() {
            // End of input.
            if too_long {
                return Err(LineError::TooLong);
            }
            if line.is_empty() {
                return Err(LineError::Eof);
            }
            return finish(line);
        }
        let newline = buf.iter().position(|&b| b == b'\n');
        let take = newline.unwrap_or(buf.len());
        if !too_long {
            line.extend_from_slice(&buf[..take]);
            // One byte of slack for a `\r` that the terminator strips. Past
            // that the line is over the cap whatever comes next.
            if line.len() > MAX_LINE_BYTES + 1 {
                too_long = true;
                line = Vec::new();
            }
        }
        let used = match newline {
            Some(i) => i + 1,
            None => buf.len(),
        };
        reader.consume(used);
        if newline.is_some() {
            if too_long {
                return Err(LineError::TooLong);
            }
            return finish(line);
        }
    }
}

/// Strip a trailing `\r` and check the cap on what remains.
fn finish(mut line: Vec<u8>) -> Item {
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    if line.len() > MAX_LINE_BYTES {
        return Err(LineError::TooLong);
    }
    Ok(String::from_utf8_lossy(&line).into_owned())
}

/// The reader thread's two channels.
struct Reader {
    /// One message asks for one line.
    request: SyncSender<()>,
    reply: Receiver<Item>,
    /// A request whose line has not been taken: a timed read gave up on it.
    /// The next read takes that line instead of asking for another.
    outstanding: bool,
}

/// The shared reader, or the reason its thread could not start.
static READER: OnceLock<Result<Mutex<Reader>, String>> = OnceLock::new();

fn reader() -> std::result::Result<&'static Mutex<Reader>, String> {
    READER
        .get_or_init(|| {
            let (request, requests): (SyncSender<()>, Receiver<()>) = mpsc::sync_channel(1);
            let (replies, reply): (SyncSender<Item>, Receiver<Item>) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("ae-stdin-reader".to_string())
                .spawn(move || {
                    let stdin = std::io::stdin();
                    while requests.recv().is_ok() {
                        // Locked per line, so stdin is free between requests.
                        let item = read_item(&mut stdin.lock());
                        let last = matches!(item, Err(LineError::Eof | LineError::Io(_)));
                        if replies.send(item).is_err() || last {
                            break;
                        }
                    }
                })
                .map(|_| {
                    Mutex::new(Reader {
                        request,
                        reply,
                        outstanding: false,
                    })
                })
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// Read the next line from stdin, bounded, through the shared reader.
///
/// `Ok(None)` only when `timeout` passes first. The line typed later stays
/// queued and answers the next read. With `timeout` of `None` the call blocks.
///
/// Errors: an over-long line is `E_BUDGET_EXCEEDED`; end of input, or no
/// reader, is `E_NO_UI`; a read failure is `E_IO`.
pub(crate) fn read_line(builtin: &str, timeout: Option<Duration>) -> Result<Option<String>> {
    let reader = match reader() {
        Ok(reader) => reader,
        Err(msg) => {
            return Err(crate::safety::fs_error(
                builtin,
                "stdin",
                &std::io::Error::other(format!("cannot start the stdin reader: {msg}")),
            ))
        }
    };
    let mut reader = reader.lock().unwrap_or_else(|e| e.into_inner());
    // A send fails only once the thread has ended at end of input or an
    // error; the reply channel is then empty and reports Disconnected.
    if !reader.outstanding && reader.request.send(()).is_ok() {
        reader.outstanding = true;
    }
    let item = match timeout {
        None => match reader.reply.recv() {
            Ok(item) => item,
            Err(_) => Err(LineError::Eof),
        },
        Some(limit) => match reader.reply.recv_timeout(limit) {
            Ok(item) => item,
            Err(RecvTimeoutError::Timeout) => return Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(LineError::Eof),
        },
    };
    reader.outstanding = false;
    match item {
        Ok(line) => Ok(Some(line)),
        Err(LineError::TooLong) => Err(crate::safety::budget_exceeded(
            builtin,
            format!("an input line longer than {} KiB", MAX_LINE_BYTES / 1024),
            "the line was discarded up to its newline; answer with a shorter line",
        )),
        Err(LineError::Eof) => Err(crate::safety::no_ui(builtin)),
        Err(LineError::Io(msg)) => Err(crate::safety::fs_error(
            builtin,
            "stdin",
            &std::io::Error::other(msg),
        )),
    }
}

/// Block for the next line. End of input is `E_NO_UI`.
pub(crate) fn read_blocking(builtin: &str) -> Result<String> {
    read_line(builtin, None)?.ok_or_else(|| crate::safety::no_ui(builtin))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Every item the splitter produces from `bytes`, up to and including the
    /// first Eof or Io, as the reader thread would send them.
    fn items(bytes: &[u8]) -> Vec<Item> {
        let mut r = Cursor::new(bytes.to_vec());
        let mut out = Vec::new();
        loop {
            let item = read_item(&mut r);
            let last = matches!(item, Err(LineError::Eof | LineError::Io(_)));
            out.push(item);
            if last {
                return out;
            }
        }
    }

    fn ok(s: &str) -> Item {
        Ok(s.to_string())
    }

    #[test]
    fn plain_lines_and_eof() {
        assert_eq!(
            items(b"one\ntwo\n"),
            vec![ok("one"), ok("two"), Err(LineError::Eof)]
        );
    }

    #[test]
    fn crlf_is_stripped_and_lone_cr_is_kept() {
        assert_eq!(
            items(b"a\r\nb\rc\r\n"),
            vec![ok("a"), ok("b\rc"), Err(LineError::Eof)]
        );
    }

    #[test]
    fn final_line_without_newline_is_a_line() {
        assert_eq!(
            items(b"first\nlast"),
            vec![ok("first"), ok("last"), Err(LineError::Eof)]
        );
        assert_eq!(items(b"tail\r"), vec![ok("tail"), Err(LineError::Eof)]);
    }

    #[test]
    fn empty_lines_are_lines() {
        assert_eq!(
            items(b"\n\r\n\n"),
            vec![ok(""), ok(""), ok(""), Err(LineError::Eof)]
        );
    }

    #[test]
    fn empty_input_is_eof() {
        assert_eq!(items(b""), vec![Err(LineError::Eof)]);
    }

    #[test]
    fn exactly_the_cap_is_accepted() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES];
        bytes.push(b'\n');
        let got = items(&bytes);
        assert_eq!(got[0].as_ref().map(|s| s.len()), Ok(MAX_LINE_BYTES));
        assert_eq!(got[1], Err(LineError::Eof));
    }

    #[test]
    fn exactly_the_cap_with_crlf_is_accepted() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES];
        bytes.extend_from_slice(b"\r\n");
        let got = items(&bytes);
        assert_eq!(got[0].as_ref().map(|s| s.len()), Ok(MAX_LINE_BYTES));
    }

    #[test]
    fn one_byte_over_the_cap_is_too_long() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES + 1];
        bytes.push(b'\n');
        bytes.extend_from_slice(b"next\n");
        assert_eq!(
            items(&bytes),
            vec![Err(LineError::TooLong), ok("next"), Err(LineError::Eof)]
        );
    }

    #[test]
    fn one_byte_over_the_cap_with_crlf_is_too_long() {
        let mut bytes = vec![b'x'; MAX_LINE_BYTES + 1];
        bytes.extend_from_slice(b"\r\n");
        assert_eq!(items(&bytes)[0], Err(LineError::TooLong));
    }

    #[test]
    fn a_megabyte_line_is_discarded_and_the_next_line_arrives() {
        let mut bytes = vec![b'y'; 1024 * 1024];
        bytes.extend_from_slice(b"\nnormal\n");
        assert_eq!(
            items(&bytes),
            vec![Err(LineError::TooLong), ok("normal"), Err(LineError::Eof)]
        );
    }

    #[test]
    fn an_over_long_final_line_without_newline_is_too_long_then_eof() {
        let bytes = vec![b'z'; MAX_LINE_BYTES + 10];
        assert_eq!(
            items(&bytes),
            vec![Err(LineError::TooLong), Err(LineError::Eof)]
        );
    }

    #[test]
    fn a_line_that_crosses_buffer_boundaries_is_joined() {
        // BufReader's default buffer is 8 KiB, so a 20 KiB line spans several
        // fill_buf calls.
        let mut bytes = vec![b'a'; 20 * 1024];
        bytes.extend_from_slice(b"\nb\n");
        let got = items(&bytes);
        assert_eq!(got[0].as_ref().map(|s| s.len()), Ok(20 * 1024));
        assert_eq!(got[1], ok("b"));
    }

    #[test]
    fn io_errors_are_reported_once_and_end_the_stream() {
        struct Broken;
        impl std::io::Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("device gone"))
            }
        }
        let mut r = std::io::BufReader::new(Broken);
        match read_item(&mut r) {
            Err(LineError::Io(msg)) => assert!(msg.contains("device gone")),
            other => panic!("expected Io, got {other:?}"),
        }
    }
}
