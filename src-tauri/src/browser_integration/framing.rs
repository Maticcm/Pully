//! Blocking length-prefixed JSON framing, used by the native-messaging host
//! binary (`pully-native-host`) on both sides of its work: reading/writing
//! Chromium's native-messaging protocol on stdin/stdout, and forwarding the
//! same envelope shape to Pully over the local named pipe (opened as a plain
//! blocking file handle — Windows named pipe clients are just files).
//!
//! Chromium's framing: a 4-byte native-endian (little-endian on every
//! platform Pully targets) unsigned length, followed by that many bytes of
//! UTF-8 JSON. Chrome itself caps a single outgoing message at 1 MiB; Pully
//! enforces the same cap on both read and write so a compromised or buggy
//! peer on either end can't force an unbounded allocation.

use super::messages::limits::MAX_FRAME_BYTES;
use std::io::{self, Read, Write};

pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_bytes = [0u8; 4];
    if let Err(error) = reader.read_exact(&mut len_bytes) {
        return if error.kind() == io::ErrorKind::UnexpectedEof {
            Ok(None)
        } else {
            Err(error)
        };
    }
    let len = u32::from_ne_bytes(len_bytes) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds the maximum allowed size",
        ));
    }
    let mut buffer = vec![0u8; len];
    reader.read_exact(&mut buffer)?;
    Ok(Some(buffer))
}

pub fn write_frame<W: Write>(writer: &mut W, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame exceeds the maximum allowed size",
        ));
    }
    writer.write_all(&(payload.len() as u32).to_ne_bytes())?;
    writer.write_all(payload)?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trips_a_frame() {
        let mut buffer = Vec::new();
        write_frame(&mut buffer, b"{\"hello\":true}").unwrap();
        let mut cursor = Cursor::new(buffer);
        let frame = read_frame(&mut cursor).unwrap().unwrap();
        assert_eq!(frame, b"{\"hello\":true}");
    }

    #[test]
    fn returns_none_at_clean_eof() {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        assert_eq!(read_frame(&mut cursor).unwrap(), None);
    }

    #[test]
    fn rejects_an_oversized_declared_length() {
        let mut cursor = Cursor::new(((MAX_FRAME_BYTES as u32 + 1).to_ne_bytes()).to_vec());
        assert!(read_frame(&mut cursor).is_err());
    }

    #[test]
    fn refuses_to_write_an_oversized_payload() {
        let mut buffer = Vec::new();
        let oversized = vec![0u8; MAX_FRAME_BYTES + 1];
        assert!(write_frame(&mut buffer, &oversized).is_err());
    }
}
