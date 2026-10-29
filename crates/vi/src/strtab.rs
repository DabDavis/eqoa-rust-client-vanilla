//! `VIStringTable`: WORDS.TXT and its kind. The encoding is picked by the
//! byte-order mark (UTF-8, UTF-16 either way round, else 8-bit text widened
//! a byte to a character); a TAB starts a column, a newline a row, a CR is
//! dropped; the widest row sets the column count. Rows count from 1, as
//! `GetString` takes them, and the column is the language.

#[derive(Clone, Debug, Default)]
pub struct StringTable {
    pub rows: usize,
    pub cols: usize,
    cells: Vec<Option<String>>,
}

impl StringTable {
    /// `ProcessRawFile` + `FixupTextBuffer`. None where the game fails (a
    /// UTF-8 file that does not convert cleanly).
    pub fn parse(buf: &[u8]) -> Option<StringTable> {
        let text: Vec<u16> = if let Some(p) = buf.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
            std::str::from_utf8(p).ok()?.encode_utf16().collect()
        } else if let Some(p) = buf.strip_prefix(&[0xFF, 0xFE]) {
            // n = size / 2 - 1 characters after the mark.
            p.as_chunks::<2>().0.iter().take(buf.len() / 2 - 1).map(|c| u16::from_le_bytes(*c)).collect()
        } else if let Some(p) = buf.strip_prefix(&[0xFE, 0xFF]) {
            p.as_chunks::<2>().0.iter().take(buf.len() / 2 - 1).map(|c| u16::from_be_bytes(*c)).collect()
        } else {
            buf.iter().map(|&b| b as u16).collect()
        };
        // The text ends at its first NUL (`strlen16`).
        let text = &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())];
        let mut rows = 1;
        let mut cols = 1;
        let mut col = 1;
        for &c in text {
            if c == '\t' as u16 {
                col += 1;
            } else if c == '\n' as u16 {
                rows += 1;
                cols = cols.max(col);
                col = 1;
            }
        }
        let mut cells: Vec<Option<String>> = vec![None; rows * cols];
        let (mut row, mut cell) = (0, 0);
        let mut cur: Vec<u16> = Vec::new();
        let put = |cells: &mut Vec<Option<String>>, row: usize, cell: usize, cur: &mut Vec<u16>| {
            // Only the last row, which no newline ends, can be wider than
            // the count; the game writes those cells past its array, and
            // here they are dropped.
            if let Some(slot) = cells.get_mut(row * cols + cell) {
                *slot = Some(String::from_utf16_lossy(cur));
            }
            cur.clear();
        };
        for &c in text {
            match c {
                0x09 => {
                    put(&mut cells, row, cell, &mut cur);
                    cell += 1;
                }
                0x0D => {}
                0x0A => {
                    put(&mut cells, row, cell, &mut cur);
                    row += 1;
                    cell = 0;
                }
                _ => cur.push(c),
            }
        }
        put(&mut cells, row, cell, &mut cur);
        Some(StringTable { rows, cols, cells })
    }

    /// `GetString(row, col)`: "(null)" for a cell out of range or missing.
    pub fn get(&self, row: i32, col: i32) -> &str {
        let r = row - 1;
        if r >= 0
            && (r as usize) < self.rows
            && col >= 0
            && (col as usize) < self.cols
            && let Some(s) = &self.cells[r as usize * self.cols + col as usize]
        {
            return s;
        }
        "(null)"
    }

    /// Row `row` (from 1) of column `col` set to `s`, the table grown to the
    /// row if it ends before it (the rows between left missing): words the
    /// client puts in itself (`eqoa`'s `livewords`). A column the table has
    /// not is not made.
    pub fn set(&mut self, row: i32, col: i32, s: &str) {
        if row < 1 || col < 0 || col as usize >= self.cols {
            return;
        }
        let r = row as usize - 1;
        if r >= self.rows {
            self.rows = r + 1;
            self.cells.resize(self.rows * self.cols, None);
        }
        self.cells[r * self.cols + col as usize] = Some(s.to_string());
    }

    /// Every cell's `from` made `to` (the client's own spelling fixes, not
    /// the game's: `eqoa`'s `spelling`).
    pub fn replace(&mut self, from: &str, to: &str) {
        for s in self.cells.iter_mut().flatten() {
            if s.contains(from) {
                *s = s.replace(from, to);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_count_from_one_and_columns_are_languages() {
        let t = StringTable::parse(b"\xEF\xBB\xBFEnglish\tFrench\r\nOn\tOui\r\nOff").unwrap();
        assert_eq!((t.rows, t.cols), (3, 2));
        assert_eq!(t.get(1, 0), "English");
        assert_eq!(t.get(2, 1), "Oui");
        assert_eq!(t.get(3, 0), "Off");
        // Out of range, and a missing cell, are "(null)".
        assert_eq!(t.get(0, 0), "(null)");
        assert_eq!(t.get(4, 0), "(null)");
        assert_eq!(t.get(3, 1), "(null)");
        assert_eq!(t.get(1, 2), "(null)");
    }

    /// `set` grows the table to the row it sets, the rows between missing;
    /// a row it has is replaced in place; no column is made.
    #[test]
    fn set_grows_the_table_to_its_row() {
        let mut t = StringTable::parse(b"A\tB\nC").unwrap();
        t.set(5, 1, "E");
        assert_eq!((t.rows, t.cols), (5, 2));
        assert_eq!((t.get(1, 0), t.get(1, 1), t.get(2, 0)), ("A", "B", "C"), "the rows it had stay");
        assert_eq!((t.get(3, 0), t.get(4, 1), t.get(5, 0)), ("(null)", "(null)", "(null)"));
        assert_eq!(t.get(5, 1), "E");
        t.set(2, 0, "D");
        assert_eq!((t.rows, t.get(2, 0)), (5, "D"));
        t.set(6, 2, "x");
        t.set(0, 0, "x");
        assert_eq!((t.rows, t.cols), (5, 2), "no column made, no row 0");
    }

    #[test]
    fn the_byte_order_mark_picks_the_encoding() {
        let le = [0xFF, 0xFE, b'A', 0, b'\t', 0, b'B', 0, b'\n', 0];
        let be = [0xFE, 0xFF, 0, b'A', 0, b'\t', 0, b'B', 0, b'\n'];
        for b in [&le[..], &be[..]] {
            let t = StringTable::parse(b).unwrap();
            assert_eq!((t.get(1, 0), t.get(1, 1)), ("A", "B"));
        }
        // Only a finished row counts its columns: an unterminated one
        // reads its second cell as missing, as the game does.
        assert_eq!(StringTable::parse(b"A\tB").unwrap().get(1, 1), "(null)");
        // No mark: bytes widened, so 0xE9 is 'é', not UTF-8.
        assert_eq!(StringTable::parse(b"caf\xE9").unwrap().get(1, 0), "caf\u{e9}");
        // A UTF-8 file that does not convert fails, as the game's does.
        assert!(StringTable::parse(b"\xEF\xBB\xBF\xFF").is_none());
    }
}
