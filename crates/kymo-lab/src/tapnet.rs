//! Puts a virtual machine on its player's lab network as a machine of its own: a tap device on
//! the network's bridge, and a tiny DHCP server that gives the guest the address the platform
//! reserved for it. Every port, TCP and UDP, is then open on that one address, and the player can
//! scan it like any machine on HackTheBox or TryHackMe.
//!
//! It needs the right to create a tap device and a bridge port (`CAP_NET_ADMIN`) and to open a
//! packet socket (`CAP_NET_RAW`); the installer's service unit grants exactly those, or the lab
//! host runs as root.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::Ipv4Addr;
use std::time::Duration;

use tokio::process::Command;
use tokio::task::JoinHandle;

/// Where a machine sits: its address, the network's own address (the bridge) and prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TapSpec {
    pub ip: Ipv4Addr,
    /// The bridge's address on this network, which names the bridge to attach to.
    pub gateway: Ipv4Addr,
    pub prefix: u8,
}

fn hash(name: &str) -> u64 {
    let mut h = DefaultHasher::new();
    name.hash(&mut h);
    h.finish()
}

/// The tap device's name: short (the kernel allows 15 characters) and unique per machine.
pub fn ifname(name: &str) -> String {
    format!("kt{:010x}", hash(name) & 0xff_ffff_ffff)
}

/// A locally administered MAC address, the same every time for the same machine.
pub fn mac(name: &str) -> [u8; 6] {
    let h = hash(&format!("mac:{name}")).to_be_bytes();
    [0x52, 0x54, 0x00, h[0], h[1], h[2]]
}

pub fn mac_text(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

async fn ip(args: &[&str]) -> Result<String, String> {
    let out = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new("ip").args(args).output(),
    )
    .await
    .map_err(|_| "the ip command took too long".to_owned())?
    .map_err(|e| format!("cannot run ip: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "ip {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The bridge interface that carries `address`, which is how a lab network's bridge is found
/// without asking Docker or Podman which name they gave it.
pub async fn bridge_for(address: Ipv4Addr) -> Result<(String, u8), String> {
    let text = ip(&["-o", "-4", "addr", "show"]).await?;
    find_interface(&text, address).ok_or_else(|| {
        format!("no network interface has {address}: the lab network does not exist on this machine (rootless Podman keeps it out of reach)")
    })
}

/// Reads `ip -o -4 addr show` for the interface that has `address`, and its prefix length.
fn find_interface(text: &str, address: Ipv4Addr) -> Option<(String, u8)> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let _index = parts.next()?;
        let name = parts.next()?.trim_end_matches(':');
        let mut rest = parts;
        while let Some(word) = rest.next() {
            if word == "inet" {
                let cidr = rest.next()?;
                let (found, prefix) = cidr.split_once('/')?;
                if found == address.to_string() {
                    return Some((name.split('@').next()?.to_owned(), prefix.parse().ok()?));
                }
            }
        }
        None
    })
}

/// The uid the tap must belong to, so an unprivileged QEMU may open it.
fn own_uid() -> u32 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata("/proc/self").map_or(0, |m| m.uid())
}

/// Makes the tap and plugs it into the lab network's bridge.
pub async fn create(ifname: &str, bridge: &str) -> Result<(), String> {
    let uid = own_uid().to_string();
    ip(&["tuntap", "add", "dev", ifname, "mode", "tap", "user", &uid]).await?;
    let plug = async {
        ip(&["link", "set", ifname, "master", bridge]).await?;
        ip(&["link", "set", ifname, "up"]).await
    }
    .await;
    if let Err(e) = plug {
        remove(ifname).await;
        return Err(e);
    }
    Ok(())
}

/// Removes the tap. A tap that is already gone is fine.
pub async fn remove(ifname: &str) {
    let _ = ip(&["link", "del", ifname]).await;
}

// ---------- DHCP ----------

const COOKIE: [u8; 4] = [99, 130, 83, 99];
const DISCOVER: u8 = 1;
const REQUEST: u8 = 3;
const OFFER: u8 = 2;
const ACK: u8 = 5;

fn netmask(prefix: u8) -> Ipv4Addr {
    Ipv4Addr::from(u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0))
}

/// What a client sent: the message type, transaction id, flags and hardware address.
struct Query {
    kind: u8,
    xid: [u8; 4],
    flags: [u8; 2],
    mac: [u8; 6],
}

fn parse(packet: &[u8]) -> Option<Query> {
    if packet.len() < 241 || packet[0] != 1 || packet[1] != 1 || packet[2] != 6 {
        return None;
    }
    if packet[236..240] != COOKIE {
        return None;
    }
    let mut kind = None;
    let mut i = 240;
    while i < packet.len() {
        match packet[i] {
            255 => break,
            0 => i += 1,
            code => {
                let len = usize::from(*packet.get(i + 1)?);
                if code == 53 && len == 1 {
                    kind = packet.get(i + 2).copied();
                }
                i += 2 + len;
            }
        }
    }
    Some(Query {
        kind: kind?,
        xid: packet[4..8].try_into().ok()?,
        flags: packet[10..12].try_into().ok()?,
        mac: packet[28..34].try_into().ok()?,
    })
}

/// The answer to a client, or none when it is not for this machine or not a question we answer.
fn answer(packet: &[u8], expected_mac: [u8; 6], spec: &TapSpec) -> Option<Vec<u8>> {
    let q = parse(packet)?;
    if q.mac != expected_mac {
        return None;
    }
    let reply = match q.kind {
        DISCOVER => OFFER,
        REQUEST => ACK,
        _ => return None,
    };
    let mut out = vec![0u8; 240];
    out[0] = 2;
    out[1] = 1;
    out[2] = 6;
    out[4..8].copy_from_slice(&q.xid);
    out[10..12].copy_from_slice(&q.flags);
    out[16..20].copy_from_slice(&spec.ip.octets());
    out[20..24].copy_from_slice(&spec.gateway.octets());
    out[28..34].copy_from_slice(&q.mac);
    out[236..240].copy_from_slice(&COOKIE);
    let mut option = |code: u8, data: &[u8]| {
        out.push(code);
        out.push(u8::try_from(data.len()).unwrap_or(0));
        out.extend_from_slice(data);
    };
    option(53, &[reply]);
    option(54, &spec.gateway.octets());
    option(51, &86_400u32.to_be_bytes());
    option(58, &43_200u32.to_be_bytes());
    option(59, &75_600u32.to_be_bytes());
    option(1, &netmask(spec.prefix).octets());
    option(3, &spec.gateway.octets());
    out.push(255);
    // Some clients want at least a BOOTP minimum size.
    while out.len() < 300 {
        out.push(0);
    }
    Some(out)
}

// ---------- raw frames ----------
//
// The lab host's firewall (ufw, firewalld) drops what a bridge sends to the host, DHCP requests
// included, and the installer should not have to open a port for this. A packet socket sees the
// frames before the firewall does, and the answer is built by hand, so nothing is listened on.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

use tokio::io::unix::AsyncFd;

const ETH_P_IP: u16 = 0x0800;

fn packet_socket(bridge: &str) -> Result<AsyncFd<OwnedFd>, String> {
    let name = std::ffi::CString::new(bridge).map_err(|e| e.to_string())?;
    // SAFETY: plain libc calls with checked results; the descriptor is owned right away.
    unsafe {
        let index = libc::if_nametoindex(name.as_ptr());
        if index == 0 {
            return Err(format!("no interface {bridge}"));
        }
        let fd = libc::socket(
            libc::AF_PACKET,
            libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            i32::from(ETH_P_IP.to_be()),
        );
        if fd < 0 {
            return Err(format!(
                "cannot open a packet socket: {} (needs CAP_NET_RAW)",
                std::io::Error::last_os_error()
            ));
        }
        let owned = OwnedFd::from_raw_fd(fd);
        let mut addr: libc::sockaddr_ll = std::mem::zeroed();
        addr.sll_family = libc::AF_PACKET as u16;
        addr.sll_protocol = ETH_P_IP.to_be();
        addr.sll_ifindex = i32::try_from(index).unwrap_or(0);
        let r = libc::bind(
            owned.as_raw_fd(),
            std::ptr::addr_of!(addr).cast(),
            std::mem::size_of::<libc::sockaddr_ll>() as u32,
        );
        if r != 0 {
            return Err(format!(
                "cannot listen on {bridge}: {}",
                std::io::Error::last_os_error()
            ));
        }
        AsyncFd::register(owned).map_err(|e| e.to_string())
    }
}

fn checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    for pair in data.chunks(2) {
        sum += u32::from(u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]));
    }
    while sum > 0xffff {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// The DHCP payload of a frame addressed to the DHCP server port, or none.
fn dhcp_payload(frame: &[u8]) -> Option<&[u8]> {
    if frame.len() < 14 + 20 + 8 || frame[12..14] != ETH_P_IP.to_be_bytes() {
        return None;
    }
    let ip = &frame[14..];
    let header = usize::from(ip[0] & 0x0f) * 4;
    if ip[0] >> 4 != 4 || ip[9] != 17 || ip.len() < header + 8 {
        return None;
    }
    let udp = &ip[header..];
    if u16::from_be_bytes([udp[2], udp[3]]) != 67 {
        return None;
    }
    Some(&udp[8..])
}

/// A broadcast Ethernet frame carrying `payload` from the server's port 67 to the client's 68.
fn reply_frame(payload: &[u8], server: Ipv4Addr, server_mac: [u8; 6]) -> Vec<u8> {
    let udp_len = 8 + payload.len();
    let total = 20 + udp_len;
    let mut f = Vec::with_capacity(14 + total);
    f.extend_from_slice(&[0xff; 6]);
    f.extend_from_slice(&server_mac);
    f.extend_from_slice(&ETH_P_IP.to_be_bytes());
    let mut ip = vec![0x45, 0, 0, 0, 0, 0, 0x40, 0, 64, 17, 0, 0];
    ip[2..4].copy_from_slice(&u16::try_from(total).unwrap_or(0).to_be_bytes());
    ip.extend_from_slice(&server.octets());
    ip.extend_from_slice(&Ipv4Addr::BROADCAST.octets());
    let sum = checksum(&ip);
    ip[10..12].copy_from_slice(&sum.to_be_bytes());
    f.extend_from_slice(&ip);
    f.extend_from_slice(&67u16.to_be_bytes());
    f.extend_from_slice(&68u16.to_be_bytes());
    f.extend_from_slice(&u16::try_from(udp_len).unwrap_or(0).to_be_bytes());
    // A zero UDP checksum means none, which IPv4 allows.
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(payload);
    f
}

fn send_frame(fd: &OwnedFd, index: i32, frame: &[u8]) {
    // SAFETY: the address and the buffer outlive the call.
    unsafe {
        let mut addr: libc::sockaddr_ll = std::mem::zeroed();
        addr.sll_family = libc::AF_PACKET as u16;
        addr.sll_protocol = ETH_P_IP.to_be();
        addr.sll_ifindex = index;
        addr.sll_halen = 6;
        addr.sll_addr[..6].copy_from_slice(&[0xff; 6]);
        libc::sendto(
            fd.as_raw_fd(),
            frame.as_ptr().cast(),
            frame.len(),
            0,
            std::ptr::addr_of!(addr).cast(),
            std::mem::size_of::<libc::sockaddr_ll>() as u32,
        );
    }
}

/// Starts answering the machine's DHCP requests, and only the machine's: its address is the one
/// the platform reserved. A bridge port's traffic arrives on the bridge itself, so that is what is
/// watched (several machines share it; each answers only its own hardware address). Opened before
/// QEMU starts so the first request is not lost.
pub fn serve(bridge: &str, mac: [u8; 6], spec: TapSpec) -> Result<JoinHandle<()>, String> {
    let sock = packet_socket(bridge)?;
    let name = std::ffi::CString::new(bridge).map_err(|e| e.to_string())?;
    // SAFETY: a valid C string.
    let index = i32::try_from(unsafe { libc::if_nametoindex(name.as_ptr()) }).unwrap_or(0);
    // The reply claims to come from the network's own address, with a made up hardware address.
    let server_mac = [0x02, 0x00, 0x00, 0x4b, 0x59, 0x4d];
    Ok(tokio::spawn(async move {
        let mut buf = vec![0u8; 2048];
        loop {
            let Ok(mut ready) = sock.readable().await else {
                return;
            };
            let got = ready.try_io(|fd| {
                // SAFETY: the buffer is valid for its length.
                let n =
                    unsafe { libc::recv(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
                if n < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(n as usize)
                }
            });
            match got {
                Ok(Ok(n)) => {
                    if let Some(payload) = dhcp_payload(&buf[..n])
                        && let Some(reply) = answer(payload, mac, &spec)
                    {
                        let frame = reply_frame(&reply, spec.gateway, server_mac);
                        send_frame(sock.get_ref(), index, &frame);
                    }
                }
                Ok(Err(_)) => tokio::time::sleep(Duration::from_millis(50)).await,
                Err(_would_block) => {}
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> TapSpec {
        TapSpec {
            ip: "10.77.0.42".parse().unwrap(),
            gateway: "10.77.0.33".parse().unwrap(),
            prefix: 27,
        }
    }

    fn discover(mac: [u8; 6], kind: u8) -> Vec<u8> {
        let mut p = vec![0u8; 240];
        p[0] = 1;
        p[1] = 1;
        p[2] = 6;
        p[4..8].copy_from_slice(&[1, 2, 3, 4]);
        p[10] = 0x80;
        p[28..34].copy_from_slice(&mac);
        p[236..240].copy_from_slice(&COOKIE);
        p.extend_from_slice(&[53, 1, kind, 255]);
        p
    }

    #[test]
    fn the_machine_is_offered_and_acknowledged_the_address_the_platform_reserved() {
        let m = mac("vm-1");
        for (ask, want) in [(DISCOVER, OFFER), (REQUEST, ACK)] {
            let reply = answer(&discover(m, ask), m, &spec()).unwrap();
            assert_eq!(&reply[16..20], &[10, 77, 0, 42], "yiaddr");
            assert_eq!(&reply[4..8], &[1, 2, 3, 4], "the client's transaction");
            assert_eq!(reply[0], 2);
            let at = reply.windows(3).position(|w| w == [53, 1, want]);
            assert!(at.is_some(), "message type {want}");
            // Subnet mask of a /27 is in the options.
            assert!(reply.windows(6).any(|w| w == [1, 4, 255, 255, 255, 224]));
        }
    }

    #[test]
    fn only_that_machine_gets_an_answer() {
        let m = mac("vm-1");
        assert!(answer(&discover(mac("vm-2"), DISCOVER), m, &spec()).is_none());
        assert!(
            answer(&discover(m, 7), m, &spec()).is_none(),
            "not a question we answer"
        );
        assert!(
            answer(&[0u8; 20], m, &spec()).is_none(),
            "too short to be DHCP"
        );
        let mut bad = discover(m, DISCOVER);
        bad[236] = 0;
        assert!(answer(&bad, m, &spec()).is_none(), "no magic cookie");
    }

    #[test]
    fn names_and_addresses_are_stable_short_and_locally_administered() {
        assert_eq!(ifname("a"), ifname("a"));
        assert_ne!(ifname("a"), ifname("b"));
        assert!(ifname("kymo-c1-u7-abcdef").len() <= 15);
        let m = mac("a");
        assert_eq!(m[0] & 0x03, 0x02, "locally administered, unicast");
        assert_eq!(mac_text([0x52, 0x54, 0, 1, 2, 3]), "52:54:00:01:02:03");
    }

    #[test]
    fn the_bridge_is_found_by_its_address() {
        let text = "1: lo    inet 127.0.0.1/8 scope host lo\n7: br-abc123    inet 10.77.0.33/27 brd 10.77.0.63 scope global br-abc123\n9: veth1@if8    inet 10.9.0.1/24 scope global veth1\n";
        assert_eq!(
            find_interface(text, "10.77.0.33".parse().unwrap()),
            Some(("br-abc123".to_owned(), 27))
        );
        assert_eq!(find_interface(text, "10.1.1.1".parse().unwrap()), None);
    }

    #[test]
    fn a_reply_frame_is_a_valid_broadcast_from_the_server_port_to_the_client_port() {
        let f = reply_frame(
            &[1, 2, 3, 4],
            "10.77.0.33".parse().unwrap(),
            [2, 0, 0, 0, 0, 1],
        );
        assert_eq!(&f[..6], &[0xff; 6], "broadcast");
        assert_eq!(&f[12..14], &[8, 0]);
        // The IPv4 header checksums to zero when it is right.
        assert_eq!(checksum(&f[14..34]), 0);
        assert_eq!(
            &f[26..30],
            &[10, 77, 0, 33],
            "from the network's own address"
        );
        assert_eq!(&f[30..34], &[255, 255, 255, 255]);
        assert_eq!(
            (
                u16::from_be_bytes([f[34], f[35]]),
                u16::from_be_bytes([f[36], f[37]])
            ),
            (67, 68)
        );
        assert_eq!(&f[42..], &[1, 2, 3, 4]);
        // And what a client sends to port 67 is recognised, not what goes to 68.
        let mut request = f.clone();
        request[36..38].copy_from_slice(&67u16.to_be_bytes());
        assert_eq!(dhcp_payload(&request), Some(&[1u8, 2, 3, 4][..]));
        assert!(dhcp_payload(&f).is_none());
        assert!(dhcp_payload(&[0u8; 10]).is_none());
    }

    #[test]
    fn masks() {
        assert_eq!(netmask(27), Ipv4Addr::new(255, 255, 255, 224));
        assert_eq!(netmask(16), Ipv4Addr::new(255, 255, 0, 0));
    }
}
