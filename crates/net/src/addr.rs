//! A server's name to the address the game talks to. The game's protocol is
//! IPv4: DRDP's addresses (`Datagrams`, the server list's shards) are four
//! bytes and a port. So a name is taken by its IPv4 address when it has one,
//! as a PS2 asks for one; on a network with IPv6 alone (a phone's hotspot on a
//! mobile network: its resolver makes an IPv6 address out of each IPv4 one,
//! DNS64, and its gateway turns the traffic back into IPv4, NAT64) the name
//! has only the made one, and the session reaches every IPv4 address through
//! that gateway (`Nat64`). F3 20261029-191720: on a work hotspot the first
//! address of eqoa.live was an IPv6 one, and the IPv4 socket refused it ("An
//! address incompatible with the requested protocol was used", os error
//! 10047).

use std::net::{Ipv6Addr, SocketAddr, ToSocketAddrs};

/// The address to take among a name's: its first IPv4, else its first.
pub fn pick(addrs: impl IntoIterator<Item = SocketAddr>) -> Option<SocketAddr> {
    let mut first = None;
    for a in addrs {
        if a.is_ipv4() {
            return Some(a);
        }
        first.get_or_insert(a);
    }
    first
}

/// `host:port` resolved, by `pick`.
pub fn resolve(host: &str, port: u16) -> std::io::Result<SocketAddr> {
    pick((host, port).to_socket_addrs()?).ok_or_else(|| std::io::Error::other("no address"))
}

/// How a session reaches IPv4 addresses from an IPv6 address alone: the
/// first 96 bits of the address a name was made into (the gateway's prefix,
/// 64:ff9b::/96 or the network's own), each IPv4 address the last 32 bits
/// under it (RFC 6052's /96). An address mapped to IPv4 (::ffff:0:0/96) is
/// one already, and needs none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nat64 {
    prefix: [u8; 12],
}

impl Nat64 {
    /// The way through for a server at `a`, and its IPv4 address; None for
    /// an IPv4 address (no way needed).
    pub fn of(a: &SocketAddr) -> Option<(Nat64, [u8; 4])> {
        let SocketAddr::V6(v6) = a else { return None };
        let o = v6.ip().octets();
        let mut prefix = [0u8; 12];
        prefix.copy_from_slice(&o[..12]);
        Some((Nat64 { prefix }, [o[12], o[13], o[14], o[15]]))
    }

    /// The IPv6 address that reaches `ip` (for the socket).
    pub fn to_v6(&self, ip: [u8; 4], port: u16) -> SocketAddr {
        let mut o = [0u8; 16];
        o[..12].copy_from_slice(&self.prefix);
        o[12..].copy_from_slice(&ip);
        SocketAddr::from((Ipv6Addr::from(o), port))
    }

    /// The IPv4 address a datagram from `a` came from: the last 32 bits of
    /// one under the prefix, else none (not the gateway's).
    pub fn to_v4(&self, a: &SocketAddr) -> Option<[u8; 4]> {
        match a {
            SocketAddr::V4(v) => Some(v.ip().octets()),
            SocketAddr::V6(v) => {
                let o = v.ip().octets();
                (o[..12] == self.prefix).then(|| [o[12], o[13], o[14], o[15]])
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    /// A name with an IPv6 address first and an IPv4 one after (a hotspot's
    /// resolver, which lists its made IPv6 address first) is taken by the
    /// IPv4; one with only the IPv6 by that. The control: the first address,
    /// as before, is the IPv6 one the IPv4 socket refused.
    #[test]
    fn a_name_is_taken_by_its_ipv4_address() {
        let both = [a("[64:ff9b::5a1:d9bc]:9735"), a("5.161.217.188:9735")];
        assert_eq!(pick(both), Some(a("5.161.217.188:9735")));
        assert_eq!(pick([a("[64:ff9b::5a1:d9bc]:9735")]), Some(a("[64:ff9b::5a1:d9bc]:9735")));
        assert_eq!(pick([]), None);
        assert!(both.into_iter().next().unwrap().is_ipv6(), "the control: the first address is the IPv6 one");
    }

    /// The gateway's way: the made address is the server's IPv4 address under
    /// the prefix, every IPv4 address goes under the same prefix, and only a
    /// datagram from under it is read back as one.
    #[test]
    fn an_ipv4_address_goes_through_the_gateway_and_back() {
        let (n, v4) = Nat64::of(&a("[64:ff9b::5a1:d9bc]:9735")).unwrap();
        assert_eq!(v4, [5, 161, 217, 188]);
        assert_eq!(n.to_v6([5, 161, 217, 188], 10070), a("[64:ff9b::5a1:d9bc]:10070"));
        assert_eq!(n.to_v6([10, 0, 0, 7], 10070), a("[64:ff9b::a00:7]:10070"));
        assert_eq!(n.to_v4(&a("[64:ff9b::a00:7]:10070")), Some([10, 0, 0, 7]));
        assert_eq!(n.to_v4(&a("[2001:db8::a00:7]:10070")), None, "not the gateway's");
        assert_eq!(Nat64::of(&a("5.161.217.188:9735")), None);
    }
}
