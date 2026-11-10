//! A server's zone's extras: the disc's art placed in it as actors of its
//! own (go-eqoa-server's `internal/worlds/extras.go`, of which this is the
//! other copy: a change there is one here, or no world with extras builds).
//!
//! Each extra is a ZoneActor (0x6000 v2): a sprite or model by DictID, where
//! it stands, its heading, pitch and roll, its scale and its static light,
//! filed after the own actors of the room whose box holds it (so the zone's
//! baked lighting keeps every slot). The world draws and collides it as the
//! zone's own (`vi::world`'s actors, `scene`'s), and the server bakes its
//! collision into its walls the same way.
//!
//! Art the zone does not register itself is copied in off the player's disc
//! from where the server says it is (a zone of a world file, or any other
//! file, a CSF inflated): the sprite's record whole and every record it names
//! by DictID the zone lacks (a palette, its materials' surfaces, a group's
//! members, an LOD's levels, an HSprite's attachments, an effect volume's
//! surface and sprite, a particle's texture), each after the records it
//! names, at the end of the zone's resources (0x3100), each with a
//! ResourceTable entry in DictID order. Nothing of the zone moves: every
//! absolute offset its tables hold (ResourceTable 0x9000, ZoneStaticTable
//! 0x32C0) is mapped to where its record is now. The refusals (a sub-sprite,
//! art on its zone's pre-translations, a character body, art another zone
//! holds as other bytes) are the server's: a world it built has none, and the
//! file's CRC says whether this build is its.

use std::collections::{HashMap, HashSet};

/// A record in the making: type, version, children, its own bytes (a leaf's
/// body; a parent's past its children, none in retail), where it was in the
/// source (-1 a new one) and where it is once laid out, and its body's size.
pub(super) struct Node {
    ty: u16,
    version: i16,
    kids: Vec<Node>,
    tail: Vec<u8>,
    old: i64,
    at: i64,
    size: i64,
}

impl Node {
    fn new(ty: u16, version: i16, kids: Vec<Node>, tail: Vec<u8>) -> Node {
        Node { ty, version, kids, tail, old: -1, at: 0, size: 0 }
    }

    fn child(&self, ty: u16) -> Option<&Node> {
        self.kids.iter().find(|k| k.ty == ty)
    }

    fn child_mut(&mut self, ty: u16) -> Option<&mut Node> {
        self.kids.iter_mut().find(|k| k.ty == ty)
    }

    /// Its body's size as parsed (children and tail).
    fn body_size(&self) -> i64 {
        self.tail.len() as i64 + self.kids.iter().map(|k| 12 + k.body_size()).sum::<i64>()
    }

    /// Its body's first word as the game reads it: its first child's header
    /// when it has children, else its own bytes.
    fn first_word(&self) -> u32 {
        match self.kids.first() {
            Some(k) => u32::from(k.ty) | u32::from(k.version as u16) << 16,
            None => word(&self.tail, 0),
        }
    }

    fn clone_new(&self) -> Node {
        Node::new(self.ty, self.version, self.kids.iter().map(Node::clone_new).collect(), self.tail.clone())
    }
}

fn word(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map_or(0, |w| u32::from_le_bytes(w.try_into().unwrap()))
}

fn word64(b: &[u8], o: usize) -> i64 {
    b.get(o..o + 8).map_or(0, |w| i64::from_le_bytes(w.try_into().unwrap()))
}

/// The records of `b` from `at` to `end` (a forest).
pub(super) fn parse_nodes(b: &[u8], mut at: i64, end: i64) -> Result<Vec<Node>, String> {
    let mut out = Vec::new();
    while at < end {
        let (n, next) = parse_node(b, at, end)?;
        out.push(n);
        at = next;
    }
    Ok(out)
}

fn parse_node(b: &[u8], at: i64, end: i64) -> Result<(Node, i64), String> {
    if at + 12 > end {
        return Err(format!("a record header at {at} runs past {end}"));
    }
    let a = at as usize;
    let ty = u16::from_le_bytes([b[a], b[a + 1]]);
    let version = i16::from_le_bytes([b[a + 2], b[a + 3]]);
    let size = i64::from(word(b, a + 4) as i32);
    let count = i64::from(word(b, a + 8) as i32);
    let body = at + 12;
    if size < 0 || body + size > end || count < 0 {
        return Err(format!("record {ty:#x} at {at}: size {size}, {count} records"));
    }
    let mut kids = Vec::new();
    let mut p = body;
    for _ in 0..count {
        let (k, next) = parse_node(b, p, body + size)?;
        kids.push(k);
        p = next;
    }
    let tail = b[p as usize..(body + size) as usize].to_vec();
    Ok((Node { ty, version, kids, tail, old: at, at: 0, size: 0 }, body + size))
}

/// Lays the records out from `at`, returning the end.
fn layout(n: &mut Node, at: i64) -> i64 {
    n.at = at;
    let mut p = at + 12;
    for k in &mut n.kids {
        p = layout(k, p);
    }
    p += n.tail.len() as i64;
    n.size = p - at - 12;
    p
}

fn write(out: &mut Vec<u8>, n: &Node) {
    out.extend(n.ty.to_le_bytes());
    out.extend(n.version.to_le_bytes());
    out.extend((n.size as u32).to_le_bytes());
    out.extend((n.kids.len() as u32).to_le_bytes());
    for k in &n.kids {
        write(out, k);
    }
    out.extend(&n.tail);
}

// Which records register a DictID, as the game reads them (vanilla's esf
// crate, the decomp's m_dict->Add sites): the id is the first word of the
// record's own body, or of its first child (a header), which registers the
// parent. The first registration wins.
const ID_IN_BODY: &[u16] = &[0x1000, 0x2D00, 0x2B00, 0x2E00, 0x2600, 0x5000, 0x1230];
const ID_IN_BODY_V2: &[u16] = &[0x1200, 0x1210];
const ID_IN_HEADER: &[u16] = &[
    0x2000, 0x2200, 0x2700, 0x2A00, 0x2A40, 0x2C00, 0x2F00, 0x2310, 0x2320, 0xB100, 0xC100, 0xC300, 0x1110, 0x7000, 0xB000, 0xB030, 0xC200, 0xC000,
];

fn in_body(n: &Node) -> bool {
    ID_IN_BODY.contains(&n.ty) || ID_IN_BODY_V2.contains(&n.ty) && n.version >= 2
}

/// A record's path from a root: the indices of the children walked.
type Path = Vec<usize>;

/// Every DictID registered under `n`, in file order, with the path (from
/// `n`) of the record it names.
fn registers(n: &Node, path: &mut Path, reg: &mut dyn FnMut(u32, &Path)) {
    if in_body(n) && n.body_size() >= 4 {
        let id = n.first_word();
        if id != 0 {
            reg(id, path);
        }
    }
    for (i, k) in n.kids.iter().enumerate() {
        if i == 0 && ID_IN_HEADER.contains(&n.ty) && !in_body(k) && k.body_size() >= 4 {
            let id = k.first_word();
            if id != 0 {
                reg(id, path);
            }
        }
        path.push(i);
        registers(k, path, reg);
        path.pop();
    }
}

fn at_path<'a>(n: &'a Node, path: &[usize]) -> &'a Node {
    path.iter().fold(n, |n, &i| &n.kids[i])
}

/// The DictIDs the records under `n` name (in file order, each once).
fn refs(n: &Node) -> Vec<u32> {
    let mut out = Vec::new();
    fn walk(n: &Node, out: &mut Vec<u32>) {
        let mut add = |id: u32| {
            if id != 0 && id != 0xFFFF_FFFF && !out.contains(&id) {
                out.push(id);
            }
        };
        let b = &n.tail;
        let count = || word(b, 0) as i32;
        match n.ty {
            0x1100 => {
                let mut p = 4 + if n.version > 1 { 4 } else { 0 } + if n.version > 2 { 4 } else { 0 };
                let mut i = 0;
                while i < count() && i < 64 && p + 68 <= b.len() {
                    add(word(b, p + 4));
                    p += 68;
                    i += 1;
                }
            }
            0x2311 | 0x2321 => add(word(b, 4)),
            0x2C30 => {
                let mut i = 0;
                while i < count() && 4 + 32 * i as usize + 4 <= b.len() {
                    add(word(b, 4 + 32 * i as usize));
                    i += 1;
                }
            }
            0x2A30 | 0x2A70 => {
                let mut i = 0;
                while i < count() && 4 + 8 * i as usize + 4 <= b.len() {
                    add(word(b, 4 + 8 * i as usize));
                    i += 1;
                }
            }
            0x2500 => {
                let mut i = 0;
                while i < count() && 8 + 12 * i as usize + 4 <= b.len() {
                    add(word(b, 8 + 12 * i as usize));
                    i += 1;
                }
            }
            0xC330 if b.len() >= 32 => {
                add(word(b, 24));
                add(word(b, 28));
            }
            0xC020 => add(word(b, 0)),
            _ => {}
        }
        for k in &n.kids {
            walk(k, out);
        }
    }
    walk(n, &mut out);
    out
}

/// The records an extra's art is taken from (a zone of a world file, or a
/// whole file), and what they register: DictID -> (root, path).
pub(super) struct Art {
    roots: Vec<Node>,
    dict: HashMap<u32, (usize, Path)>,
}

impl Art {
    pub(super) fn new(roots: Vec<Node>) -> Art {
        let mut dict = HashMap::new();
        for (r, root) in roots.iter().enumerate() {
            registers(root, &mut Vec::new(), &mut |id, path| {
                dict.entry(id).or_insert_with(|| (r, path.clone()));
            });
        }
        Art { roots, dict }
    }

    /// A whole file's records: from its tree's start to its end (or a zero
    /// type at the top), a CSF inflated first.
    pub(super) fn whole(file: Vec<u8>) -> Result<Art, String> {
        let b = if esf::is_csf(&file) { esf::decompress_csf(&file).map_err(|e| e.to_string())? } else { file };
        if b.len() < 24 || &b[..4] != b"FJBO" {
            return Err("not an OBJF file".into());
        }
        let start = word64(&b, 16);
        if start < 0 || start > b.len() as i64 {
            return Err(format!("its records at {start}"));
        }
        let (mut at, end) = (start, b.len() as i64);
        let mut roots = Vec::new();
        while at + 12 <= end && u16::from_le_bytes([b[at as usize], b[at as usize + 1]]) != 0 {
            let (n, next) = parse_node(&b, at, end)?;
            roots.push(n);
            at = next;
        }
        Ok(Art::new(roots))
    }

    fn get(&self, id: u32) -> Option<&Node> {
        self.dict.get(&id).map(|(r, p)| at_path(&self.roots[*r], p))
    }
}

/// A zone room's box (none before version 2).
#[derive(Clone, Copy)]
struct Room {
    b: [f32; 6],
    has: bool,
}

/// The room an extra at `p` is filed under: the first whose box holds it;
/// else the first whose box holds it seen from above; else the nearest from
/// above (the first as near).
fn pick_room(rooms: &[Room], p: [f32; 3]) -> Option<usize> {
    let inside = |r: &Room, y: bool| r.has && r.b[0] <= p[0] && p[0] <= r.b[3] && r.b[2] <= p[2] && p[2] <= r.b[5] && (!y || r.b[1] <= p[1] && p[1] <= r.b[4]);
    if let Some(i) = rooms.iter().position(|r| inside(r, true)) {
        return Some(i);
    }
    if let Some(i) = rooms.iter().position(|r| inside(r, false)) {
        return Some(i);
    }
    let mut best: Option<(usize, f64)> = None;
    for (i, r) in rooms.iter().enumerate().filter(|(_, r)| r.has) {
        let dx = f64::from((r.b[0] - p[0]).max(0.0).max(p[0] - r.b[3]));
        let dz = f64::from((r.b[2] - p[2]).max(0.0).max(p[2] - r.b[5]));
        let d = dx * dx + dz * dz;
        if best.is_none_or(|(_, bd)| d < bd) {
            best = Some((i, d));
        }
    }
    best.map(|b| b.0)
}

fn actor_node(e: &net::serverworlds::Extra) -> Node {
    let mut b = Vec::with_capacity(36);
    b.extend(e.sprite.to_le_bytes());
    for v in e.at.iter().chain(&e.hpr).chain(std::iter::once(&e.scale)) {
        b.extend(v.to_bits().to_le_bytes());
    }
    b.extend(e.color);
    Node::new(0x6000, 2, Vec::new(), b)
}

/// Refuses art placed by its zone's pre-translations (a PrimBuffer of kind 4,
/// a CollBuffer packed with vertex groups).
fn stands_alone(n: &Node) -> Result<(), String> {
    match n.ty {
        0x1200 if n.version >= 1 && word(&n.tail, if n.version > 1 { 4 } else { 0 }) as i32 == 4 => {
            return Err("its geometry stands on its zone's pre-translations".into());
        }
        0x4200 if n.version >= 2 && matches!(word(&n.tail, 0) as i32, 2 | 3) => {
            return Err("its collision stands on its zone's pre-translations".into());
        }
        _ => {}
    }
    n.kids.iter().try_for_each(stands_alone)
}

/// A zone record written with its extras: the record, where its ZoneBase
/// record is in it and that record's length.
pub(super) struct WithExtras {
    pub rec: Vec<u8>,
    pub base_at: i64,
    pub base_len: i64,
}

/// Each record that was in the zone's source record, by where it was there:
/// where it is now and its length (header and body).
fn placed(n: &Node, out: &mut HashMap<i64, (i64, i64)>) {
    if n.old >= 0 {
        out.insert(n.old, (n.at, 12 + n.size));
    }
    for k in &n.kids {
        placed(k, out);
    }
}

/// The zone record `rec` (at `src_at` of its source file) as written at
/// `out_at` of the new file with its extras; `arts` are the extras' art
/// sources by place (the extra's file in capitals, and its zone).
pub(super) fn with_extras(rec: &[u8], src_at: i64, out_at: i64, extras: &[net::serverworlds::Extra], arts: &HashMap<(String, u16), Art>) -> Result<WithExtras, String> {
    let mut roots = parse_nodes(rec, 0, rec.len() as i64)?;
    if roots.len() != 1 || roots[0].ty != 0x3000 {
        return Err("not a zone record".into());
    }
    let mut zone = roots.pop().unwrap();
    let kid = |ty: u16| zone.kids.iter().position(|k| k.ty == ty);
    let (Some(res_i), Some(acts_i), Some(base_i)) = (kid(0x3100), kid(0x3290), kid(0x3200)) else {
        return Err("a zone without resources, actors or a base".into());
    };
    let base = &zone.kids[base_i];
    let (Some(rt), Some(st), Some(rooms)) = (base.child(0x9000), base.child(0x32C0), base.child(0x3230)) else {
        return Err("a zone base without its resource table, static table or rooms".into());
    };
    let old_rt = rt.tail.clone();
    let mut st = st.tail.clone();
    let rooms: Vec<Room> = rooms
        .kids
        .iter()
        .filter(|r| r.ty == 0x3240)
        .map(|r| {
            let mut x = Room { b: [0.0; 6], has: false };
            if r.version >= 2 && r.tail.len() >= 36 {
                for (k, v) in x.b.iter_mut().enumerate() {
                    *v = f32::from_bits(word(&r.tail, 12 + 4 * k));
                }
                x.has = true;
            }
            x
        })
        .collect();

    // the zone's own registrations, then the copies': whether an id is had
    let mut had: HashSet<u32> = HashSet::new();
    registers(&zone, &mut Vec::new(), &mut |id, _| {
        had.insert(id);
    });

    // the art copied in, each record after those it names
    fn need(a: &Art, id: u32, had: &mut HashSet<u32>, copies: &mut Vec<(Node, u32)>) -> Result<(), String> {
        if had.contains(&id) {
            return Ok(());
        }
        let r = a.get(id).ok_or_else(|| format!("{id:#010x} is not in its art"))?;
        if matches!(r.ty, 0x2310 | 0x2320 | 0x2700) {
            return Err(format!("{id:#010x} is a record of type {:#x}, which extras do not copy", r.ty));
        }
        stands_alone(r).map_err(|e| format!("{id:#010x}: {e}"))?;
        registers(r, &mut Vec::new(), &mut |d, _| {
            had.insert(d);
        });
        for rf in refs(r) {
            need(a, rf, had, copies)?;
        }
        copies.push((r.clone_new(), id));
        Ok(())
    }
    let mut copies: Vec<(Node, u32)> = Vec::new();
    for (i, e) in extras.iter().enumerate() {
        if had.contains(&e.sprite) {
            continue;
        }
        if e.from.is_empty() {
            return Err(format!("extra {i}: the zone does not hold {:#010x}, and the extra names no art", e.sprite));
        }
        let a = arts.get(&(e.from.to_uppercase(), e.zone)).ok_or_else(|| format!("extra {i}: {} zone {} not read", e.from, e.zone))?;
        need(a, e.sprite, &mut had, &mut copies).map_err(|err| format!("extra {i} ({} zone {}): {err}", e.from, e.zone))?;
    }
    let res_len = zone.kids[res_i].kids.len();
    let copy_ids: Vec<u32> = copies.iter().map(|c| c.1).collect();
    zone.kids[res_i].kids.extend(copies.into_iter().map(|c| c.0));

    // the actors, each in its room's list after the room's own; a room with
    // none given a list of its own, in room order
    let mut added: Vec<i32> = Vec::new();
    {
        let acts = &mut zone.kids[acts_i];
        let room_of = |l: &Node| -> Option<i32> { if l.ty == 0x3270 { l.child(0x3280).filter(|r| r.tail.len() >= 4).map(|r| word(&r.tail, 0) as i32) } else { None } };
        for (i, e) in extras.iter().enumerate() {
            let ri = pick_room(&rooms, e.at).ok_or_else(|| format!("extra {i}: the zone has no room to file it under"))? as i32;
            let at = match acts.kids.iter().position(|l| room_of(l) == Some(ri) && l.child(0x6040).is_some()) {
                Some(k) => k,
                None => {
                    let list = Node::new(0x3270, 0, vec![Node::new(0x3280, 0, Vec::new(), (ri as u32).to_le_bytes().to_vec()), Node::new(0x6040, 0, Vec::new(), Vec::new())], Vec::new());
                    let k = acts.kids.iter().position(|l| room_of(l).is_some_and(|r| r > ri)).unwrap_or(acts.kids.len());
                    acts.kids.insert(k, list);
                    added.push(ri);
                    k
                }
            };
            acts.kids[at].child_mut(0x6040).unwrap().kids.push(actor_node(e));
        }
    }

    // the resource table: its entries (by where their records were) and the
    // copies' (by their place among the resources), in DictID order
    let n = word(&old_rt, 0) as i32;
    if n < 0 || 4 + 16 * n as usize > old_rt.len() {
        return Err(format!("resource table: {n} entries in {} bytes", old_rt.len()));
    }
    enum Of {
        Old(i64),
        Copy(usize),
    }
    let mut entries: Vec<(Of, u32)> = Vec::with_capacity(n as usize + copy_ids.len());
    for i in 0..n as usize {
        entries.push((Of::Old(word64(&old_rt, 4 + 16 * i) - src_at), word(&old_rt, 4 + 16 * i + 12)));
    }
    entries.extend(copy_ids.iter().enumerate().map(|(k, &id)| (Of::Copy(res_len + k), id)));
    entries.sort_by_key(|e| e.1);
    zone.kids[base_i].child_mut(0x9000).unwrap().tail = vec![0u8; 4 + 16 * entries.len()];

    // lay out, then fill in the tables with where their records are
    let end = layout(&mut zone, 0);
    let mut was = HashMap::new();
    placed(&zone, &mut was);
    let mut rt = vec![0u8; 4 + 16 * entries.len()];
    rt[0..4].copy_from_slice(&(entries.len() as u32).to_le_bytes());
    for (i, (of, id)) in entries.iter().enumerate() {
        let (at, len) = match *of {
            Of::Old(v) => *was.get(&v).ok_or_else(|| format!("resource table entry names {}, no record of the zone", v + src_at))?,
            Of::Copy(k) => {
                let c = &zone.kids[res_i].kids[k];
                (c.at, 12 + c.size)
            }
        };
        let o = 4 + 16 * i;
        rt[o..o + 8].copy_from_slice(&(out_at + at).to_le_bytes());
        rt[o + 8..o + 12].copy_from_slice(&(len as u32).to_le_bytes());
        rt[o + 12..o + 16].copy_from_slice(&id.to_le_bytes());
    }
    let m = word(&st, 0) as i32;
    if m < 0 || 4 + 24 * m as usize != st.len() {
        return Err(format!("static table: {m} rooms in {} bytes", st.len()));
    }
    for i in 0..m as usize {
        for o in [4 + 24 * i, 4 + 24 * i + 12] {
            let v = word64(&st, o);
            if v < 0 {
                continue;
            }
            let (at, len) = *was.get(&(v - src_at)).ok_or_else(|| format!("static table room {i} names {v}, no record of the zone"))?;
            st[o..o + 8].copy_from_slice(&(out_at + at).to_le_bytes());
            st[o + 8..o + 12].copy_from_slice(&(len as u32).to_le_bytes());
        }
        if added.contains(&(i as i32)) {
            let l = zone.kids[acts_i].kids.iter().find(|l| l.old < 0 && l.child(0x3280).is_some_and(|r| word(&r.tail, 0) as usize == i)).unwrap();
            let o = 4 + 24 * i;
            st[o..o + 8].copy_from_slice(&(out_at + l.at).to_le_bytes());
            st[o + 8..o + 12].copy_from_slice(&((12 + l.size) as u32).to_le_bytes());
        }
    }
    {
        let base = &mut zone.kids[base_i];
        base.child_mut(0x9000).unwrap().tail = rt;
        base.child_mut(0x32C0).unwrap().tail = st;
    }
    let mut out = Vec::with_capacity(end as usize);
    write(&mut out, &zone);
    if out.len() as i64 != end {
        return Err(format!("laid out {end} bytes, wrote {}", out.len()));
    }
    let base = &zone.kids[base_i];
    Ok(WithExtras { rec: out, base_at: base.at, base_len: 12 + base.size })
}
