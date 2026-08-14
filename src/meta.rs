//! ECMA-335 CLI metadata reader, scoped to one question:
//! "which custom attributes are applied to the assembly itself?"
//!
//! This is the part that has no ready-made Rust crate. We parse the metadata
//! root, locate the `#~` table stream and `#Strings` heap, compute the physical
//! row layout of the tables preceding `CustomAttribute` (0x0C) so we can find
//! it, then walk its rows looking for ones whose parent is the `Assembly` row.
//!
//! References are ECMA-335 6th edition, partition II.

use crate::bytes::{cstr_at, u16_at, u32_at, u64_at, u8_at, uint_at};

// Table identifiers we reference by name.
const T_MODULE: usize = 0x00;
const T_TYPEREF: usize = 0x01;
const T_TYPEDEF: usize = 0x02;
const T_FIELD: usize = 0x04;
const T_METHODDEF: usize = 0x06;
const T_PARAM: usize = 0x08;
const T_MEMBERREF: usize = 0x0A;
const T_CUSTOMATTR: usize = 0x0C;
const T_ASSEMBLY: usize = 0x20;

/// A coded index: a tag in the low bits selecting one of several tables,
/// with the 1-based row number in the high bits. (II.24.2.6)
#[derive(Clone, Copy)]
struct Coded {
    /// Table id per tag value; -1 marks a reserved/unused tag slot.
    tables: &'static [i16],
}

impl Coded {
    /// Bits needed to hold the largest tag.
    fn bits(&self) -> u32 {
        let n = self.tables.len();
        if n <= 1 {
            0
        } else {
            u32::BITS - ((n - 1) as u32).leading_zeros()
        }
    }

    /// Coded indexes narrow to 2 bytes when every referenced table is small
    /// enough to leave room for the tag.
    fn width(&self, rows: &[u32; 64]) -> usize {
        let limit = (0xFFFFu32) >> self.bits();
        let max = self
            .tables
            .iter()
            .filter(|&&t| t >= 0)
            .map(|&t| rows[t as usize])
            .max()
            .unwrap_or(0);
        if max > limit {
            4
        } else {
            2
        }
    }

    fn decode(&self, value: u32) -> Option<(usize, u32)> {
        let bits = self.bits();
        let mask = (1u32 << bits) - 1;
        let tag = (value & mask) as usize;
        let row = value >> bits;
        let table = *self.tables.get(tag)?;
        if table < 0 {
            return None;
        }
        Some((table as usize, row))
    }
}

const TYPE_DEF_OR_REF: Coded = Coded { tables: &[0x02, 0x01, 0x1B] };
const HAS_CONSTANT: Coded = Coded { tables: &[0x04, 0x08, 0x17] };
const RESOLUTION_SCOPE: Coded = Coded { tables: &[0x00, 0x1A, 0x23, 0x01] };
const MEMBER_REF_PARENT: Coded = Coded { tables: &[0x02, 0x01, 0x1A, 0x06, 0x1B] };
const CUSTOM_ATTR_TYPE: Coded = Coded { tables: &[-1, -1, 0x06, 0x0A, -1] };
const HAS_CUSTOM_ATTR: Coded = Coded {
    tables: &[
        0x06, 0x04, 0x01, 0x02, 0x08, 0x09, 0x0A, 0x00, 0x0E, 0x17, 0x14, 0x11, 0x1A, 0x1B, 0x20,
        0x23, 0x26, 0x27, 0x28, 0x2A, 0x2C, 0x2B,
    ],
};

#[derive(Clone, Copy)]
enum Col {
    Fixed(usize),
    Str,
    Guid,
    Blob,
    Simple(usize),
    Coded(Coded),
}

/// Physical column layout for tables 0x00..=0x0C. We need these — and only
/// these — because their combined size is what tells us where the
/// `CustomAttribute` table starts.
fn schema(table: usize) -> Option<&'static [Col]> {
    use Col::*;
    Some(match table {
        0x00 => &[Fixed(2), Str, Guid, Guid, Guid],
        0x01 => &[Coded(RESOLUTION_SCOPE), Str, Str],
        0x02 => &[Fixed(4), Str, Str, Coded(TYPE_DEF_OR_REF), Simple(T_FIELD), Simple(T_METHODDEF)],
        0x03 => &[Simple(T_FIELD)],
        0x04 => &[Fixed(2), Str, Blob],
        0x05 => &[Simple(T_METHODDEF)],
        0x06 => &[Fixed(4), Fixed(2), Fixed(2), Str, Blob, Simple(T_PARAM)],
        0x07 => &[Simple(T_PARAM)],
        0x08 => &[Fixed(2), Fixed(2), Str],
        0x09 => &[Simple(T_TYPEDEF), Coded(TYPE_DEF_OR_REF)],
        0x0A => &[Coded(MEMBER_REF_PARENT), Str, Blob],
        0x0B => &[Fixed(1), Fixed(1), Coded(HAS_CONSTANT), Blob],
        0x0C => &[Coded(HAS_CUSTOM_ATTR), Coded(CUSTOM_ATTR_TYPE), Blob],
        _ => return None,
    })
}

pub struct Meta<'a> {
    tables: &'a [u8],
    strings: &'a [u8],
    rows: [u32; 64],
    /// Byte offset of each table's first row, within `tables`.
    table_off: [usize; 64],
    row_size: [usize; 64],
    str_w: usize,
    guid_w: usize,
    blob_w: usize,
}

impl<'a> Meta<'a> {
    /// Parse the metadata root produced by `pe::read_metadata_region`.
    pub fn parse(md: &'a [u8]) -> Option<Meta<'a>> {
        if u32_at(md, 0)? != 0x424A_5342 {
            return None; // "BSJB"
        }
        let ver_len = u32_at(md, 12)? as usize;
        // Version string is padded to a 4-byte boundary.
        let after_ver = 16 + ((ver_len + 3) & !3);
        let stream_count = u16_at(md, after_ver + 2)? as usize;

        let mut cursor = after_ver + 4;
        let mut tables_stream: Option<&[u8]> = None;
        let mut strings: &[u8] = &[];

        for _ in 0..stream_count {
            let off = u32_at(md, cursor)? as usize;
            let size = u32_at(md, cursor + 4)? as usize;
            let name = cstr_at(md, cursor + 8)?;
            // Name is NUL-terminated then padded to a 4-byte boundary.
            cursor += 8 + ((name.len() + 1 + 3) & !3);

            let body = md.get(off..off.checked_add(size)?)?;
            match name {
                // "#-" is the uncompressed/edit-and-continue variant; same layout.
                "#~" | "#-" => tables_stream = Some(body),
                "#Strings" => strings = body,
                _ => {}
            }
        }

        let ts = tables_stream?;
        let heap_sizes = u8_at(ts, 6)?;
        let str_w = if heap_sizes & 0x01 != 0 { 4 } else { 2 };
        let guid_w = if heap_sizes & 0x02 != 0 { 4 } else { 2 };
        let blob_w = if heap_sizes & 0x04 != 0 { 4 } else { 2 };

        let valid = u64_at(ts, 8)?;
        let mut rows = [0u32; 64];
        let mut p = 24;
        // One row count per set bit in `valid`, in ascending table order.
        for (t, row) in rows.iter_mut().enumerate() {
            if valid & (1u64 << t) != 0 {
                *row = u32_at(ts, p)?;
                p += 4;
            }
        }

        let mut meta = Meta {
            tables: ts,
            strings,
            rows,
            table_off: [0; 64],
            row_size: [0; 64],
            str_w,
            guid_w,
            blob_w,
        };

        // Walk tables in id order accumulating sizes until we have located
        // CustomAttribute. Anything past it is irrelevant to us.
        //
        // The loop counter is a metadata table id used to index several
        // parallel arrays and to test a bit in `valid`, so iterating one of
        // those arrays instead would not express what this is doing.
        let mut off = p;
        #[allow(clippy::needless_range_loop)]
        for t in 0..=T_CUSTOMATTR {
            if valid & (1u64 << t) == 0 {
                continue;
            }
            let size = meta.compute_row_size(schema(t)?);
            meta.row_size[t] = size;
            meta.table_off[t] = off;
            off = off.checked_add(size.checked_mul(rows[t] as usize)?)?;
        }

        Some(meta)
    }

    fn col_width(&self, c: &Col) -> usize {
        match c {
            Col::Fixed(n) => *n,
            Col::Str => self.str_w,
            Col::Guid => self.guid_w,
            Col::Blob => self.blob_w,
            Col::Simple(t) => {
                if self.rows[*t] > 0xFFFF {
                    4
                } else {
                    2
                }
            }
            Col::Coded(c) => c.width(&self.rows),
        }
    }

    fn compute_row_size(&self, cols: &[Col]) -> usize {
        cols.iter().map(|c| self.col_width(c)).sum()
    }

    /// Read column `col` of 1-based `row` in `table`.
    fn cell(&self, table: usize, row: u32, col: usize) -> Option<u32> {
        if row == 0 || row > self.rows[table] {
            return None;
        }
        let cols = schema(table)?;
        let mut off = self.table_off[table] + (row as usize - 1) * self.row_size[table];
        for c in cols.iter().take(col) {
            off += self.col_width(c);
        }
        let w = self.col_width(cols.get(col)?);
        uint_at(self.tables, off, w)
    }

    fn string(&self, idx: u32) -> Option<&'a str> {
        cstr_at(self.strings, idx as usize)
    }

    /// True when this image carries an assembly manifest (as opposed to being
    /// a bare netmodule). Mirrors `MetadataReader.IsAssembly`.
    pub fn is_assembly(&self) -> bool {
        self.rows[T_ASSEMBLY] > 0
    }

    /// Resolve the declaring type of a MethodDef by locating the TypeDef whose
    /// MethodList range covers it.
    fn declaring_type(&self, method_row: u32) -> Option<u32> {
        let n = self.rows[T_TYPEDEF];
        for i in 1..=n {
            let start = self.cell(T_TYPEDEF, i, 5)?;
            let end = if i == n {
                self.rows[T_METHODDEF] + 1
            } else {
                self.cell(T_TYPEDEF, i + 1, 5)?
            };
            if method_row >= start && method_row < end {
                return Some(i);
            }
        }
        None
    }

    /// Namespace and name of the type an attribute's constructor belongs to.
    fn attribute_type_name(&self, ctor: u32) -> Option<(&'a str, &'a str)> {
        let (table, row) = CUSTOM_ATTR_TYPE.decode(ctor)?;
        match table {
            T_MEMBERREF => {
                let parent = self.cell(T_MEMBERREF, row, 0)?;
                let (ptable, prow) = MEMBER_REF_PARENT.decode(parent)?;
                match ptable {
                    T_TYPEREF => {
                        let ns = self.string(self.cell(T_TYPEREF, prow, 2)?)?;
                        let name = self.string(self.cell(T_TYPEREF, prow, 1)?)?;
                        Some((ns, name))
                    }
                    T_TYPEDEF => {
                        let ns = self.string(self.cell(T_TYPEDEF, prow, 2)?)?;
                        let name = self.string(self.cell(T_TYPEDEF, prow, 1)?)?;
                        Some((ns, name))
                    }
                    _ => None,
                }
            }
            T_METHODDEF => {
                let td = self.declaring_type(row)?;
                let ns = self.string(self.cell(T_TYPEDEF, td, 2)?)?;
                let name = self.string(self.cell(T_TYPEDEF, td, 1)?)?;
                Some((ns, name))
            }
            _ => None,
        }
    }

    /// Invoke `f(namespace, name)` for each attribute applied to the assembly
    /// itself, stopping early if `f` returns true. Returns whether it did.
    pub fn for_each_assembly_attribute<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&str, &str) -> bool,
    {
        if !self.is_assembly() {
            return false;
        }
        for row in 1..=self.rows[T_CUSTOMATTR] {
            let Some(parent) = self.cell(T_CUSTOMATTR, row, 0) else {
                continue;
            };
            // Only attributes whose parent is the single Assembly row (1).
            match HAS_CUSTOM_ATTR.decode(parent) {
                Some((T_ASSEMBLY, 1)) => {}
                _ => continue,
            }
            let Some(ctor) = self.cell(T_CUSTOMATTR, row, 1) else {
                continue;
            };
            if let Some((ns, name)) = self.attribute_type_name(ctor) {
                if f(ns, name) {
                    return true;
                }
            }
        }
        false
    }
}

// Silence unused-constant warnings for ids kept for documentation value.
const _: () = {
    let _ = T_MODULE;
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Tag width is ceil(log2(n)) over the number of tables a coded index can
    /// point at. Getting this wrong shifts every row number in the table and
    /// silently yields garbage rather than an error.
    #[test]
    fn coded_index_tag_widths_match_ecma335() {
        assert_eq!(TYPE_DEF_OR_REF.bits(), 2); // 3 tables
        assert_eq!(HAS_CONSTANT.bits(), 2); // 3 tables
        assert_eq!(RESOLUTION_SCOPE.bits(), 2); // 4 tables
        assert_eq!(MEMBER_REF_PARENT.bits(), 3); // 5 tables
        assert_eq!(CUSTOM_ATTR_TYPE.bits(), 3); // 5 slots, 2 of them reserved
        assert_eq!(HAS_CUSTOM_ATTR.bits(), 5); // 22 tables
    }

    /// A coded index is 2 bytes while every table it can reference still fits
    /// in the bits left over after the tag, and widens to 4 beyond that.
    #[test]
    fn coded_index_widens_past_the_tag_limit() {
        let mut rows = [0u32; 64];
        assert_eq!(HAS_CUSTOM_ATTR.width(&rows), 2);

        // 5 tag bits leave 11 bits of row space: 0xFFFF >> 5 == 2047.
        rows[T_METHODDEF] = 2047;
        assert_eq!(HAS_CUSTOM_ATTR.width(&rows), 2);
        rows[T_METHODDEF] = 2048;
        assert_eq!(HAS_CUSTOM_ATTR.width(&rows), 4);
    }

    /// The exact decode the scanner relies on: an attribute whose parent is the
    /// assembly itself. Assembly is tag 14 of HasCustomAttribute, and there is
    /// only ever row 1.
    #[test]
    fn decodes_an_assembly_scoped_custom_attribute_parent() {
        let encoded = (1 << 5) | 14;
        assert_eq!(HAS_CUSTOM_ATTR.decode(encoded), Some((T_ASSEMBLY, 1)));
    }

    #[test]
    fn decodes_custom_attribute_constructor_kinds() {
        // Tag 2 => MethodDef, tag 3 => MemberRef, row 1 in both cases.
        assert_eq!(CUSTOM_ATTR_TYPE.decode((1 << 3) | 2), Some((T_METHODDEF, 1)));
        assert_eq!(CUSTOM_ATTR_TYPE.decode((1 << 3) | 3), Some((T_MEMBERREF, 1)));
    }

    /// Tags 0, 1 and 4 of CustomAttributeType are reserved. Treating a reserved
    /// tag as a real table would index into the wrong metadata table.
    #[test]
    fn reserved_coded_index_tags_are_rejected() {
        assert_eq!(CUSTOM_ATTR_TYPE.decode(1 << 3), None);
        assert_eq!(CUSTOM_ATTR_TYPE.decode((1 << 3) | 1), None);
        assert_eq!(CUSTOM_ATTR_TYPE.decode((1 << 3) | 4), None);
    }

    #[test]
    fn garbage_input_is_rejected_rather_than_panicking() {
        assert!(Meta::parse(&[]).is_none());
        assert!(Meta::parse(b"not metadata").is_none());
        assert!(Meta::parse(&[0u8; 512]).is_none());
    }
}
