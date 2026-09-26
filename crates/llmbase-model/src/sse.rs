use crate::ModelError;

/// Incremental SSE framing; decoding UTF-8 only after a complete line permits
/// arbitrary HTTP splits inside a scalar, CRLF, JSON, or the DONE marker.
pub(crate) struct Decoder {
    line: Vec<u8>,
    data: String,
    frame_bytes: usize,
    limit: usize,
    skip_lf: bool,
    first_line: bool,
}

impl Decoder {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            line: Vec::new(),
            data: String::new(),
            frame_bytes: 0,
            limit,
            skip_lf: false,
            first_line: true,
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, ModelError> {
        let mut events = Vec::new();
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            self.frame_bytes = self
                .frame_bytes
                .checked_add(1)
                .ok_or(ModelError::ResponseTooLarge)?;
            if self.frame_bytes > self.limit {
                return Err(ModelError::ResponseTooLarge);
            }
            if byte == b'\r' || byte == b'\n' {
                self.skip_lf = byte == b'\r';
                if let Some(event) = self.end_line()? {
                    events.push(event);
                }
            } else {
                self.line.push(byte);
            }
        }
        Ok(events)
    }

    fn end_line(&mut self) -> Result<Option<String>, ModelError> {
        let mut line = std::str::from_utf8(&self.line)
            .map_err(|_| ModelError::Protocol("SSE contains invalid UTF-8"))?;
        if self.first_line {
            line = line.trim_start_matches('\u{feff}');
            self.first_line = false;
        }
        let result = if line.is_empty() {
            self.frame_bytes = 0;
            if self.data.is_empty() {
                None
            } else {
                self.data.pop(); // SSE joins data lines with LF, without final LF.
                Some(std::mem::take(&mut self.data))
            }
        } else {
            if let Some(value) = line.strip_prefix("data:") {
                self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
                self.data.push('\n');
            } else if line == "data" {
                self.data.push('\n');
            }
            None
        };
        self.line.clear();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_split_and_all_sse_newlines() {
        let mut decoder = Decoder::new(1000);
        let mut events = Vec::new();
        for byte in
            "\u{feff}:heartbeat\r\ndata: 文𠮷\r\ndata: 二\r\n\r\ndata: [DONE]\r\r".as_bytes()
        {
            events.extend(decoder.push(&[*byte]).unwrap());
        }
        assert_eq!(events, ["文𠮷\n二", "[DONE]"]);
    }

    #[test]
    fn bounds_comments_and_unterminated_frames() {
        let mut decoder = Decoder::new(10);
        assert_eq!(
            decoder.push(b":1234567890").unwrap_err(),
            ModelError::ResponseTooLarge
        );
    }
}
