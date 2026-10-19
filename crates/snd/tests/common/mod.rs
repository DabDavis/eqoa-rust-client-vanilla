//! The harness both oracle tests share: eqoa-port's `build/oracle_snd`, the
//! disc, and CDSTREAM's files as the oracle numbers them.

use snd::Disc;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct Oracle {
    _child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
}

impl Oracle {
    pub fn open() -> Option<Oracle> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../eqoa-port/build/oracle_snd");
        if !path.exists() {
            eprintln!("SKIP: no oracle at {} (make oracle in eqoa-port)", path.display());
            return None;
        }
        let mut child = Command::new(path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().ok()?;
        let stdin = child.stdin.take()?;
        let stdout = child.stdout.take()?;
        Some(Oracle { _child: child, stdin, stdout })
    }

    pub fn call(&mut self, op: u32, payload: &[u8]) -> Vec<u8> {
        let mut head = op.to_le_bytes().to_vec();
        head.extend((payload.len() as u32).to_le_bytes());
        self.stdin.write_all(&head).unwrap();
        self.stdin.write_all(payload).unwrap();
        self.stdin.flush().unwrap();
        let mut n = [0u8; 4];
        self.stdout.read_exact(&mut n).unwrap();
        let mut r = vec![0u8; u32::from_le_bytes(n) as usize];
        self.stdout.read_exact(&mut r).unwrap();
        r
    }
}

pub fn disc() -> Option<iso::Iso> {
    let path = std::env::var("EQOA_ISO").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso"));
    match iso::Iso::open(&path) {
        Ok(d) => Some(d),
        Err(_) => {
            eprintln!("SKIP: no disc at {}", path.display());
            None
        }
    }
}

/// The files the oracle's CDSTREAM serves, numbered as it numbers them.
pub struct Files(pub Vec<(String, Vec<u8>)>);

impl Disc for Files {
    fn search(&mut self, name: &str) -> Option<(u32, u32)> {
        let i = self.0.iter().position(|f| f.0 == name)?;
        Some(((i as u32 + 1) << 20, self.0[i].1.len() as u32))
    }
    fn read(&mut self, lsn: u32, sectors: u32, buf: &mut [u8]) -> bool {
        let i = (lsn >> 20) as usize - 1;
        let off = (lsn & 0xfffff) as usize * 0x800;
        let n = sectors as usize * 0x800;
        buf[..n].fill(0);
        let data = &self.0[i].1;
        if off < data.len() {
            let k = (data.len() - off).min(n);
            buf[..k].copy_from_slice(&data[off..off + k]);
        }
        true
    }
}

pub fn pcm(b: &[u8]) -> (Vec<i16>, &[u8]) {
    let n = u32::from_le_bytes(b[..4].try_into().unwrap()) as usize;
    let s = b[4..4 + n * 4].chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
    (s, &b[4 + n * 4..])
}
