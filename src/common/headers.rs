use std::{
    iter::{Enumerate, Peekable},
    slice::Iter,
};

pub(crate) struct HeaderIterator<'x> {
    message: &'x [u8],
    iter: Peekable<Enumerate<Iter<'x, u8>>>,
    start_pos: usize,
}

pub(crate) struct HeaderParser<'x> {
    message: &'x [u8],
    iter: Peekable<Enumerate<Iter<'x, u8>>>,
    start_pos: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthenticatedHeader<'x> {
    Ds(&'x [u8]),
    Aar(&'x [u8]),
    Ams(&'x [u8]),
    As(&'x [u8]),
    From(&'x [u8]),
    Other(&'x [u8]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header<'x, T> {
    pub(crate) name: &'x [u8],
    pub(crate) value: &'x [u8],
    pub(crate) header: T,
}

impl<'x> HeaderParser<'x> {
    pub fn new(message: &'x [u8]) -> Self {
        HeaderParser {
            message,
            iter: message.iter().enumerate().peekable(),
            start_pos: 0,
        }
    }

    pub fn body_offset(&mut self) -> Option<usize> {
        self.iter.peek().map(|(pos, _)| *pos)
    }
}

impl<'x> HeaderIterator<'x> {
    pub fn new(message: &'x [u8]) -> Self {
        HeaderIterator {
            message,
            iter: message.iter().enumerate().peekable(),
            start_pos: 0,
        }
    }

    pub fn body_offset(&mut self) -> Option<usize> {
        self.iter.peek().map(|(pos, _)| *pos)
    }
}

impl<'x> Iterator for HeaderIterator<'x> {
    type Item = (&'x [u8], &'x [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let mut colon_pos = usize::MAX;
        let mut last_ch = 0;

        while let Some((pos, &ch)) = self.iter.next() {
            if colon_pos == usize::MAX {
                match ch {
                    b':' => {
                        colon_pos = pos;
                    }
                    b'\n' => {
                        if last_ch == b'\r' || self.start_pos == pos {
                            // End of headers
                            return None;
                        } else if self
                            .iter
                            .peek()
                            .map_or(true, |(_, next_byte)| ![b' ', b'\t'].contains(next_byte))
                        {
                            // Invalid header, return anyway.
                            let header_name = self
                                .message
                                .get(self.start_pos..pos + 1)
                                .unwrap_or_default();
                            self.start_pos = pos + 1;
                            return Some((header_name, b""));
                        }
                    }
                    _ => (),
                }
            } else if ch == b'\n'
                && self
                    .iter
                    .peek()
                    .map_or(true, |(_, next_byte)| ![b' ', b'\t'].contains(next_byte))
            {
                let header_name = self
                    .message
                    .get(self.start_pos..colon_pos)
                    .unwrap_or_default();
                let header_value = self.message.get(colon_pos + 1..pos + 1).unwrap_or_default();

                self.start_pos = pos + 1;

                return Some((header_name, header_value));
            }

            last_ch = ch;
        }

        None
    }
}

impl<'x> Iterator for HeaderParser<'x> {
    type Item = (AuthenticatedHeader<'x>, &'x [u8]);

    fn next(&mut self) -> Option<Self::Item> {
        let mut colon_pos = usize::MAX;
        let mut last_ch = 0;

        let mut token_start = usize::MAX;
        let mut token_end = usize::MAX;

        let mut hash: u64 = 0;
        let mut hash_shift = 0;

        while let Some((pos, &ch)) = self.iter.next() {
            if colon_pos == usize::MAX {
                match ch {
                    b':' => {
                        colon_pos = pos;
                    }
                    b'\n' => {
                        if last_ch == b'\r' || self.start_pos == pos {
                            // End of headers
                            return None;
                        } else if self
                            .iter
                            .peek()
                            .map_or(true, |(_, next_byte)| ![b' ', b'\t'].contains(next_byte))
                        {
                            // Invalid header, return anyway.
                            let header_name = self
                                .message
                                .get(self.start_pos..pos + 1)
                                .unwrap_or_default();
                            self.start_pos = pos + 1;
                            return Some((AuthenticatedHeader::Other(header_name), b""));
                        }
                    }
                    b' ' | b'\t' | b'\r' => (),
                    b'A'..=b'Z' => {
                        if hash_shift < 64 {
                            hash |= ((ch - b'A' + b'a') as u64) << hash_shift;
                            hash_shift += 8;

                            if token_start == usize::MAX {
                                token_start = pos;
                            }
                        }
                        token_end = pos;
                    }
                    b'a'..=b'z' | b'-' => {
                        if hash_shift < 64 {
                            hash |= (ch as u64) << hash_shift;
                            hash_shift += 8;

                            if token_start == usize::MAX {
                                token_start = pos;
                            }
                        }
                        token_end = pos;
                    }
                    _ => {
                        hash = u64::MAX;
                    }
                }
            } else if ch == b'\n'
                && self
                    .iter
                    .peek()
                    .map_or(true, |(_, next_byte)| ![b' ', b'\t'].contains(next_byte))
            {
                let header_name = self
                    .message
                    .get(self.start_pos..colon_pos)
                    .unwrap_or_default();
                let header_value = self.message.get(colon_pos + 1..pos + 1).unwrap_or_default();
                let header_name = match hash {
                    FROM => AuthenticatedHeader::From(header_name),
                    AS => AuthenticatedHeader::As(header_name),
                    AAR if self
                        .message
                        .get(token_start + 8..token_end + 1)
                        .unwrap_or_default()
                        .eq_ignore_ascii_case(b"entication-Results") =>
                    {
                        AuthenticatedHeader::Aar(header_name)
                    }
                    AMS if self
                        .message
                        .get(token_start + 8..token_end + 1)
                        .unwrap_or_default()
                        .eq_ignore_ascii_case(b"age-Signature") =>
                    {
                        AuthenticatedHeader::Ams(header_name)
                    }
                    DKIM if self
                        .message
                        .get(token_start + 8..token_end + 1)
                        .unwrap_or_default()
                        .eq_ignore_ascii_case(b"nature") =>
                    {
                        AuthenticatedHeader::Ds(header_name)
                    }
                    _ => AuthenticatedHeader::Other(header_name),
                };

                self.start_pos = pos + 1;

                return Some((header_name, header_value));
            }

            last_ch = ch;
        }

        None
    }
}

const FROM: u64 = (b'f' as u64) | (b'r' as u64) << 8 | (b'o' as u64) << 16 | (b'm' as u64) << 24;
const DKIM: u64 = (b'd' as u64)
    | (b'k' as u64) << 8
    | (b'i' as u64) << 16
    | (b'm' as u64) << 24
    | (b'-' as u64) << 32
    | (b's' as u64) << 40
    | (b'i' as u64) << 48
    | (b'g' as u64) << 56;
const AAR: u64 = (b'a' as u64)
    | (b'r' as u64) << 8
    | (b'c' as u64) << 16
    | (b'-' as u64) << 24
    | (b'a' as u64) << 32
    | (b'u' as u64) << 40
    | (b't' as u64) << 48
    | (b'h' as u64) << 56;
const AMS: u64 = (b'a' as u64)
    | (b'r' as u64) << 8
    | (b'c' as u64) << 16
    | (b'-' as u64) << 24
    | (b'm' as u64) << 32
    | (b'e' as u64) << 40
    | (b's' as u64) << 48
    | (b's' as u64) << 56;
const AS: u64 = (b'a' as u64)
    | (b'r' as u64) << 8
    | (b'c' as u64) << 16
    | (b'-' as u64) << 24
    | (b's' as u64) << 32
    | (b'e' as u64) << 40
    | (b'a' as u64) << 48
    | (b'l' as u64) << 56;

#[cfg(test)]
mod test {
    use crate::common::headers::{AuthenticatedHeader, HeaderParser};

    use super::HeaderIterator;

    #[test]
    fn header_iterator() {
        for (message, headers) in [
            (
                "From: a\nTo: b\nEmpty:\nMulti: 1\n 2\nSubject: c\n\nNot-header: ignore\n",
                vec![
                    ("From", " a\n"),
                    ("To", " b\n"),
                    ("Empty", "\n"),
                    ("Multi", " 1\n 2\n"),
                    ("Subject", " c\n"),
                ],
            ),
            (
                ": a\nTo: b\n \n \nc\n:\nFrom : d\nSubject: e\n\nNot-header: ignore\n",
                vec![
                    ("", " a\n"),
                    ("To", " b\n \n \n"),
                    ("c\n", ""),
                    ("", "\n"),
                    ("From ", " d\n"),
                    ("Subject", " e\n"),
                ],
            ),
            (
                concat!(
                    "A: X\r\n",
                    "B : Y\t\r\n",
                    "\tZ  \r\n",
                    "\r\n",
                    " C \r\n",
                    "D \t E\r\n"
                ),
                vec![("A", " X\r\n"), ("B ", " Y\t\r\n\tZ  \r\n")],
            ),
        ] {
            assert_eq!(
                HeaderIterator::new(message.as_bytes())
                    .map(|(h, v)| {
                        (
                            std::str::from_utf8(h).unwrap(),
                            std::str::from_utf8(v).unwrap(),
                        )
                    })
                    .collect::<Vec<_>>(),
                headers
            );

            assert_eq!(
                HeaderParser::new(message.as_bytes())
                    .map(|(h, v)| {
                        (
                            std::str::from_utf8(match h {
                                AuthenticatedHeader::Ds(v)
                                | AuthenticatedHeader::Aar(v)
                                | AuthenticatedHeader::Ams(v)
                                | AuthenticatedHeader::As(v)
                                | AuthenticatedHeader::From(v)
                                | AuthenticatedHeader::Other(v) => v,
                            })
                            .unwrap(),
                            std::str::from_utf8(v).unwrap(),
                        )
                    })
                    .collect::<Vec<_>>(),
                headers
            );
        }
    }

    #[test]
    fn header_parser() {
        let message = concat!(
            "ARC-Message-Signature: i=1; a=rsa-sha256;\n",
            "ARC-Authentication-Results: i=1;\n",
            "ARC-Seal: i=1; a=rsa-sha256;\n",
            "DKIM-Signature: v=1; a=rsa-sha256; c=relaxed/simple;\n",
            "From: jdoe@domain\n",
            "F r o m : jane@domain.com\n",
            "ARC-Authentication: i=1;\n",
            "\nhey",
        );
        assert_eq!(
            HeaderParser::new(message.as_bytes())
                .map(|(h, _)| { h })
                .collect::<Vec<_>>(),
            vec![
                AuthenticatedHeader::Ams(b"ARC-Message-Signature"),
                AuthenticatedHeader::Aar(b"ARC-Authentication-Results"),
                AuthenticatedHeader::As(b"ARC-Seal"),
                AuthenticatedHeader::Ds(b"DKIM-Signature"),
                AuthenticatedHeader::From(b"From"),
                AuthenticatedHeader::From(b"F r o m "),
                AuthenticatedHeader::Other(b"ARC-Authentication"),
            ]
        );
    }
}