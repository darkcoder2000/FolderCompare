//! Loading and saving text files so that encoding, byte order mark and line endings round-trip unchanged.

use std::io;
use std::path::Path;

use encoding_rs::{EncoderResult, UTF_16BE, UTF_16LE, WINDOWS_1252};

const BINARY_PROBE_LENGTH: usize = 8000;

/// Line ending used when a text has none (and for new documents).
pub const DEFAULT_NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextEncoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Windows-1252, the fallback for text that is not valid UTF-8.
    Windows1252,
}

/// A text file as loaded for comparison, with what is needed to write it back unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedText {
    pub text: String,
    pub encoding: TextEncoding,
    pub has_bom: bool,
    pub newline: &'static str,
    pub is_binary: bool,
}

impl Default for LoadedText {
    fn default() -> Self {
        Self::empty()
    }
}

impl LoadedText {
    pub fn empty() -> Self {
        Self {
            text: String::new(),
            encoding: TextEncoding::Utf8,
            has_bom: false,
            newline: DEFAULT_NEWLINE,
            is_binary: false,
        }
    }

    /// An empty document in the format of another one (a side that does not exist yet).
    pub fn empty_like(other: &LoadedText) -> Self {
        Self {
            text: String::new(),
            is_binary: false,
            ..other.clone()
        }
    }

    pub fn encoding_name(&self) -> &'static str {
        match self.encoding {
            TextEncoding::Utf8 if self.has_bom => "UTF-8 BOM",
            TextEncoding::Utf8 => "UTF-8",
            TextEncoding::Utf16Le => "UTF-16 LE",
            TextEncoding::Utf16Be => "UTF-16 BE",
            TextEncoding::Windows1252 => "ANSI",
        }
    }

    pub fn newline_name(&self) -> &'static str {
        newline_name(self.newline)
    }
}

pub fn newline_name(newline: &str) -> &'static str {
    match newline {
        "\r\n" => "CRLF",
        "\n" => "LF",
        _ => "CR",
    }
}

pub fn load(path: &Path) -> io::Result<LoadedText> {
    Ok(decode(&std::fs::read(path)?))
}

pub fn decode(bytes: &[u8]) -> LoadedText {
    let (encoding, bom) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (TextEncoding::Utf8, 3)
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        (TextEncoding::Utf16Le, 2)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (TextEncoding::Utf16Be, 2)
    } else {
        if bytes[..bytes.len().min(BINARY_PROBE_LENGTH)].contains(&0) {
            return LoadedText {
                is_binary: true,
                ..LoadedText::empty()
            };
        }
        if std::str::from_utf8(bytes).is_ok() {
            (TextEncoding::Utf8, 0)
        } else {
            (TextEncoding::Windows1252, 0)
        }
    };

    let body = &bytes[bom..];
    let text = match encoding {
        TextEncoding::Utf8 => String::from_utf8_lossy(body).into_owned(),
        TextEncoding::Utf16Le => UTF_16LE.decode_without_bom_handling(body).0.into_owned(),
        TextEncoding::Utf16Be => UTF_16BE.decode_without_bom_handling(body).0.into_owned(),
        TextEncoding::Windows1252 => WINDOWS_1252
            .decode_without_bom_handling(body)
            .0
            .into_owned(),
    };
    let newline = detect_newline(&text);
    LoadedText {
        text,
        encoding,
        has_bom: bom > 0,
        newline,
        is_binary: false,
    }
}

/// The bytes of `text` in the given format, with the byte order mark when the original had one.
pub fn encode(text: &str, format: &LoadedText) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 3);
    if format.has_bom {
        out.extend_from_slice(match format.encoding {
            TextEncoding::Utf8 => &[0xEF, 0xBB, 0xBF],
            TextEncoding::Utf16Le => &[0xFF, 0xFE],
            TextEncoding::Utf16Be => &[0xFE, 0xFF],
            TextEncoding::Windows1252 => &[],
        });
    }
    match format.encoding {
        TextEncoding::Utf8 => out.extend_from_slice(text.as_bytes()),
        TextEncoding::Utf16Le => out.extend(text.encode_utf16().flat_map(u16::to_le_bytes)),
        TextEncoding::Utf16Be => out.extend(text.encode_utf16().flat_map(u16::to_be_bytes)),
        TextEncoding::Windows1252 => encode_1252(text, &mut out),
    }
    out
}

/// Windows-1252; chars it can't represent become `?`.
fn encode_1252(text: &str, out: &mut Vec<u8>) {
    let mut encoder = WINDOWS_1252.new_encoder();
    let mut rest = text;
    let mut buffer = vec![0u8; text.len() + 16];
    loop {
        let (result, read, written) =
            encoder.encode_from_utf8_without_replacement(rest, &mut buffer, true);
        out.extend_from_slice(&buffer[..written]);
        rest = &rest[read..];
        match result {
            EncoderResult::InputEmpty => break,
            EncoderResult::Unmappable(_) => out.push(b'?'),
            EncoderResult::OutputFull => buffer.resize(buffer.len() * 2, 0),
        }
    }
}

pub fn save(path: &Path, text: &str, format: &LoadedText) -> io::Result<()> {
    std::fs::write(path, encode(text, format))
}

/// The most frequent line ending; [`DEFAULT_NEWLINE`] when the text has none.
pub fn detect_newline(text: &str) -> &'static str {
    let (mut crlf, mut lf, mut cr) = (0, 0, 0);
    let bytes = text.as_bytes();
    let mut n = 0;
    while n < bytes.len() {
        match bytes[n] {
            b'\r' if bytes.get(n + 1) == Some(&b'\n') => {
                crlf += 1;
                n += 1;
            }
            b'\r' => cr += 1,
            b'\n' => lf += 1,
            _ => {}
        }
        n += 1;
    }
    if crlf + lf + cr == 0 {
        DEFAULT_NEWLINE
    } else if lf > crlf && lf >= cr {
        "\n"
    } else if cr > crlf && cr > lf {
        "\r"
    } else {
        "\r\n"
    }
}
