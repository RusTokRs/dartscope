use std::fmt;
use std::io::{self, Write};
use std::string::FromUtf8Error;

use dartscope::JsonContract;
use serde::Serialize;

pub(super) const MAX_STRUCTURED_OUTPUT_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug)]
pub(super) enum BoundedJsonError {
    Serialization(serde_json::Error),
    InvalidUtf8(FromUtf8Error),
    LimitExceeded { max_bytes: usize },
}

impl fmt::Display for BoundedJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialization(error) => error.fmt(formatter),
            Self::InvalidUtf8(error) => error.fmt(formatter),
            Self::LimitExceeded { max_bytes } => {
                write!(
                    formatter,
                    "structured output exceeds the limit of {max_bytes} bytes"
                )
            }
        }
    }
}

pub(super) fn to_json_contract_pretty_bounded<T: Serialize + ?Sized>(
    contract: JsonContract,
    value: &T,
    max_bytes: usize,
) -> Result<String, BoundedJsonError> {
    serialize_pretty_bounded(&contract.envelope(value), max_bytes)
}

pub(super) fn to_json_pretty_bounded<T: Serialize + ?Sized>(
    value: &T,
    max_bytes: usize,
) -> Result<String, BoundedJsonError> {
    serialize_pretty_bounded(value, max_bytes)
}

fn serialize_pretty_bounded<T: Serialize + ?Sized>(
    value: &T,
    max_bytes: usize,
) -> Result<String, BoundedJsonError> {
    let mut writer = BoundedWriter::new(max_bytes);
    if let Err(error) = serde_json::to_writer_pretty(&mut writer, value) {
        return if writer.limit_exceeded {
            Err(BoundedJsonError::LimitExceeded { max_bytes })
        } else {
            Err(BoundedJsonError::Serialization(error))
        };
    }
    writer.finish()
}

struct BoundedWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
    limit_exceeded: bool,
}

impl BoundedWriter {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
            limit_exceeded: false,
        }
    }

    fn finish(self) -> Result<String, BoundedJsonError> {
        String::from_utf8(self.bytes).map_err(BoundedJsonError::InvalidUtf8)
    }

    fn limit_error(&mut self) -> io::Error {
        self.limit_exceeded = true;
        io::Error::other("structured output limit exceeded")
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(next_len) = self.bytes.len().checked_add(buffer.len()) else {
            return Err(self.limit_error());
        };
        if next_len > self.max_bytes {
            return Err(self.limit_error());
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    struct Payload {
        value: String,
    }

    #[test]
    fn accepts_output_exactly_at_the_byte_limit() {
        let payload = Payload {
            value: "bounded".repeat(32),
        };
        let expected =
            serde_json::to_string_pretty(&JsonContract::FileAnalysis.envelope(&payload)).unwrap();

        let actual =
            to_json_contract_pretty_bounded(JsonContract::FileAnalysis, &payload, expected.len())
                .unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn rejects_output_one_byte_over_the_limit() {
        let payload = Payload {
            value: "bounded".repeat(32),
        };
        let expected =
            serde_json::to_string_pretty(&JsonContract::FileAnalysis.envelope(&payload)).unwrap();

        let error = to_json_contract_pretty_bounded(
            JsonContract::FileAnalysis,
            &payload,
            expected.len() - 1,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            BoundedJsonError::LimitExceeded { max_bytes }
                if max_bytes == expected.len() - 1
        ));
    }
}
