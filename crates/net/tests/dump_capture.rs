//! `net::dump` against the console's own session: pcaps/ps2_sandstorm.pcap's
//! server packets are replayed into the Rust DRDP (which joins the
//! fragments), and every MemoryDump Sandstorm sent must read with no short
//! read (a PS2 entered the world on each, so `Msg000D` read it whole). Every
//! retail dump ends in 39 zero bytes past the last field `Msg000D` reads
//! (`ReadState`); the reader must stop exactly there, which, with the
//! crafting slots landing on retail's fixed markers just before the Combat
//! Mastery block, pins the alignment of everything before. Negative
//! control: the dump cut into its last field must read short. SKIPs without
//! the capture.

use drdp::buffer::Buffer;
use drdp::{Addr, Drdp, PORT_LEADER, address_to_endpoint};
use net::stream::Reader;
use vi::fpu::Ieee;

type Datagram = (f64, [u8; 4], u16, [u8; 4], u16, Vec<u8>);

fn read_pcap(path: &std::path::Path) -> Option<Vec<Datagram>> {
    let d = std::fs::read(path).ok()?;
    let mut p = 24;
    let mut out = Vec::new();
    while p + 16 <= d.len() {
        let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let (ts, tu, cap) = (u(p), u(p + 4), u(p + 8) as usize);
        let b = &d[p + 16..p + 16 + cap];
        p += 16 + cap;
        if b.len() < 42 || b[12..14] != [8, 0] || b[23] != 17 {
            continue;
        }
        let ip = &b[14..];
        let udp = &ip[((ip[0] & 15) * 4) as usize..];
        let len = u16::from_be_bytes([udp[4], udp[5]]) as usize;
        out.push((
            ts as f64 + tu as f64 / 1e6,
            ip[12..16].try_into().unwrap(),
            u16::from_be_bytes([udp[0], udp[1]]),
            ip[16..20].try_into().unwrap(),
            u16::from_be_bytes([udp[2], udp[3]]),
            udp[8..len.min(udp.len())].to_vec(),
        ));
    }
    Some(out)
}

/// Every guaranteed message the server delivered, in order.
fn messages(cap: &[Datagram]) -> Vec<Vec<u8>> {
    let first = cap.iter().find(|d| d.4 == 10070).unwrap();
    let server = (first.3, first.4);
    // The PS2's instance: its first segment's, which is also its clock.
    let mut b = Buffer::reader(&first.5[..first.5.len() - 4]);
    b.u16();
    b.u16();
    let instance = drdp::read_segment_header(&mut b, 0, 0).crc;
    let mut d = Drdp::<Ieee>::create_client(Addr { ip: first.1, port: first.2 }, instance);
    let sys = d.system_endpoint;
    d.connection_open(sys, address_to_endpoint(server.0, server.1, PORT_LEADER)).unwrap();
    let t0 = cap[0].0;
    let mut out = Vec::new();
    for p in cap {
        d.set_time(instance.wrapping_add(((p.0 - t0) * 1000.0) as u32));
        if p.2 == 10070 {
            let mut pkt = p.1.to_vec();
            pkt.extend(p.2.to_le_bytes());
            pkt.extend(&p.5);
            d.process_packet(&pkt);
            while let Some(m) = d.receive() {
                if m.ty == drdp::TYPE_GUARANTEED {
                    out.push(m.data);
                }
            }
        }
        d.flush();
        d.outbox.clear();
    }
    out
}

/// The zero run every retail dump ends with.
const TRAILER: usize = 39;

/// Reads without a short read, stopping where the zero trailer starts.
fn reads_whole(body: &[u8]) -> (bool, net::dump::MemoryDump) {
    let mut r = Reader::new(body);
    let d = net::dump::read(&mut r);
    (!r.error && r.pos + TRAILER == body.len() && body[r.pos..].iter().all(|&b| b == 0), d)
}

/// The weapon-set markers every retail dump carries in the crafting slots.
const SLOTS: [[u32; 4]; 6] = [[99, 1, 0, 98], [99, 1, 0, 98], [99, 1, 0, 98], [99, 1, 0, 98], [99, 99, 99, 98], [99, 99, 99, 98]];

#[test]
fn sandstorms_memory_dumps_read_whole() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../pcaps/ps2_sandstorm.pcap");
    let Some(cap) = read_pcap(&path) else {
        eprintln!("SKIP: no capture at {}", path.display());
        return;
    };
    let msgs = messages(&cap);
    let dumps: Vec<&[u8]> = msgs.iter().filter(|m| m.len() > 2 && m[0..2] == [0x0D, 0x00]).map(|m| &m[2..]).collect();
    assert!(!dumps.is_empty(), "no MemoryDump among {} messages", msgs.len());
    for body in &dumps {
        let (whole, d) = reads_whole(body);
        eprintln!(
            "{} bytes: {} as {} (level {}), {} quick-chat sets, {} quests, {} items ({} worn), bank {} items + {} lots + {} bids, corpse {}, {} abilities, speed {}, CM stats {:?}",
            body.len(),
            d.world_path,
            d.player.name,
            d.player.level,
            d.text_sets.len(),
            d.quests.len(),
            d.inventory.items.len(),
            d.inventory.items.iter().filter(|i| i.equip_slot >= 0).count(),
            d.bank.items.items.len(),
            d.bank.lots.len(),
            d.bank.bids.len(),
            d.corpse.is_some(),
            d.abilities.len(),
            d.speed,
            &d.mastery.stats[..8]
        );
        assert!(whole, "a {}-byte MemoryDump did not read to its trailer", body.len());
        assert_eq!(d.craft_slots.map(|s| s.0), SLOTS);
        assert!(d.inventory.items.iter().any(|i| !i.item.name.is_empty()), "no item has a name");
        // Negative control: cut into the last field read.
        let mut r = Reader::new(&body[..body.len() - TRAILER - 1]);
        net::dump::read(&mut r);
        assert!(r.error, "a dump cut into its last field read whole");
    }
}

