//! Minimal PE/COFF reader: just enough to locate the CLI metadata region.
//!
//! Deliberately does *not* read whole files. A Sitefinity bin folder is
//! hundreds of megabytes, but the CLI metadata we care about is a small slice
//! near the end of each image. We read the headers, resolve the metadata
//! directory, then read only that slice.
//!
//! The three-way return type matters: callers must distinguish "definitely has
//! no managed metadata" from "I could not tell", because the caller's
//! fail-safe rule includes anything uncertain.

use crate::bytes::{u16_at, u32_at};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const DOS_LFANEW: usize = 0x3C;
const PE_SIG: u32 = 0x0000_4550; // "PE\0\0"
const MAGIC_PE32: u16 = 0x010B;
const MAGIC_PE32PLUS: u16 = 0x020B;
const DIR_CLR_HEADER: usize = 14;

pub enum Outcome {
    /// The CLI metadata region, read out of the image.
    Metadata(Vec<u8>),
    /// Definitively carries no CLI metadata: a native DLL, a resource-only
    /// DLL, or not a PE image at all. Cannot possibly hold a managed attribute.
    NotManaged,
    /// Truncated, malformed, or unreadable. We genuinely do not know.
    Unreadable,
}

/// A section header entry, used to map RVAs onto file offsets.
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_pointer: u32,
    raw_size: u32,
}

fn rva_to_offset(sections: &[Section], rva: u32) -> Option<u32> {
    for s in sections {
        // Use the larger of virtual/raw size: linkers pad either way, and being
        // permissive here avoids rejecting otherwise-valid images.
        let span = s.virtual_size.max(s.raw_size);
        if rva >= s.virtual_address && rva < s.virtual_address.saturating_add(span) {
            let delta = rva - s.virtual_address;
            if delta >= s.raw_size {
                return None; // lands in zero-fill, not backed by file bytes
            }
            return s.raw_pointer.checked_add(delta);
        }
    }
    None
}

fn read_at(f: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Read the CLI metadata region of a managed PE image.
pub fn read_metadata(path: &Path) -> Outcome {
    match read_inner(path) {
        Ok(o) => o,
        Err(()) => Outcome::Unreadable,
    }
}

fn read_inner(path: &Path) -> Result<Outcome, ()> {
    let mut f = File::open(path).map_err(|_| ())?;
    let file_len = f.metadata().map_err(|_| ())?.len();

    // A PE with no room for a DOS header is not a PE.
    if file_len < 0x40 {
        return Ok(Outcome::NotManaged);
    }

    let prefix = (file_len.min(0x400)) as usize;
    let head = read_at(&mut f, 0, prefix).ok_or(())?;

    if u16_at(&head, 0).ok_or(())? != 0x5A4D {
        return Ok(Outcome::NotManaged); // no "MZ"
    }
    let pe_off = u32_at(&head, DOS_LFANEW).ok_or(())? as usize;
    match u32_at(&head, pe_off) {
        Some(PE_SIG) => {}
        // Header sits past our prefix, or the signature is wrong. A real PE
        // always has this within the first bytes, so treat a mismatch as
        // "not a PE" and a truncated read as unreadable.
        Some(_) => return Ok(Outcome::NotManaged),
        None => return Err(()),
    }

    let coff = pe_off + 4;
    let num_sections = u16_at(&head, coff + 2).ok_or(())? as usize;
    let opt_size = u16_at(&head, coff + 16).ok_or(())? as usize;
    let opt_off = coff + 20;
    let sections_off = opt_off + opt_size;
    let headers_len = sections_off + num_sections * 40;

    if headers_len as u64 > file_len {
        return Err(());
    }

    // Re-read if the header block is larger than our initial prefix.
    let head = if headers_len > head.len() {
        read_at(&mut f, 0, headers_len).ok_or(())?
    } else {
        head
    };

    // Data directories sit at a magic-dependent offset in the optional header.
    let dir_base = match u16_at(&head, opt_off).ok_or(())? {
        MAGIC_PE32 => opt_off + 96,
        MAGIC_PE32PLUS => opt_off + 112,
        _ => return Err(()), // malformed optional header
    };

    let clr_rva = match u32_at(&head, dir_base + DIR_CLR_HEADER * 8) {
        Some(v) => v,
        // No CLR data directory at all: an ordinary native image.
        None => return Ok(Outcome::NotManaged),
    };
    if clr_rva == 0 {
        return Ok(Outcome::NotManaged);
    }

    let mut sections = Vec::with_capacity(num_sections);
    for i in 0..num_sections {
        let s = sections_off + i * 40;
        sections.push(Section {
            virtual_size: u32_at(&head, s + 8).ok_or(())?,
            virtual_address: u32_at(&head, s + 12).ok_or(())?,
            raw_size: u32_at(&head, s + 16).ok_or(())?,
            raw_pointer: u32_at(&head, s + 20).ok_or(())?,
        });
    }

    // COR20 header: metadata RVA/size live at offset 8.
    let clr_off = rva_to_offset(&sections, clr_rva).ok_or(())?;
    let cor20 = read_at(&mut f, clr_off as u64, 72).ok_or(())?;
    let md_rva = u32_at(&cor20, 8).ok_or(())?;
    let md_size = u32_at(&cor20, 12).ok_or(())? as usize;
    if md_rva == 0 || md_size == 0 {
        return Err(());
    }

    let md_off = rva_to_offset(&sections, md_rva).ok_or(())? as u64;
    if md_off + md_size as u64 > file_len {
        return Err(());
    }

    Ok(Outcome::Metadata(read_at(&mut f, md_off, md_size).ok_or(())?))
}
