#![allow(dead_code)]
// src/net.rs
//
// RTL8139 + minimal Ethernet/ARP/IPv4/UDP/DHCP + ICMP ping (polling).
//
// IMPORTANT (DMA):
// - RBSTART / TSAD* must be PHYSICAL addresses.
// - This code assumes your kernel & these statics are identity-mapped below 4GiB.
//   If your kernel is higher-half, you MUST translate virt->phys for these buffers.
//
// IMPORTANT (RX ring):
// - The RTL8139 receive ring "wraps" at 8 KiB, but hardware can write into an
//   "overflow" area appended after the ring. We allocate RX_RING + 16 + 2048,
//   and read packets linearly using that overflow area (no modulo indexing).

use core::str;

use crate::portio::{inb, inl, inw, outb, outl, outw};
use crate::time;

pub mod dns;
pub mod tcp;
pub mod http;
pub mod tls;

// -----------------------------------------------------------------------------
// Public surface
// -----------------------------------------------------------------------------

pub struct NetScanResult {
    pub devices: &'static [&'static str],
}

#[derive(Copy, Clone)]
pub struct NetConfig {
    pub nic_present: bool,
    pub mac: [u8; 6],

    pub dhcp_bound: bool,
    pub ip: [u8; 4],
    pub mask: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    pub server_id: [u8; 4],
    pub lease_seconds: u32,
}

impl NetConfig {
    pub const fn empty() -> Self {
        Self {
            nic_present: false,
            mac: [0; 6],
            dhcp_bound: false,
            ip: [0; 4],
            mask: [0; 4],
            gateway: [0; 4],
            dns: [0; 4],
            server_id: [0; 4],
            lease_seconds: 0,
        }
    }
}

#[derive(Copy, Clone)]
pub struct NetStats {
    pub rx_packets: u32,
    pub tx_packets: u32,
    pub rx_dropped: u32,
}

#[derive(Copy, Clone, Debug)]
pub enum DhcpError {
    NoNic,
    Timeout,
    Malformed,
    Nack,
}

#[derive(Copy, Clone, Debug)]
pub enum PingError {
    NoNic,
    NotConfigured,
    ArpTimeout,
    Timeout,
    TxFail,
}

#[derive(Copy, Clone)]
pub struct PingReply {
    pub seq: u16,
    pub ttl: u8,
    pub rtt_tsc: u64,
}

// -----------------------------------------------------------------------------
// RTL8139 definitions
// -----------------------------------------------------------------------------

const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

const RTL_VENDOR_ID: u16 = 0x10ec;
const RTL_DEVICE_ID: u16 = 0x8139;

// RTL8139 registers (I/O base + offset)
const IDR0: u16 = 0x00;
const TSD0: u16 = 0x10;
const TSAD0: u16 = 0x20;
const RBSTART: u16 = 0x30;
const CAPR: u16 = 0x38;
const CBR: u16 = 0x3A;
const IMR: u16 = 0x3C;
const ISR: u16 = 0x3E;
const TCR: u16 = 0x40;
const RCR: u16 = 0x44;
const CR: u16 = 0x37;

const CR_RESET: u8 = 0x10;
const CR_RX_ENABLE: u8 = 0x08;
const CR_TX_ENABLE: u8 = 0x04;

// RX ring (wraps at 8 KiB)
const RX_RING_LEN: usize = 8192;
const RX_OVERFLOW: usize = 16 + 2048;
const RX_BUFFER_SIZE: usize = RX_RING_LEN + RX_OVERFLOW;

const TX_BUF_SIZE: usize = 1600;
const NUM_TX: usize = 4;

#[repr(align(16))]
struct AlignedRx([u8; RX_BUFFER_SIZE]);

#[link_section = ".nic_rx_buffer"]
static mut RX_BUFFER: AlignedRx = AlignedRx([0u8; RX_BUFFER_SIZE]);

#[link_section = ".nic_tx_buffers"]
static mut TX_BUFFERS: [[u8; TX_BUF_SIZE]; NUM_TX] = [[0u8; TX_BUF_SIZE]; NUM_TX];

static mut RX_SCRATCH: [u8; 2048] = [0u8; 2048];

struct Rtl8139 {
    io: u16,
    mac: [u8; 6],
    rx_off: usize, // 0..RX_RING_LEN
    tx_cur: usize,
}

impl Rtl8139 {
    fn init(io: u16) -> Self {
        let mut mac = [0u8; 6];

        unsafe {
            // reset
            outb(io + CR, CR_RESET);
            while (inb(io + CR) & CR_RESET) != 0 {
                time::cpu_pause();
            }

            // read MAC
            for i in 0..6 {
                mac[i] = inb(io + IDR0 + i as u16);
            }

            // program RX buffer physical address (assumes identity map <4GB)
            let rx_phys = (&RX_BUFFER as *const _ as u64) as u32;
            outl(io + RBSTART, rx_phys);

            // RX config:
            // - accept physical match (bit1) + broadcast (bit3)
            // - wrap (bit7)
            // - MXDMA = unlimited (bits 10:8 = 111)
            let rcr = (1u32 << 0) | (1u32 << 1) | (1u32 << 3) | (1u32 << 7) | (0x7u32 << 8);
            outl(io + RCR, rcr);

            // TX config: MXDMA unlimited
            outl(io + TCR, (0x7u32 << 8));

            // Enable RX OK in IMR (we poll, but some QEMU models behave better with it enabled)
            outw(io + IMR, 0x0001);
            outw(io + ISR, 0xFFFF);

            // enable RX/TX
            outb(io + CR, CR_RX_ENABLE | CR_TX_ENABLE);

            // CAPR is "read pointer - 16"
            outw(io + CAPR, 0xFFF0);
        }

        Self { io, mac, rx_off: 0, tx_cur: 0 }
    }

    fn send_frame(&mut self, dst: [u8; 6], ethertype: u16, payload: &[u8]) -> bool {
        // Ethernet minimum frame size is 60 bytes (excluding 4-byte CRC).
        // If we transmit runt frames, some backends/emulations will drop them,
        // which breaks ARP and small TCP packets (e.g. SYN is 58 bytes).
        let total = 14 + payload.len();
        let tx_total = core::cmp::max(total, 60);
        if tx_total > TX_BUF_SIZE {
            return false;
        }

        // Pick a TX descriptor that looks idle. Some emulations will drop frames if we
// stomp an in-flight descriptor.
let mut idx = self.tx_cur % NUM_TX;
for _ in 0..NUM_TX {
    let tsd = unsafe { inl(self.io + TSD0 + (idx as u16) * 4) };
    // OWN (bit13) is typically set while the NIC owns the descriptor.
    // If an emulation uses different semantics, this just becomes a best-effort heuristic.
    let own = (tsd & (1u32 << 13)) != 0;
    if !own { break; }
    idx = (idx + 1) % NUM_TX;
}
self.tx_cur = (idx + 1) % NUM_TX;

        unsafe {
            let buf = &mut TX_BUFFERS[idx];
            buf[0..6].copy_from_slice(&dst);
            buf[6..12].copy_from_slice(&self.mac);
            buf[12..14].copy_from_slice(&ethertype.to_be_bytes());
            buf[14..14 + payload.len()].copy_from_slice(payload);
            if tx_total > total {
                // Zero padding to satisfy minimum frame size.
                for b in buf[14 + payload.len()..tx_total].iter_mut() { *b = 0; }
            }

            let phys = (&TX_BUFFERS[idx] as *const _ as u64) as u32;
            outl(self.io + TSAD0 + (idx as u16) * 4, phys);
            outl(self.io + TSD0 + (idx as u16) * 4, tx_total as u32);
        }

        unsafe { NET.stats.tx_packets = NET.stats.tx_packets.wrapping_add(1); }
        true
    }

    fn poll_recv(&mut self) -> Option<&'static [u8]> {
        // Poll ring write pointer (CBR) instead of gating on ISR bits.
        let cbr = unsafe { inw(self.io + CBR) as usize } & (RX_RING_LEN - 1);
        if cbr == self.rx_off {
            return None;
        }

        unsafe {
            let ring = &RX_BUFFER.0;

            // Header is 4 bytes at rx_off (status + length), potentially in the overflow region.
            let off = self.rx_off;
            let status = u16::from_le_bytes([ring[off], ring[off + 1]]);
            let len_raw = u16::from_le_bytes([ring[off + 2], ring[off + 3]]) as usize;

            // Length is the whole frame bytes (usually includes CRC). Keep it conservative.
            if len_raw < 14 || len_raw > 2048 {
                NET.stats.rx_dropped = NET.stats.rx_dropped.wrapping_add(1);
                self.advance_rx(align4(4 + len_raw));
                outw(self.io + ISR, 0xFFFF);
                return None;
            }

            // Some emulators/NICs report the length including the 4-byte Ethernet FCS (CRC),
            // others do not. If we blindly subtract 4 we can truncate ARP/IP packets and then
            // everything (ARP, DHCP, TCP) mysteriously times out.
            //
            // Strategy:
            // 1) Copy exactly `len_raw` bytes into RX_SCRATCH (bounded).
            // 2) Trim the returned slice using L2 ethertype + IPv4 total length when possible.
            let start = off + 4;

            let copy_len = len_raw.min(RX_SCRATCH.len());
            if start + copy_len <= ring.len() {
                RX_SCRATCH[..copy_len].copy_from_slice(&ring[start..start + copy_len]);
            } else {
                // Extremely rare fallback (shouldn't happen with our overflow size)
                let mut j = 0usize;
                while j < copy_len {
                    let src = (start + j) & (RX_RING_LEN - 1);
                    RX_SCRATCH[j] = ring[src];
                    j += 1;
                }
            }

            // Advance rx ptr: header(4) + len_raw, aligned to dword, wrap at 8KiB.
            self.advance_rx(align4(4 + len_raw));

            // ack everything (we poll)
            outw(self.io + ISR, 0xFFFF);

            if (status & 0x0001) == 0 {
                NET.stats.rx_dropped = NET.stats.rx_dropped.wrapping_add(1);
                return None;
            }

            // Trim padding/FCS for the consumer.
            let mut out_len = copy_len.max(14);

            if copy_len >= 14 {
                let mut l2 = 14usize;
                let mut ethertype = u16::from_be_bytes([RX_SCRATCH[12], RX_SCRATCH[13]]);
                if ethertype == 0x8100 && copy_len >= 18 {
                    ethertype = u16::from_be_bytes([RX_SCRATCH[16], RX_SCRATCH[17]]);
                    l2 = 18;
                }

                if ethertype == 0x0806 {
                    // ARP header is 28 bytes after Ethernet/VLAN header.
                    let want = l2 + 28;
                    if want <= copy_len { out_len = want; }
                } else if ethertype == 0x0800 && copy_len >= l2 + 20 {
                    // IPv4: use Total Length field to trim trailing pad/CRC.
                    let ip = l2;
                    let ver_ihl = RX_SCRATCH[ip];
                    if (ver_ihl >> 4) == 4 {
                        let ihl = ((ver_ihl & 0x0F) as usize) * 4;
                        if ihl >= 20 && copy_len >= l2 + ihl + 4 {
                            let total = u16::from_be_bytes([RX_SCRATCH[ip + 2], RX_SCRATCH[ip + 3]]) as usize;
                            let want = l2 + total;
                            if want >= 14 && want <= copy_len { out_len = want; }
                        }
                    }
                }
            }

            NET.stats.rx_packets = NET.stats.rx_packets.wrapping_add(1);
            Some(&RX_SCRATCH[..out_len.min(copy_len)])
        }
    }

    fn advance_rx(&mut self, bytes: usize) {
        self.rx_off = (self.rx_off + bytes) & (RX_RING_LEN - 1);

        // CAPR wants (rx_off - 16) modulo ring length.
        let capr = (self.rx_off.wrapping_sub(16) & (RX_RING_LEN - 1)) as u16;
        unsafe { outw(self.io + CAPR, capr); }
    }
}

#[inline(always)]
fn align4(x: usize) -> usize { (x + 3) & !3 }

// -----------------------------------------------------------------------------
// Global net state
// -----------------------------------------------------------------------------

struct NetState {
    rtl: Option<Rtl8139>,
    cfg: NetConfig,
    stats: NetStats,

    // tiny ARP cache (1 entry)
    arp_valid: bool,
    arp_ip: [u8; 4],
    arp_mac: [u8; 6],
}

static mut NET: NetState = NetState {
    rtl: None,
    cfg: NetConfig::empty(),
    stats: NetStats { rx_packets: 0, tx_packets: 0, rx_dropped: 0 },

    arp_valid: false,
    arp_ip: [0; 4],
    arp_mac: [0; 6],
};

// net_scan pretty strings (no alloc)
static mut SCAN_BUFS: [[u8; 96]; 4] = [[0; 96]; 4];
static mut SCAN_STRS: [&'static str; 4] = ["", "", "", ""];

// -----------------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------------

pub fn init() {
    unsafe {
        if NET.rtl.is_some() {
            return;
        }

        if let Some(io) = pci_find_rtl8139_io() {
            let rtl = Rtl8139::init(io);
            NET.cfg.nic_present = true;
            NET.cfg.mac = rtl.mac;
            NET.rtl = Some(rtl);
        }
    }
}

pub fn mac() -> Option<[u8; 6]> {
    unsafe {
        if NET.cfg.nic_present { Some(NET.cfg.mac) } else { None }
    }
}


pub fn config() -> NetConfig {
    unsafe { NET.cfg }
}

pub fn stats() -> NetStats {
    unsafe { NET.stats }
}

pub fn set_static_config(ip: [u8; 4], mask: [u8; 4], gateway: [u8; 4], dns: [u8; 4]) {
    unsafe {
        NET.cfg.dhcp_bound = false;
        NET.cfg.ip = ip;
        NET.cfg.mask = mask;
        NET.cfg.gateway = gateway;
        NET.cfg.dns = dns;
        NET.cfg.server_id = [0, 0, 0, 0];
        NET.cfg.lease_seconds = 0;
        NET.arp_valid = false;
    }
}

pub fn net_scan() -> NetScanResult {
    // Be helpful: try init if not already
    init();

    unsafe {
        let mut n = 0usize;

        if NET.rtl.is_none() {
            n += write_line_from_buf(n, b"rtl8139: not found");
            return NetScanResult { devices: &SCAN_STRS[..n] };
        }

        // line 0: mac
        {
            let mut buf = [0u8; 96];
            let mut k = 0usize;
            k += copy_bytes(&mut buf[k..], b"rtl8139: up  mac=");
            k += write_mac(&mut buf[k..], NET.cfg.mac);
            n += write_line_from_buf(n, &buf[..k]);
        }

        // line 1: ipv4
        {
            let mut buf = [0u8; 96];
            let mut k = 0usize;
            k += copy_bytes(&mut buf[k..], b"ipv4: ");
            k += write_ipv4(&mut buf[k..], NET.cfg.ip);
            k += copy_bytes(&mut buf[k..], if NET.cfg.dhcp_bound { b" (dhcp)" } else { b" (static/none)" });
            n += write_line_from_buf(n, &buf[..k]);
        }

        // line 2: stats
        {
            let mut buf = [0u8; 96];
            let mut k = 0usize;
            k += copy_bytes(&mut buf[k..], b"stats: rx=");
            k += write_u32_dec(&mut buf[k..], NET.stats.rx_packets);
            k += copy_bytes(&mut buf[k..], b" tx=");
            k += write_u32_dec(&mut buf[k..], NET.stats.tx_packets);
            k += copy_bytes(&mut buf[k..], b" drop=");
            k += write_u32_dec(&mut buf[k..], NET.stats.rx_dropped);
            n += write_line_from_buf(n, &buf[..k]);
        }

        NetScanResult { devices: &SCAN_STRS[..n] }
    }
}

// -----------------------------------------------------------------------------
// DHCP (blocking, polled)
// -----------------------------------------------------------------------------

pub fn dhcp_acquire() -> Result<(), DhcpError> {
    init();

    unsafe {
        let rtl = match NET.rtl.as_mut() {
            Some(r) => r,
            None => return Err(DhcpError::NoNic),
        };

        // clear prior
        NET.cfg.dhcp_bound = false;
        NET.cfg.ip = [0, 0, 0, 0];
        NET.cfg.mask = [0, 0, 0, 0];
        NET.cfg.gateway = [0, 0, 0, 0];
        NET.cfg.dns = [0, 0, 0, 0];
        NET.cfg.server_id = [0, 0, 0, 0];
        NET.cfg.lease_seconds = 0;

        let xid = (time::rdtsc() as u32) ^ 0xA5A5_1234;

        // DISCOVER
        send_dhcp(rtl, xid, 1, [0, 0, 0, 0], [0, 0, 0, 0])?;

        // OFFER
        let offer = wait_dhcp(rtl, xid, 2)?;
        let offered_ip = offer.yiaddr;
        let server = offer.server_id;

        // REQUEST
        send_dhcp(rtl, xid, 3, offered_ip, server)?;

        // ACK (or NACK)
        let ack = wait_dhcp(rtl, xid, 5)?;
        if ack.msg_type == 6 {
            return Err(DhcpError::Nack);
        }

        NET.cfg.dhcp_bound = true;
        NET.cfg.ip = offered_ip;
        NET.cfg.mask = ack.subnet_mask;
        NET.cfg.gateway = ack.router;
        NET.cfg.dns = ack.dns1;
        NET.cfg.server_id = server;
        NET.cfg.lease_seconds = ack.lease_time;

        // clear ARP cache
        NET.arp_valid = false;

        Ok(())
    }
}

fn send_dhcp(
    rtl: &mut Rtl8139,
    xid: u32,
    msg_type: u8,
    req_ip: [u8; 4],
    server_id: [u8; 4],
) -> Result<(), DhcpError> {
    // build DHCP payload
    let mut dhcp = [0u8; 300];
    let dhcp_len = build_dhcp(&mut dhcp, rtl.mac, xid, msg_type, req_ip, server_id);

    // build UDP + IPv4
    let mut pkt = [0u8; 420];
    let udp_len = 8 + dhcp_len;
    let ip_len = 20 + udp_len;

    // IPv4 header
    pkt[0] = 0x45;
    pkt[1] = 0;
    pkt[2..4].copy_from_slice(&(ip_len as u16).to_be_bytes());
    pkt[4..6].copy_from_slice(&0u16.to_be_bytes());
    pkt[6..8].copy_from_slice(&0u16.to_be_bytes());
    pkt[8] = 64;
    pkt[9] = 17; // UDP
    pkt[10..12].copy_from_slice(&0u16.to_be_bytes());
    pkt[12..16].copy_from_slice(&[0, 0, 0, 0]); // src 0.0.0.0
    pkt[16..20].copy_from_slice(&[255, 255, 255, 255]); // dst broadcast
    let csum = checksum16(&pkt[0..20]);
    pkt[10..12].copy_from_slice(&csum.to_be_bytes());

    // UDP header
    let u = 20;
    pkt[u + 0..u + 2].copy_from_slice(&68u16.to_be_bytes());
    pkt[u + 2..u + 4].copy_from_slice(&67u16.to_be_bytes());
    pkt[u + 4..u + 6].copy_from_slice(&(udp_len as u16).to_be_bytes());
    pkt[u + 6..u + 8].copy_from_slice(&0u16.to_be_bytes()); // UDP checksum 0 OK for IPv4

    // DHCP
    pkt[u + 8..u + 8 + dhcp_len].copy_from_slice(&dhcp[..dhcp_len]);

    let ok = rtl.send_frame([0xFF; 6], 0x0800, &pkt[..ip_len]);
    if ok { Ok(()) } else { Err(DhcpError::Malformed) }
}

#[derive(Copy, Clone)]
struct DhcpParsed {
    msg_type: u8,
    yiaddr: [u8; 4],
    subnet_mask: [u8; 4],
    router: [u8; 4],
    dns1: [u8; 4],
    server_id: [u8; 4],
    lease_time: u32,
}

fn wait_dhcp(rtl: &mut Rtl8139, xid: u32, want_type: u8) -> Result<DhcpParsed, DhcpError> {
    let mut spins: u32 = 0;
    while spins < 12_000_000 {
        if let Some(frame) = rtl.poll_recv() {
            if let Some(p) = parse_dhcp_frame(frame, xid) {
                if p.msg_type == want_type || (want_type == 5 && p.msg_type == 6) {
                    return Ok(p);
                }
            }
        }
        spins = spins.wrapping_add(1);
        if (spins & 0x3FF) == 0 { time::cpu_pause(); }
    }
    Err(DhcpError::Timeout)
}

fn parse_dhcp_frame(frame: &[u8], xid: u32) -> Option<DhcpParsed> {
    // Ethernet(14) + IPv4(20+) + UDP(8) + DHCP(240+)
    if frame.len() < 14 + 20 + 8 + 240 { return None; }

    // VLAN-aware L2 header
    let mut l2 = 14usize;
    let mut ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    if ethertype == 0x8100 && frame.len() >= 18 {
        ethertype = u16::from_be_bytes([frame[16], frame[17]]);
        l2 = 18;
    }
    if ethertype != 0x0800 { return None; }

    let ip = l2;
    let ver_ihl = frame[ip];
    if (ver_ihl >> 4) != 4 { return None; }
    let ihl = (ver_ihl & 0x0F) as usize * 4;
    if frame.len() < l2 + ihl + 8 + 240 { return None; }
    if frame[ip + 9] != 17 { return None; }

    let udp = l2 + ihl;
    let src_port = u16::from_be_bytes([frame[udp], frame[udp + 1]]);
    let dst_port = u16::from_be_bytes([frame[udp + 2], frame[udp + 3]]);
    if !(src_port == 67 && dst_port == 68) { return None; }

    let dh = udp + 8;
    if frame.len() < dh + 240 { return None; }

    let got_xid = u32::from_be_bytes([frame[dh + 4], frame[dh + 5], frame[dh + 6], frame[dh + 7]]);
    if got_xid != xid { return None; }

    // magic cookie
    if frame[dh + 236..dh + 240] != [99, 130, 83, 99] { return None; }

    let mut out = DhcpParsed {
        msg_type: 0,
        yiaddr: [frame[dh + 16], frame[dh + 17], frame[dh + 18], frame[dh + 19]],
        subnet_mask: [0, 0, 0, 0],
        router: [0, 0, 0, 0],
        dns1: [0, 0, 0, 0],
        server_id: [0, 0, 0, 0],
        lease_time: 0,
    };

    let mut i = dh + 240;
    while i < frame.len() {
        let opt = frame[i];
        i += 1;
        if opt == 0 { continue; }
        if opt == 255 { break; }
        if i >= frame.len() { break; }
        let len = frame[i] as usize;
        i += 1;
        if i + len > frame.len() { break; }

        match opt {
            53 if len >= 1 => out.msg_type = frame[i],
            1 if len == 4 => out.subnet_mask.copy_from_slice(&frame[i..i + 4]),
            3 if len >= 4 => out.router.copy_from_slice(&frame[i..i + 4]),
            6 if len >= 4 => out.dns1.copy_from_slice(&frame[i..i + 4]),
            51 if len == 4 => out.lease_time = u32::from_be_bytes([frame[i], frame[i + 1], frame[i + 2], frame[i + 3]]),
            54 if len == 4 => out.server_id.copy_from_slice(&frame[i..i + 4]),
            _ => {}
        }
        i += len;
    }

    if out.msg_type == 0 { None } else { Some(out) }
}

fn build_dhcp(
    buf: &mut [u8],
    mac: [u8; 6],
    xid: u32,
    msg_type: u8,
    req_ip: [u8; 4],
    server_id: [u8; 4],
) -> usize {
    buf.fill(0);

    buf[0] = 1; // BOOTREQUEST
    buf[1] = 1; // ethernet
    buf[2] = 6; // mac len
    buf[3] = 0; // hops
    buf[4..8].copy_from_slice(&xid.to_be_bytes());
    buf[10..12].copy_from_slice(&0x8000u16.to_be_bytes()); // broadcast flag
    buf[28..34].copy_from_slice(&mac); // chaddr

    // magic cookie
    buf[236..240].copy_from_slice(&[99, 130, 83, 99]);

    let mut i = 240usize;

    // 53: message type
    i = push_opt(buf, i, 53, &[msg_type]);

    // 61: client identifier (type 1 + mac)
    let mut cid = [0u8; 7];
    cid[0] = 1;
    cid[1..7].copy_from_slice(&mac);
    i = push_opt(buf, i, 61, &cid);

    // 55: parameter request list (subnet, router, dns, lease, server id)
    i = push_opt(buf, i, 55, &[1, 3, 6, 51, 54]);

    // 12: hostname
    i = push_opt(buf, i, 12, b"othello");

    if msg_type == 3 {
        i = push_opt(buf, i, 50, &req_ip);     // requested ip
        i = push_opt(buf, i, 54, &server_id);  // server identifier
    }

    if i < buf.len() {
        buf[i] = 255;
        i += 1;
    }
    i
}

fn push_opt(buf: &mut [u8], mut i: usize, code: u8, data: &[u8]) -> usize {
    if i + 2 + data.len() > buf.len() { return i; }
    buf[i] = code; i += 1;
    buf[i] = data.len() as u8; i += 1;
    buf[i..i + data.len()].copy_from_slice(data);
    i + data.len()
}

// -----------------------------------------------------------------------------
// Ping (ARP + ICMP echo)
// -----------------------------------------------------------------------------

pub fn ping_once(dst_ip: [u8; 4], seq: u16) -> Result<PingReply, PingError> {
    init();

    let (src_ip, mask, gateway, mac) = unsafe {
        if NET.rtl.is_none() { return Err(PingError::NoNic); }
        if NET.cfg.ip == [0, 0, 0, 0] { return Err(PingError::NotConfigured); }
        (NET.cfg.ip, NET.cfg.mask, NET.cfg.gateway, NET.cfg.mac)
    };

    let next_hop = if same_subnet(src_ip, dst_ip, mask) || gateway == [0, 0, 0, 0] {
        dst_ip
    } else {
        gateway
    };

    // Resolve next-hop MAC
    let nh_mac = arp_resolve(next_hop, mac, src_ip)?;

    // Build ICMP echo request
    let ident: u16 = 0x4F54; // 'OT'
    let mut icmp = [0u8; 8 + 32];
    icmp[0] = 8; // type = echo request
    icmp[1] = 0; // code
    icmp[2..4].copy_from_slice(&0u16.to_be_bytes()); // checksum placeholder
    icmp[4..6].copy_from_slice(&ident.to_be_bytes());
    icmp[6..8].copy_from_slice(&seq.to_be_bytes());
    for i in 0..32 { icmp[8 + i] = i as u8; }
    let csum = checksum16(&icmp);
    icmp[2..4].copy_from_slice(&csum.to_be_bytes());

    // Build IPv4 packet
    let ip_len = 20 + icmp.len();
    let mut ip = [0u8; 20 + 8 + 32];
    ip[0] = 0x45;
    ip[1] = 0;
    ip[2..4].copy_from_slice(&(ip_len as u16).to_be_bytes());
    ip[4..6].copy_from_slice(&seq.to_be_bytes()); // id
    ip[6..8].copy_from_slice(&0u16.to_be_bytes()); // flags/frag
    ip[8] = 64; // ttl
    ip[9] = 1;  // ICMP
    ip[10..12].copy_from_slice(&0u16.to_be_bytes());
    ip[12..16].copy_from_slice(&src_ip);
    ip[16..20].copy_from_slice(&dst_ip);
    let ip_csum = checksum16(&ip[0..20]);
    ip[10..12].copy_from_slice(&ip_csum.to_be_bytes());
    ip[20..20 + icmp.len()].copy_from_slice(&icmp);

    // Send Ethernet frame
    let ok = unsafe { NET.rtl.as_mut().unwrap().send_frame(nh_mac, 0x0800, &ip[..ip_len]) };
    if !ok { return Err(PingError::TxFail); }

    let start = time::rdtsc();

    // Wait for echo reply
    let mut spins: u32 = 0;
    while spins < 12_000_000 {
        let frame = unsafe { NET.rtl.as_mut().unwrap().poll_recv() };
        if let Some(f) = frame {
            if let Some((ttl, got_seq)) = parse_icmp_echo_reply(f, src_ip, dst_ip, ident) {
                if got_seq == seq {
                    let end = time::rdtsc();
                    return Ok(PingReply { seq, ttl, rtt_tsc: end.wrapping_sub(start) });
                }
            }

            // harvest ARP replies while we're here
            if let Some((sip, sha)) = parse_arp_reply_for_us(f, src_ip) {
                unsafe {
                    NET.arp_valid = true;
                    NET.arp_ip = sip;
                    NET.arp_mac = sha;
                }
            }
        }
        spins = spins.wrapping_add(1);
        if (spins & 0x3FF) == 0 { time::cpu_pause(); }
    }

    Err(PingError::Timeout)
}

fn same_subnet(a: [u8; 4], b: [u8; 4], m: [u8; 4]) -> bool {
    ((a[0] & m[0]) == (b[0] & m[0])) &&
    ((a[1] & m[1]) == (b[1] & m[1])) &&
    ((a[2] & m[2]) == (b[2] & m[2])) &&
    ((a[3] & m[3]) == (b[3] & m[3]))
}

fn arp_resolve(target_ip: [u8; 4], our_mac: [u8; 6], our_ip: [u8; 4]) -> Result<[u8; 6], PingError> {
    unsafe {
        if NET.arp_valid && NET.arp_ip == target_ip {
            return Ok(NET.arp_mac);
        }
    }

    // Build ARP request
    let mut arp = [0u8; 28];
    arp[0..2].copy_from_slice(&1u16.to_be_bytes());      // htype ethernet
    arp[2..4].copy_from_slice(&0x0800u16.to_be_bytes()); // ptype ipv4
    arp[4] = 6; // hlen
    arp[5] = 4; // plen
    arp[6..8].copy_from_slice(&1u16.to_be_bytes());      // opcode request
    arp[8..14].copy_from_slice(&our_mac);
    arp[14..18].copy_from_slice(&our_ip);
    arp[18..24].copy_from_slice(&[0u8; 6]);              // target mac
    arp[24..28].copy_from_slice(&target_ip);

    let ok = unsafe { NET.rtl.as_mut().unwrap().send_frame([0xFF; 6], 0x0806, &arp) };
    if !ok { return Err(PingError::TxFail); }

    // Wait for ARP reply
    let mut spins: u32 = 0;
    while spins < 6_000_000 {
        if let Some(f) = unsafe { NET.rtl.as_mut().unwrap().poll_recv() } {
            if let Some((sip, sha)) = parse_arp_reply_for_us(f, our_ip) {
                if sip == target_ip {
                    unsafe {
                        NET.arp_valid = true;
                        NET.arp_ip = sip;
                        NET.arp_mac = sha;
                    }
                    return Ok(sha);
                }
            }
        }
        spins = spins.wrapping_add(1);
        if (spins & 0x3FF) == 0 { time::cpu_pause(); }
    }

    Err(PingError::ArpTimeout)
}

fn parse_arp_reply_for_us(frame: &[u8], our_ip: [u8; 4]) -> Option<([u8; 4], [u8; 6])> {
    if frame.len() < 14 + 28 { return None; }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    if ethertype != 0x0806 { return None; }

    let a = 14;
    let opcode = u16::from_be_bytes([frame[a + 6], frame[a + 7]]);
    if opcode != 2 { return None; }

    let sha = [frame[a + 8], frame[a + 9], frame[a + 10], frame[a + 11], frame[a + 12], frame[a + 13]];
    let spa = [frame[a + 14], frame[a + 15], frame[a + 16], frame[a + 17]];
    let tpa = [frame[a + 24], frame[a + 25], frame[a + 26], frame[a + 27]];

    if tpa != our_ip { return None; }
    Some((spa, sha))
}

fn parse_icmp_echo_reply(frame: &[u8], our_ip: [u8; 4], dst_ip: [u8; 4], ident: u16) -> Option<(u8, u16)> {
    if frame.len() < 14 + 20 + 8 { return None; }

    let mut l2 = 14usize;
    let mut ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    if ethertype == 0x8100 && frame.len() >= 18 {
        ethertype = u16::from_be_bytes([frame[16], frame[17]]);
        l2 = 18;
    }
    if ethertype != 0x0800 { return None; }

    let ip = l2;
    let ver_ihl = frame[ip];
    if (ver_ihl >> 4) != 4 { return None; }
    let ihl = (ver_ihl & 0x0F) as usize * 4;
    if frame.len() < l2 + ihl + 8 { return None; }
    if frame[ip + 9] != 1 { return None; } // ICMP

    let ttl = frame[ip + 8];
    let src = [frame[ip + 12], frame[ip + 13], frame[ip + 14], frame[ip + 15]];
    let dst = [frame[ip + 16], frame[ip + 17], frame[ip + 18], frame[ip + 19]];
    if dst != our_ip { return None; }
    if src != dst_ip { return None; }

    let icmp = l2 + ihl;
    let typ = frame[icmp];
    let code = frame[icmp + 1];
    if typ != 0 || code != 0 { return None; } // echo reply

    let got_ident = u16::from_be_bytes([frame[icmp + 4], frame[icmp + 5]]);
    if got_ident != ident { return None; }

    let seq = u16::from_be_bytes([frame[icmp + 6], frame[icmp + 7]]);
    Some((ttl, seq))
}

// -----------------------------------------------------------------------------
// PCI helper
// -----------------------------------------------------------------------------

fn pci_cfg_addr(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | ((off as u32) & 0xFC)
}

fn pci_read_u32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    unsafe {
        outl(PCI_CONFIG_ADDR, pci_cfg_addr(bus, dev, func, off));
        inl(PCI_CONFIG_DATA)
    }
}

fn pci_write_u32(bus: u8, dev: u8, func: u8, off: u8, val: u32) {
    unsafe {
        outl(PCI_CONFIG_ADDR, pci_cfg_addr(bus, dev, func, off));
        outl(PCI_CONFIG_DATA, val);
    }
}

fn pci_find_rtl8139_io() -> Option<u16> {
    for bus in 0u8..=255 {
        for dev in 0u8..32 {
            for func in 0u8..8 {
                let vd = pci_read_u32(bus, dev, func, 0x00);
                let vendor = (vd & 0xFFFF) as u16;
                if vendor == 0xFFFF { continue; }
                let device = (vd >> 16) as u16;
                if vendor == RTL_VENDOR_ID && device == RTL_DEVICE_ID {
                    // BAR0
                    let bar0 = pci_read_u32(bus, dev, func, 0x10);
                    if (bar0 & 0x1) == 0 { continue; } // not I/O bar
                    let io = (bar0 & 0xFFFC) as u16;

                    // enable I/O + bus master
                    let cmdsts = pci_read_u32(bus, dev, func, 0x04);
                    let cmd = (cmdsts & 0xFFFF) as u16;
                    let sts = (cmdsts >> 16) as u16;

                    let new_cmd = cmd | 0x0001 | 0x0004;
                    let new_val = ((sts as u32) << 16) | (new_cmd as u32);
                    pci_write_u32(bus, dev, func, 0x04, new_val);

                    return Some(io);
                }
            }
        }
    }
    None
}

// -----------------------------------------------------------------------------
// Small utilities (no alloc)
// -----------------------------------------------------------------------------

fn checksum16(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0usize;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() { sum += (data[i] as u32) << 8; }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

unsafe fn write_line_from_buf(slot: usize, s: &[u8]) -> usize {
    if slot >= SCAN_BUFS.len() { return 0; }
    let len = s.len().min(SCAN_BUFS[slot].len());
    SCAN_BUFS[slot][..len].copy_from_slice(&s[..len]);
    let st = str::from_utf8_unchecked(&SCAN_BUFS[slot][..len]);
    SCAN_STRS[slot] = st;
    1
}

fn copy_bytes(dst: &mut [u8], src: &[u8]) -> usize {
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);
    n
}

fn write_u8_dec(out: &mut [u8], v: u8) -> usize {
    if out.is_empty() { return 0; }
    if v >= 100 {
        if out.len() < 3 { return 0; }
        out[0] = b'0' + (v / 100);
        out[1] = b'0' + ((v / 10) % 10);
        out[2] = b'0' + (v % 10);
        3
    } else if v >= 10 {
        if out.len() < 2 { return 0; }
        out[0] = b'0' + (v / 10);
        out[1] = b'0' + (v % 10);
        2
    } else {
        out[0] = b'0' + v;
        1
    }
}

fn write_u32_dec(out: &mut [u8], mut v: u32) -> usize {
    let mut tmp = [0u8; 10];
    let mut n = 0usize;
    if v == 0 {
        if !out.is_empty() { out[0] = b'0'; return 1; }
        return 0;
    }
    while v > 0 && n < tmp.len() {
        tmp[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    let mut w = 0usize;
    while n > 0 && w < out.len() {
        n -= 1;
        out[w] = tmp[n];
        w += 1;
    }
    w
}

fn write_ipv4(out: &mut [u8], ip: [u8; 4]) -> usize {
    let mut n = 0usize;
    n += write_u8_dec(&mut out[n..], ip[0]);
    if n < out.len() { out[n] = b'.'; n += 1; }
    n += write_u8_dec(&mut out[n..], ip[1]);
    if n < out.len() { out[n] = b'.'; n += 1; }
    n += write_u8_dec(&mut out[n..], ip[2]);
    if n < out.len() { out[n] = b'.'; n += 1; }
    n += write_u8_dec(&mut out[n..], ip[3]);
    n
}

fn hex(b: u8) -> u8 {
    match b & 0xF {
        0..=9 => b'0' + (b & 0xF),
        _ => b'a' + ((b & 0xF) - 10),
    }
}

fn write_mac(out: &mut [u8], mac: [u8; 6]) -> usize {
    let mut n = 0usize;
    for i in 0..6 {
        if n + 2 > out.len() { break; }
        out[n] = hex(mac[i] >> 4); n += 1;
        out[n] = hex(mac[i]); n += 1;
        if i != 5 {
            if n < out.len() { out[n] = b'-'; n += 1; }
        }
    }
    n
}
