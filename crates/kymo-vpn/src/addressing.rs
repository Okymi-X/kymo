//! IPv4 plan: one pool of lab networks (one small network per player or team) and one pool of
//! tunnel addresses (one per player). Everything is computed from an index, so the database
//! only stores small integers and the addresses can always be re-derived.

use std::collections::BTreeSet;
use std::fmt;
use std::net::Ipv4Addr;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::VpnError;

/// An IPv4 network in CIDR form. Always stored with its host bits cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Ipv4Net {
    base: u32,
    prefix: u8,
}

impl Ipv4Net {
    pub fn new(addr: Ipv4Addr, prefix: u8) -> Result<Self, VpnError> {
        if prefix > 30 {
            return Err(VpnError::Address(format!(
                "/{prefix} is too small for a network"
            )));
        }
        let mask = Self::mask(prefix);
        Ok(Self {
            base: u32::from(addr) & mask,
            prefix,
        })
    }

    fn mask(prefix: u8) -> u32 {
        if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        }
    }

    pub fn prefix(&self) -> u8 {
        self.prefix
    }

    pub fn network(&self) -> Ipv4Addr {
        Ipv4Addr::from(self.base)
    }

    /// The netmask in dotted form, as OpenVPN wants it.
    pub fn netmask(&self) -> Ipv4Addr {
        Ipv4Addr::from(Self::mask(self.prefix))
    }

    pub fn size(&self) -> u64 {
        1u64 << (32 - u32::from(self.prefix))
    }

    pub fn contains(&self, addr: Ipv4Addr) -> bool {
        u32::from(addr) & Self::mask(self.prefix) == self.base
    }

    pub fn overlaps(&self, other: &Ipv4Net) -> bool {
        self.contains(other.network()) || other.contains(self.network())
    }

    /// The address `offset` hosts into the network, or none when it falls outside.
    pub fn host(&self, offset: u32) -> Option<Ipv4Addr> {
        (u64::from(offset) < self.size()).then(|| Ipv4Addr::from(self.base + offset))
    }
}

impl fmt::Display for Ipv4Net {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.network(), self.prefix)
    }
}

impl FromStr for Ipv4Net {
    type Err = VpnError;

    fn from_str(text: &str) -> Result<Self, VpnError> {
        let bad = || VpnError::Address(text.to_owned());
        let (addr, prefix) = text.split_once('/').ok_or_else(bad)?;
        let addr: Ipv4Addr = addr.parse().map_err(|_| bad())?;
        let prefix: u8 = prefix.parse().map_err(|_| bad())?;
        Ipv4Net::new(addr, prefix)
    }
}

/// The first host of every lab network is the network's own gateway address, the next few are
/// left for the VPN gateway container, and instances start at this offset.
pub const FIRST_INSTANCE_OFFSET: u32 = 10;

/// A pool carved into equal lab networks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LabPool {
    pool: Ipv4Net,
    lab_prefix: u8,
}

impl LabPool {
    pub fn new(pool: Ipv4Net, lab_prefix: u8) -> Result<Self, VpnError> {
        if !(pool.prefix() < lab_prefix && lab_prefix <= 28) {
            return Err(VpnError::Address(
                "labs must be smaller than the pool and at least a /28".into(),
            ));
        }
        Ok(Self { pool, lab_prefix })
    }

    pub fn pool(&self) -> Ipv4Net {
        self.pool
    }

    pub fn capacity(&self) -> u32 {
        1 << (self.lab_prefix - self.pool.prefix())
    }

    /// The network of lab number `index`.
    pub fn lab(&self, index: u32) -> Option<Ipv4Net> {
        if index >= self.capacity() {
            return None;
        }
        let step = 1u32 << (32 - self.lab_prefix);
        Ipv4Net::new(
            Ipv4Addr::from(u32::from(self.pool.network()) + index * step),
            self.lab_prefix,
        )
        .ok()
    }

    /// The lowest index not in `used`.
    pub fn next_free(&self, used: &BTreeSet<u32>) -> Result<u32, VpnError> {
        (0..self.capacity())
            .find(|i| !used.contains(i))
            .ok_or(VpnError::PoolFull)
    }

    /// Addresses an instance may take inside `lab`, in order.
    pub fn instance_slots(&self, lab: &Ipv4Net) -> Vec<Ipv4Addr> {
        (FIRST_INSTANCE_OFFSET..lab.size() as u32 - 1)
            .filter_map(|o| lab.host(o))
            .collect()
    }
}

/// Tunnel addresses handed to players. The first address is the server's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TunnelPool {
    net: Ipv4Net,
}

impl TunnelPool {
    pub fn new(net: Ipv4Net) -> Result<Self, VpnError> {
        if net.prefix() > 24 {
            return Err(VpnError::Address(
                "the tunnel network must be a /24 or larger".into(),
            ));
        }
        Ok(Self { net })
    }

    pub fn net(&self) -> Ipv4Net {
        self.net
    }

    pub fn server_ip(&self) -> Ipv4Addr {
        self.net.host(1).unwrap_or_else(|| self.net.network())
    }

    pub fn capacity(&self) -> u32 {
        (self.net.size() - 3) as u32
    }

    /// The tunnel address of client number `index` (starting at the third address).
    pub fn client(&self, index: u32) -> Option<Ipv4Addr> {
        (index < self.capacity())
            .then(|| self.net.host(index + 2))
            .flatten()
    }

    pub fn next_free(&self, used: &BTreeSet<u32>) -> Result<u32, VpnError> {
        (0..self.capacity())
            .find(|i| !used.contains(i))
            .ok_or(VpnError::PoolFull)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn net(text: &str) -> Ipv4Net {
        text.parse().unwrap()
    }

    #[test]
    fn networks_parse_clear_host_bits_and_print_back() {
        assert_eq!(net("10.77.5.9/24").to_string(), "10.77.5.0/24");
        assert_eq!(net("10.77.0.0/16").netmask(), Ipv4Addr::new(255, 255, 0, 0));
        for bad in [
            "10.0.0.0",
            "10.0.0.0/33",
            "10.0.0/24",
            "x/24",
            "10.0.0.0/31",
            "10.0.0.0/ 8",
        ] {
            assert!(bad.parse::<Ipv4Net>().is_err(), "{bad}");
        }
    }

    #[test]
    fn containment_and_overlap() {
        let pool = net("10.77.0.0/16");
        assert!(pool.contains(Ipv4Addr::new(10, 77, 255, 255)));
        assert!(!pool.contains(Ipv4Addr::new(10, 78, 0, 0)));
        assert!(pool.overlaps(&net("10.77.4.0/27")));
        assert!(!pool.overlaps(&net("10.8.0.0/16")));
    }

    #[test]
    fn labs_are_adjacent_non_overlapping_and_bounded() {
        let pool = LabPool::new(net("10.77.0.0/16"), 27).unwrap();
        assert_eq!(pool.capacity(), 2048);
        assert_eq!(pool.lab(0).unwrap().to_string(), "10.77.0.0/27");
        assert_eq!(pool.lab(1).unwrap().to_string(), "10.77.0.32/27");
        assert_eq!(pool.lab(2047).unwrap().to_string(), "10.77.255.224/27");
        assert!(pool.lab(2048).is_none());
        for i in 0..64 {
            assert!(!pool.lab(i).unwrap().overlaps(&pool.lab(i + 1).unwrap()));
        }
    }

    #[test]
    fn the_lowest_free_index_is_reused() {
        let pool = LabPool::new(net("10.77.0.0/16"), 27).unwrap();
        let used: BTreeSet<u32> = [0, 1, 3].into();
        assert_eq!(pool.next_free(&used).unwrap(), 2);
        let full: BTreeSet<u32> = (0..2048).collect();
        assert_eq!(pool.next_free(&full), Err(VpnError::PoolFull));
    }

    #[test]
    fn instance_slots_skip_the_gateway_range_and_the_broadcast_address() {
        let pool = LabPool::new(net("10.77.0.0/16"), 27).unwrap();
        let slots = pool.instance_slots(&pool.lab(1).unwrap());
        assert_eq!(slots.first(), Some(&Ipv4Addr::new(10, 77, 0, 42)));
        assert_eq!(slots.last(), Some(&Ipv4Addr::new(10, 77, 0, 62)));
        assert_eq!(slots.len(), 21);
    }

    #[test]
    fn lab_pool_rejects_nonsense_shapes() {
        assert!(LabPool::new(net("10.77.0.0/16"), 16).is_err());
        assert!(LabPool::new(net("10.77.0.0/16"), 30).is_err());
        assert!(LabPool::new(net("10.77.0.0/28"), 27).is_err());
    }

    #[test]
    fn tunnel_clients_start_after_the_server_address() {
        let pool = TunnelPool::new(net("10.8.0.0/16")).unwrap();
        assert_eq!(pool.server_ip(), Ipv4Addr::new(10, 8, 0, 1));
        assert_eq!(pool.client(0), Some(Ipv4Addr::new(10, 8, 0, 2)));
        assert_eq!(pool.client(254), Some(Ipv4Addr::new(10, 8, 1, 0)));
        assert_eq!(pool.capacity(), 65533);
        assert!(pool.client(65533).is_none());
        assert!(TunnelPool::new(net("10.8.0.0/28")).is_err());
    }
}
