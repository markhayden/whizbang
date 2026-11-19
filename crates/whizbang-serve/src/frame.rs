//! A message read from the server's input (ADR 0031, "What is read"): one line of
//! JSON, at most [`MESSAGE_LIMIT`] bytes, nesting at most [`DEPTH_LIMIT`] deep. What a
//! client writes is not trusted: a line over the limit is read to its end and dropped
//! without being held, and a line nested deeper is refused before it is parsed, so
//! neither the memory a line takes nor the depth a parser recurses to is the client's
//! to choose. Nothing of a refused line is said back.

use std::io::{self, BufRead};

use serde_json::Value as Json;

/// The most bytes of one message, its line break not counted.
pub const MESSAGE_LIMIT: usize = 1024 * 1024;
/// The deepest a message nests its arrays and objects.
pub const DEPTH_LIMIT: usize = 16;

/// What one line of the input is.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// A JSON object.
    Message(serde_json::Map<String, Json>),
    /// A line over [`MESSAGE_LIMIT`] bytes: read and dropped.
    TooLarge,
    /// A line nested deeper than [`DEPTH_LIMIT`].
    TooDeep,
    /// A line that is not JSON, or JSON that is not an object.
    NotJson,
    /// The input ended.
    End,
}

/// The next message of `input`. An empty line is passed over. A last line with no
/// line break is a message as any other.
pub fn next(input: &mut impl BufRead) -> io::Result<Frame> {
    loop {
        let mut line = Vec::new();
        let mut over = false;
        let mut ended = false;
        loop {
            let available = match input.fill_buf() {
                Ok(available) => available,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if available.is_empty() {
                ended = true;
                break;
            }
            let (chunk, found) = match available.iter().position(|byte| *byte == b'\n') {
                Some(at) => (&available[..at], true),
                None => (available, false),
            };
            if !over && line.len() + chunk.len() > MESSAGE_LIMIT {
                over = true;
                line = Vec::new();
            }
            if !over {
                line.extend_from_slice(chunk);
            }
            let used = chunk.len() + usize::from(found);
            input.consume(used);
            if found {
                break;
            }
        }
        if over {
            return Ok(Frame::TooLarge);
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            if ended {
                return Ok(Frame::End);
            }
            continue;
        }
        return Ok(parsed(&line));
    }
}

/// One line, parsed once its depth is known to be within the limit.
pub fn parsed(line: &[u8]) -> Frame {
    if depth(line) > DEPTH_LIMIT {
        return Frame::TooDeep;
    }
    match serde_json::from_slice::<Json>(line) {
        Ok(Json::Object(message)) => Frame::Message(message),
        _ => Frame::NotJson,
    }
}

/// How deep the arrays and objects of a line nest, read outside its strings. A line
/// that is not JSON is given some depth; the parser refuses it afterwards.
pub fn depth(line: &[u8]) -> usize {
    let (mut now, mut most) = (0usize, 0usize);
    let (mut quoted, mut escaped) = (false, false);
    for byte in line {
        if quoted {
            match (escaped, byte) {
                (true, _) => escaped = false,
                (false, b'\\') => escaped = true,
                (false, b'"') => quoted = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'[' | b'{' => {
                now += 1;
                most = most.max(now);
            }
            b']' | b'}' => now = now.saturating_sub(1),
            _ => {}
        }
    }
    most
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(input: &[u8]) -> Vec<Frame> {
        let mut reader = io::BufReader::with_capacity(64, input);
        let mut out = Vec::new();
        for _ in 0..16 {
            let frame = next(&mut reader).unwrap();
            let end = frame == Frame::End;
            out.push(frame);
            if end {
                break;
            }
        }
        out
    }

    #[test]
    fn a_line_over_the_limit_is_dropped_and_the_next_is_read() {
        let mut input = vec![b'{'];
        input.extend(std::iter::repeat_n(b' ', MESSAGE_LIMIT));
        input.extend_from_slice(b"}\n\n{\"a\":1}");
        let read = frames(&input);
        assert_eq!(read[0], Frame::TooLarge);
        assert!(matches!(&read[1], Frame::Message(m) if m["a"] == 1));
        assert_eq!(read[2], Frame::End);
    }

    #[test]
    fn nesting_past_the_limit_is_refused_before_it_is_parsed_and_brackets_in_strings_do_not_count()
    {
        let deep = format!(
            "{}{}",
            "[".repeat(DEPTH_LIMIT + 1),
            "]".repeat(DEPTH_LIMIT + 1)
        );
        assert_eq!(parsed(deep.as_bytes()), Frame::TooDeep);
        // A million brackets are refused the same way, never handed to the parser.
        assert_eq!(parsed("[".repeat(1_000_000).as_bytes()), Frame::TooDeep);
        let quoted = format!("{{\"a\":\"{}\\\"\"}}", "[".repeat(100));
        assert!(matches!(parsed(quoted.as_bytes()), Frame::Message(_)));
        assert_eq!(parsed(b"[1,2]"), Frame::NotJson);
        assert_eq!(parsed(b"{\"a\":"), Frame::NotJson);
    }
}
