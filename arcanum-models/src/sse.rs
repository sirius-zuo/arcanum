//! Server-Sent Events line parser used by the streaming generators.

use arcanum_core::ArcanumError;
use futures::stream::BoxStream;
use futures::{StreamExt, TryStreamExt};

#[derive(Debug, Clone, PartialEq)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

/// Incremental parser. Holds raw bytes so a multibyte character split across
/// network chunks is only decoded once its whole line has arrived.
#[derive(Default)]
pub struct SseParser {
    buf: Vec<u8>,
    event: Option<String>,
    data: Option<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buf.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.buf.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let line = String::from_utf8_lossy(&line);
            if line.is_empty() {
                if let Some(data) = self.data.take() {
                    out.push(SseEvent {
                        event: self.event.take(),
                        data,
                    });
                } else {
                    self.event = None;
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
                None => (line.as_ref(), ""),
            };
            match field {
                "data" => match &mut self.data {
                    Some(d) => {
                        d.push('\n');
                        d.push_str(value);
                    }
                    None => self.data = Some(value.to_string()),
                },
                "event" => self.event = Some(value.to_string()),
                _ => {}
            }
        }
        out
    }
}

#[allow(dead_code)] // first used by the generators in the next commits
pub(crate) fn sse_events(
    resp: reqwest::Response,
) -> BoxStream<'static, arcanum_core::Result<SseEvent>> {
    let mut parser = SseParser::new();
    resp.bytes_stream()
        .map_err(|e| ArcanumError::Generation(e.to_string()))
        .map_ok(move |chunk| futures::stream::iter(parser.push(&chunk).into_iter().map(Ok)))
        .try_flatten()
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(event: Option<&str>, data: &str) -> SseEvent {
        SseEvent {
            event: event.map(String::from),
            data: data.to_string(),
        }
    }

    #[test]
    fn multi_line_data_is_joined() {
        let mut p = SseParser::new();
        assert_eq!(p.push(b"data: a\ndata: b\n\n"), vec![ev(None, "a\nb")]);
    }

    #[test]
    fn comments_and_unknown_fields_ignored() {
        let mut p = SseParser::new();
        assert_eq!(
            p.push(b": ping\nid: 7\nevent: x\ndata: y\n\n"),
            vec![ev(Some("x"), "y")]
        );
    }

    #[test]
    fn crlf_lines() {
        let mut p = SseParser::new();
        assert_eq!(
            p.push(b"event: e\r\ndata: d\r\n\r\n"),
            vec![ev(Some("e"), "d")]
        );
    }

    #[test]
    fn line_split_across_pushes() {
        let mut p = SseParser::new();
        assert!(p.push(b"da").is_empty());
        assert!(p.push(b"ta: hel").is_empty());
        assert_eq!(p.push(b"lo\n\n"), vec![ev(None, "hello")]);
    }

    #[test]
    fn multibyte_split_across_pushes() {
        let mut p = SseParser::new();
        assert!(p.push(b"data: caf\xC3").is_empty());
        assert_eq!(p.push(b"\xA9\n\n"), vec![ev(None, "café")]);
    }

    #[test]
    fn no_dispatch_without_blank_line() {
        let mut p = SseParser::new();
        assert!(p.push(b"data: x\n").is_empty());
    }
}
