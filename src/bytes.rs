//! Bounds-checked little-endian readers.
//!
//! Every accessor returns `Option` rather than panicking. A malformed or
//! truncated DLL in the bin folder must degrade to "skip this file", never to
//! a crash that takes the build down with it.

pub fn u8_at(b: &[u8], o: usize) -> Option<u8> {
    b.get(o).copied()
}

pub fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    let s = b.get(o..o.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

pub fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    let s = b.get(o..o.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    let s = b.get(o..o.checked_add(8)?)?;
    let mut a = [0u8; 8];
    a.copy_from_slice(s);
    Some(u64::from_le_bytes(a))
}

/// Read an unsigned integer of `width` bytes (2 or 4) as u32.
pub fn uint_at(b: &[u8], o: usize, width: usize) -> Option<u32> {
    match width {
        2 => u16_at(b, o).map(u32::from),
        4 => u32_at(b, o),
        _ => None,
    }
}

/// NUL-terminated UTF-8 string starting at `o`, as used by the `#Strings` heap.
pub fn cstr_at(b: &[u8], o: usize) -> Option<&str> {
    let rest = b.get(o..)?;
    let end = rest.iter().position(|&c| c == 0)?;
    std::str::from_utf8(&rest[..end]).ok()
}
