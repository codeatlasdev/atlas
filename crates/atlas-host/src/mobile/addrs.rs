//! Which of the endpoint's direct addresses a pairing publishes (codeatlasdev/atlas#113).
//!
//! iroh binds `0.0.0.0` / `[::]` and reports every address of every interface. On a Mac with a VPN
//! (Cloudflare WARP, Tailscale, a corporate client) that includes the tunnel's own addresses: `utun4`
//! `100.96.0.1`, CGNAT `100.64.0.0/10`, a global IPv6 and a `fe80::` one on the same `utun`. A phone on
//! the LAN can't reach any of those, yet it dials every published address, so they only add dead
//! candidates in front of the one that works.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// One address of a local network interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IfAddr {
    pub name: String,
    pub ip: IpAddr,
    /// `IFF_POINTOPOINT`: the link has a single peer (VPNs), so it is not a shared LAN.
    pub point_to_point: bool,
}

/// Interface names of VPNs and tunnels that don't always set `IFF_POINTOPOINT`
/// (`utun` macOS, `tun`/`tap`/`wg` Linux, `ppp`, Tailscale, IPsec, `gif`, ZeroTier `zt…`).
const TUNNEL_PREFIXES: [&str; 9] = [
    "utun",
    "tun",
    "tap",
    "ppp",
    "wg",
    "tailscale",
    "ipsec",
    "gif",
    "zt",
];

fn is_tunnel(i: &IfAddr) -> bool {
    i.point_to_point || TUNNEL_PREFIXES.iter().any(|p| i.name.starts_with(p))
}

/// `100.64.0.0/10` (RFC 6598), where WARP, Tailscale and carrier NAT put their addresses.
fn is_cgnat(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    a == 100 && (b & 0xc0) == 0x40
}

/// Unicast addresses a peer on another host can reach, in principle.
fn is_routable(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_loopback() || v4.is_unspecified() || v4.is_link_local() || is_cgnat(v4))
        }
        IpAddr::V6(v6) => {
            // fe80::/10 needs a scope id the phone doesn't have.
            !(v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xffc0) == 0xfe80)
        }
    }
}

/// Dial order: private IPv4 (the usual LAN), then ULA IPv6, then global v4, then global v6.
fn rank(ip: IpAddr) -> u8 {
    match ip {
        IpAddr::V4(v4) if v4.is_private() => 0,
        IpAddr::V6(v6) if (v6.segments()[0] & 0xfe00) == 0xfc00 => 1,
        IpAddr::V4(_) => 2,
        IpAddr::V6(_) => 3,
    }
}

/// The addresses worth publishing, LAN first. Pure: `ifaces` is the machine's interface list.
///
/// Dropped: loopback, unspecified, link-local, `100.64.0.0/10`, and any address whose only owners are
/// tunnel interfaces. An address no interface claims is kept (the list may lag the endpoint's).
/// When that leaves nothing (loopback-only sandbox, VPN-only host), the input is returned as it came:
/// a candidate that may fail beats an empty pairing.
pub(crate) fn publishable(addrs: &[SocketAddr], ifaces: &[IfAddr]) -> Vec<SocketAddr> {
    let mut keep: Vec<SocketAddr> = addrs
        .iter()
        .copied()
        .filter(|a| is_routable(a.ip()))
        .filter(|a| {
            let mut owners = ifaces.iter().filter(|i| i.ip == a.ip()).peekable();
            owners.peek().is_none() || owners.any(|i| !is_tunnel(i))
        })
        .collect();
    if keep.is_empty() {
        return addrs.to_vec();
    }
    keep.sort_by_key(|a| rank(a.ip()));
    keep
}

/// This machine's interface addresses (empty where `getifaddrs` is not available).
#[cfg(unix)]
pub(crate) fn local_interfaces() -> Vec<IfAddr> {
    use std::ffi::CStr;

    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` fills `head` with a list we only read and then free once with `freeifaddrs`.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut cur = head;
    while !cur.is_null() {
        // SAFETY: `cur` is a non-null node of the list `getifaddrs` returned, alive until `freeifaddrs`.
        let ifa = unsafe { &*cur };
        cur = ifa.ifa_next;
        if ifa.ifa_addr.is_null() {
            continue;
        }
        // SAFETY: `ifa_addr` is non-null and points to a sockaddr whose family says which struct it is.
        let ip = unsafe {
            match i32::from((*ifa.ifa_addr).sa_family) {
                libc::AF_INET => {
                    let sa = &*ifa.ifa_addr.cast::<libc::sockaddr_in>();
                    Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(sa.sin_addr.s_addr))))
                }
                libc::AF_INET6 => {
                    let sa = &*ifa.ifa_addr.cast::<libc::sockaddr_in6>();
                    Some(IpAddr::V6(sa.sin6_addr.s6_addr.into()))
                }
                _ => None,
            }
        };
        let Some(ip) = ip else { continue };
        // SAFETY: `ifa_name` is a NUL-terminated string owned by the list.
        let name = unsafe { CStr::from_ptr(ifa.ifa_name) }
            .to_string_lossy()
            .into_owned();
        out.push(IfAddr {
            name,
            ip,
            point_to_point: ifa.ifa_flags & libc::IFF_POINTOPOINT as libc::c_uint != 0,
        });
    }
    // SAFETY: `head` came from a successful `getifaddrs` and is freed exactly once; no reference outlives it.
    unsafe { libc::freeifaddrs(head) };
    out
}

#[cfg(not(unix))]
pub(crate) fn local_interfaces() -> Vec<IfAddr> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(name: &str, ip: &str, p2p: bool) -> IfAddr {
        IfAddr {
            name: name.into(),
            ip: ip.parse().unwrap(),
            point_to_point: p2p,
        }
    }

    fn sock(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    fn strings(v: Vec<SocketAddr>) -> Vec<String> {
        v.iter().map(|a| a.to_string()).collect()
    }

    /// The Mac from the issue: WARP on `utun4`, Wi-Fi on `en0`.
    fn warp_mac() -> Vec<IfAddr> {
        vec![
            iface("lo0", "127.0.0.1", false),
            iface("lo0", "::1", false),
            iface("en0", "192.168.68.50", false),
            iface("en0", "2804:14d:1::50", false),
            iface("en0", "fe80::1c2b:3d4e:5f60:7182", false),
            iface("utun4", "100.96.0.1", true),
            iface("utun4", "2606:4700:110:8a36::1", true),
            iface("utun4", "fe80::ce1:2d3:4e5:6f7", true),
        ]
    }

    #[test]
    fn warp_mac_publishes_only_the_lan_addresses() {
        let addrs = [
            sock("100.96.0.1:4433"),
            sock("192.168.68.50:4433"),
            sock("[2606:4700:110:8a36::1]:4434"),
            sock("[2804:14d:1::50]:4434"),
            sock("[fe80::ce1:2d3:4e5:6f7]:4434"),
        ];
        assert_eq!(
            strings(publishable(&addrs, &warp_mac())),
            ["192.168.68.50:4433", "[2804:14d:1::50]:4434"]
        );
    }

    #[test]
    fn cgnat_is_dropped_even_when_the_interface_is_unknown() {
        let addrs = [
            sock("100.64.0.1:1"),
            sock("100.127.255.254:1"),
            sock("10.0.0.2:1"),
        ];
        assert_eq!(strings(publishable(&addrs, &[])), ["10.0.0.2:1"]);
    }

    #[test]
    fn cgnat_range_is_exact() {
        let addrs = [sock("100.63.255.255:1"), sock("100.128.0.0:1")];
        assert_eq!(
            strings(publishable(&addrs, &[])),
            ["100.63.255.255:1", "100.128.0.0:1"]
        );
    }

    #[test]
    fn tunnel_names_are_dropped_without_the_point_to_point_flag() {
        for name in [
            "utun0",
            "tun0",
            "tap1",
            "ppp0",
            "wg0",
            "tailscale0",
            "ipsec0",
            "gif0",
            "zt3jnqc4t2",
        ] {
            let ifaces = [
                iface(name, "172.16.5.9", false),
                iface("en0", "192.168.1.5", false),
            ];
            let addrs = [sock("172.16.5.9:1"), sock("192.168.1.5:1")];
            assert_eq!(
                strings(publishable(&addrs, &ifaces)),
                ["192.168.1.5:1"],
                "{name}"
            );
        }
    }

    #[test]
    fn point_to_point_is_dropped_whatever_its_name() {
        let ifaces = [
            iface("corp0", "172.20.0.9", true),
            iface("en0", "192.168.1.5", false),
        ];
        let addrs = [sock("172.20.0.9:1"), sock("192.168.1.5:1")];
        assert_eq!(strings(publishable(&addrs, &ifaces)), ["192.168.1.5:1"]);
    }

    #[test]
    fn docker_and_bridge_names_are_not_tunnels() {
        let ifaces = [
            iface("bridge100", "192.168.64.1", false),
            iface("docker0", "172.17.0.1", false),
            iface("eth0", "10.1.2.3", false),
        ];
        let addrs = [
            sock("192.168.64.1:1"),
            sock("172.17.0.1:1"),
            sock("10.1.2.3:1"),
        ];
        assert_eq!(publishable(&addrs, &ifaces).len(), 3);
    }

    #[test]
    fn loopback_unspecified_and_link_local_are_dropped() {
        let addrs = [
            sock("127.0.0.1:1"),
            sock("[::1]:1"),
            sock("0.0.0.0:1"),
            sock("169.254.10.10:1"),
            sock("[fe80::1]:1"),
            sock("192.168.1.5:1"),
        ];
        assert_eq!(strings(publishable(&addrs, &[])), ["192.168.1.5:1"]);
    }

    #[test]
    fn private_lan_comes_before_global_addresses() {
        let addrs = [
            sock("[2804:14d:1::50]:2"),
            sock("203.0.113.7:1"),
            sock("192.168.1.5:1"),
            sock("[fd00::5]:2"),
            sock("10.0.0.4:1"),
        ];
        assert_eq!(
            strings(publishable(&addrs, &[])),
            [
                "192.168.1.5:1",
                "10.0.0.4:1",
                "[fd00::5]:2",
                "203.0.113.7:1",
                "[2804:14d:1::50]:2"
            ]
        );
    }

    #[test]
    fn an_address_on_a_tunnel_and_a_lan_interface_is_kept() {
        // The same IP can be listed on two interfaces (aliases); one non-tunnel owner is enough.
        let ifaces = [
            iface("utun4", "192.168.1.5", true),
            iface("en0", "192.168.1.5", false),
        ];
        assert_eq!(
            strings(publishable(&[sock("192.168.1.5:1")], &ifaces)),
            ["192.168.1.5:1"]
        );
    }

    /// The reader sees this machine's real interfaces (loopback always exists), and what it reports
    /// never survives the filter when it is loopback or CGNAT, whatever the host's VPN setup is.
    #[cfg(unix)]
    #[test]
    fn local_interfaces_reads_the_machine_and_the_filter_drops_its_tunnels() {
        let ifaces = local_interfaces();
        assert!(
            ifaces.iter().any(|i| i.ip.is_loopback()),
            "no loopback in {ifaces:?}"
        );
        let all: Vec<SocketAddr> = ifaces.iter().map(|i| SocketAddr::new(i.ip, 4433)).collect();
        let kept = publishable(&all, &ifaces);
        let tunnel_only = |ip: IpAddr| {
            let owners: Vec<_> = ifaces.iter().filter(|i| i.ip == ip).collect();
            !owners.is_empty() && owners.iter().all(|i| is_tunnel(i))
        };
        // A loopback-only host (CI sandbox) has nothing dialable: the fallback returns the input.
        let has_lan = all
            .iter()
            .any(|a| is_routable(a.ip()) && !tunnel_only(a.ip()));
        if has_lan {
            for a in &kept {
                assert!(
                    !tunnel_only(a.ip()),
                    "tunnel address kept: {a} in {ifaces:?}"
                );
                assert!(is_routable(a.ip()), "unroutable address kept: {a}");
            }
        }
    }

    #[test]
    fn nothing_dialable_left_falls_back_to_the_input() {
        // A host with only loopback (sandbox, CI) still publishes what the endpoint reported.
        let addrs = [sock("127.0.0.1:4433"), sock("[::1]:4434")];
        assert_eq!(
            strings(publishable(&addrs, &[])),
            ["127.0.0.1:4433", "[::1]:4434"]
        );
    }

    #[test]
    fn only_tunnel_addresses_falls_back_to_the_input() {
        // Better a candidate that may fail than an empty pairing QR (the relay still works).
        let ifaces = [iface("utun4", "100.96.0.1", true)];
        assert_eq!(
            strings(publishable(&[sock("100.96.0.1:1")], &ifaces)),
            ["100.96.0.1:1"]
        );
    }

    #[test]
    fn empty_in_empty_out() {
        assert!(publishable(&[], &warp_mac()).is_empty());
    }
}
