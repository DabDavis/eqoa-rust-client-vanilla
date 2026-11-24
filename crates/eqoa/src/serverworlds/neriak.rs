use std::io::{Cursor, Read};

/// Apply the same local-disc reconstruction as the server. The plan comes
/// with the client and contains only offsets, counts and authored doorway data.
pub(super) fn cut(rec: &mut [u8]) -> Result<(), String> {
    if super::crc32(rec) != 0xfec7_de9c {
        return Err("Neriak arena requires the Frontiers Tunaria zone 45".into());
    }
    let source = rec.to_vec();
    let mut code = Vec::new();
    flate2::read::GzDecoder::new(&include_bytes!("neriak-room.cut.gz")[..])
        .read_to_end(&mut code).map_err(|e| e.to_string())?;
    let mut reader = Cursor::new(code.as_slice());
    fn word(reader: &mut Cursor<&[u8]>) -> Result<usize, String> {
        let mut v = [0; 4];
        reader.read_exact(&mut v).map_err(|e| e.to_string())?;
        Ok(u32::from_le_bytes(v) as usize)
    }
    while reader.position() < code.len() as u64 {
        let mut op = [0];
        reader.read_exact(&mut op).map_err(|e| e.to_string())?;
        let dest = word(&mut reader)?;
        let n = word(&mut reader)?;
        let out = rec.get_mut(dest..dest.checked_add(n).ok_or("arena offset overflow")?).ok_or("invalid arena output range")?;
        match op[0] {
            0 => out.fill(0),
            1 => {
                let src = word(&mut reader)?;
                out.copy_from_slice(source.get(src..src.checked_add(n).ok_or("arena offset overflow")?).ok_or("invalid arena source range")?);
            }
            2 => reader.read_exact(out).map_err(|e| e.to_string())?,
            _ => return Err("unknown arena cut instruction".into()),
        }
    }
    Ok(())
}
