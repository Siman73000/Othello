use crate::window::GuiEvent;
use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};
use x86_64::instructions::hlt;
use x86_64::instructions::port::Port;

const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_CLASS_NETWORK: u8 = 0x02;
const PCI_SUBCLASS_ETHERNET: u8 = 0x00;
const PCI_SUBCLASS_WIRELESS: u8 = 0x80; // 802.11-compatible wireless controllers
const MAX_WIFI_SSIDS: usize = 8;
const MAX_WIFI_SSID_LEN: usize = 32;

const RTL_VENDOR_ID: u16 = 0x10ec;
const RTL_DEVICE_ID: u16 = 0x8139;
const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;

/// RTL8139 register offsets
const IDR0: u16 = 0x00; // MAC 6 bytes: 0x00..0x05
const RBSTART: u16 = 0x30; // 32-bit RBSTART register
const CR: u16 = 0x37; // Command register (8-bit)
const CAPR: u16 = 0x38; // Current address pointer (16-bit)
const IMR: u16 = 0x3C; // Interrupt mask (16-bit)
const ISR: u16 = 0x3E; // Interrupt status (16-bit)
const RCR: u16 = 0x44; // Receive config (32-bit)
const TSD0: u16 = 0x10; // Transmit status 0 (32-bit)
const TSAD0: u16 = 0x20; // Transmit start addr 0 (32-bit)
const CONFIG1: u16 = 0x52; // Config1 (8-bit) - optional
const CBR: u16 = 0x3A; // Current buffer address (16-bit)

// command register bits
const CR_RESET: u8 = 0x10;
const CR_RX_ENABLE: u8 = 0x08;
const CR_TX_ENABLE: u8 = 0x04;

const ISR_RX_OK: u16 = 0x01;
const ISR_RX_ERR: u16 = 0x02;
const ISR_TX_OK: u16 = 0x04;
const ISR_TX_ERR: u16 = 0x08;
const ISR_RX_OVERFLOW: u16 = 0x10;

const RCR_ACCEPT_BROADCAST: u32 = 1 << 3;
const RCR_ACCEPT_PHYS_MATCH: u32 = 1 << 1;
const RCR_ACCEPT_MULTICAST: u32 = 1 << 2;
const RCR_ACCEPT_ALL: u32 = 1 << 0;
const RCR_WRAP: u32 = 1 << 7;
const RCR_MXDMA_UNLIMITED: u32 = 0b111 << 8;

const TCR: u16 = 0x40; // Transmit configuration
const TCR_IFG_STD: u32 = 0b11; // 96-bit interframe gap
const TCR_MXDMA_UNLIMITED: u32 = 0b111 << 8;

// Receive status bits
const RX_STATUS_OK: u16 = 1 << 0;
const RX_STATUS_FAE: u16 = 1 << 1;
const RX_STATUS_CRC: u16 = 1 << 2;
const RX_STATUS_LONG: u16 = 1 << 3;
const RX_STATUS_RUNT: u16 = 1 << 4;

const RX_BUFFER_SIZE: usize = 8192 + 16 + 1500;
const MAX_PACKETS: usize = 64;
const MAX_PACKET_SIZE: usize = 1536;
const NUM_TX_DESC: usize = 4;

const RX_BASE_BUDGET: usize = 8;
const RX_BURST_BUDGET: usize = 32;
const RX_MAX_BUDGET: usize = 64;
const RX_BACKLOG_HIGH_WATER: usize = 2048;
const RX_HIGH_FLOW_STREAK_TRIGGER: u32 = 8;
const FAULT_RESET_THRESHOLD: u32 = 64;
const QUEUE_PRESSURE_THRESHOLD: usize = MAX_PACKETS - 4;
const BROADCAST_BUDGET_PER_IRQ: usize = 4;

// Guardrails for suspicious hardware state.
const MAX_VALID_CBR: usize = RX_BUFFER_SIZE - 4;
const ALLOWED_ETHERTYPES: [u16; 3] = [0x0800, 0x0806, 0x86DD]; // IPv4, ARP, IPv6

const TX_WAIT_LIMIT: usize = 200_000;
const PCI_SCAN_TIMEOUT: usize = 1000;
const WIFI_RECON_THRESHOLD: u32 = 4;
const WIFI_HARD_DROP_THRESHOLD: u32 = 32;
const NETWORK_POLL_WARMUP: usize = 256;

/// static DMA buffers (must be physically contiguous and DMA-accessible)
/// place in physical memory
/// accessible by the NIC (identity-mapped or reserved physical memory)
#[link_section = ".nic_rx_buffer"]
static mut RX_BUFFER: [u8; RX_BUFFER_SIZE] = [0u8; RX_BUFFER_SIZE];

#[link_section = ".nic_tx_buffers"]
static mut TX_BUFFERS: [[u8; MAX_PACKET_SIZE]; NUM_TX_DESC] = [[0u8; MAX_PACKET_SIZE]; NUM_TX_DESC];

struct PacketQueue {
    storage: [[u8; MAX_PACKET_SIZE]; MAX_PACKETS],
    lengths: [usize; MAX_PACKETS],
    head: usize,
    tail: usize,
}

impl PacketQueue {
    const fn new() -> Self {
        Self {
            storage: [[0u8; MAX_PACKET_SIZE]; MAX_PACKETS],
            lengths: [0usize; MAX_PACKETS],
            head: 0,
            tail: 0,
        }
    }

    fn clear(&mut self) {
        for slot in self.storage.iter_mut() {
            slot.fill(0);
        }
        self.lengths = [0usize; MAX_PACKETS];
        self.head = 0;
        self.tail = 0;
    }

    fn push(&mut self, data: &[u8]) {
        let next = (self.tail + 1) % MAX_PACKETS;
        if next == self.head {
            // queue full - drop oldest
            self.head = (self.head + 1) % MAX_PACKETS;
        }
        let slot = self.tail;
        let len = data.len().min(MAX_PACKET_SIZE);
        self.storage[slot][..len].copy_from_slice(&data[..len]);
        self.lengths[slot] = len;
        self.tail = next;
    }

    fn pop(&mut self) -> Option<&[u8]> {
        if self.head == self.tail {
            return None;
        }
        let slot = self.head;
        let len = self.lengths[slot];
        let slice = &self.storage[slot][..len];
        self.head = (self.head + 1) % MAX_PACKETS;
        Some(slice)
    }

    fn len(&self) -> usize {
        if self.tail >= self.head {
            self.tail - self.head
        } else {
            (MAX_PACKETS - self.head) + self.tail
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rtl8139Stats {
    pub rx_overflows: u32,
    pub rx_errors: u32,
    pub rx_drops: u32,
    pub rx_policy_drops: u32,
    pub rx_spoof_drops: u32,
    pub tx_errors: u32,
    pub queued_packets: usize,
    pub fault_resets: u32,
    pub tamper_events: u32,
    pub high_flow_events: u32,
    pub register_tamper_events: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WifiBand {
    Band2G4,
    Band5G,
}

#[derive(Clone, Copy, Debug)]
struct SsidEntry {
    name: [u8; MAX_WIFI_SSID_LEN],
    len: usize,
    channel: u8,
    band: WifiBand,
    secure: bool,
}

impl Default for SsidEntry {
    fn default() -> Self {
        Self {
            name: [0u8; MAX_WIFI_SSID_LEN],
            len: 0,
            channel: 0,
            band: WifiBand::Band2G4,
            secure: false,
        }
    }
}

impl SsidEntry {
    fn new(name: &[u8], channel: u8, band: WifiBand, secure: bool) -> Self {
        let mut entry = Self::default();
        let len = name.len().min(MAX_WIFI_SSID_LEN);
        entry.name[..len].copy_from_slice(&name[..len]);
        entry.len = len;
        entry.channel = channel;
        entry.band = band;
        entry.secure = secure;
        entry
    }

    fn ssid(&self) -> &[u8] {
        &self.name[..self.len]
    }
}

#[derive(Clone, Copy)]
struct WirelessSecurityProfile {
    allowed: [[u8; MAX_WIFI_SSID_LEN]; MAX_WIFI_SSIDS],
    allowed_lens: [usize; MAX_WIFI_SSIDS],
    denylist_hits: u32,
}

impl WirelessSecurityProfile {
    fn new() -> Self {
        Self {
            allowed: [[0u8; MAX_WIFI_SSID_LEN]; MAX_WIFI_SSIDS],
            allowed_lens: [0usize; MAX_WIFI_SSIDS],
            denylist_hits: 0,
        }
    }

    fn add_allowed(&mut self, ssid: &[u8]) {
        let mut slot = None;
        for i in 0..MAX_WIFI_SSIDS {
            if self.allowed_lens[i] == 0 {
                slot = Some(i);
                break;
            }
        }
        if let Some(idx) = slot {
            let len = ssid.len().min(MAX_WIFI_SSID_LEN);
            self.allowed[idx][..len].copy_from_slice(&ssid[..len]);
            self.allowed_lens[idx] = len;
        }
    }

    fn is_allowed(&self, ssid: &[u8]) -> bool {
        for i in 0..MAX_WIFI_SSIDS {
            let len = self.allowed_lens[i];
            if len == 0 {
                continue;
            }
            if &self.allowed[i][..len] == ssid {
                return true;
            }
        }
        false
    }
}

struct WirelessController {
    device: PciDeviceInfo,
    policy: WirelessSecurityProfile,
    scan_results: [Option<SsidEntry>; MAX_WIFI_SSIDS],
    pending_intrusion_events: u32,
    connection_attempts: u32,
    connected: Option<SsidEntry>,
}

impl WirelessController {
    fn new(device: PciDeviceInfo) -> Self {
        let mut policy = WirelessSecurityProfile::new();
        // Whitelist can be tightened at runtime; start with a hardened empty set.
        policy.add_allowed(b"secured-2g");
        policy.add_allowed(b"secured-5g");

        Self {
            device,
            policy,
            scan_results: [None; MAX_WIFI_SSIDS],
            pending_intrusion_events: 0,
            connection_attempts: 0,
            connected: None,
        }
    }

    fn clear_scan_results(&mut self) {
        for slot in self.scan_results.iter_mut() {
            *slot = None;
        }
    }

    fn collect_passive_beacons(&self) -> [Option<SsidEntry>; MAX_WIFI_SSIDS] {
        // This routine can be replaced with hardware-backed scanning; for now we
        // surface hardened defaults while preserving band separation.
        let mut results: [Option<SsidEntry>; MAX_WIFI_SSIDS] = [None; MAX_WIFI_SSIDS];
        results[0] = Some(SsidEntry::new(b"secured-2g", 6, WifiBand::Band2G4, true));
        results[1] = Some(SsidEntry::new(b"secured-5g", 36, WifiBand::Band5G, true));
        results
    }

    fn perform_secure_scan(&mut self) {
        self.clear_scan_results();
        let fresh = self.collect_passive_beacons();
        let mut idx = 0;
        for candidate in fresh.into_iter().flatten() {
            if idx >= MAX_WIFI_SSIDS {
                break;
            }
            if !candidate.secure {
                self.pending_intrusion_events = self.pending_intrusion_events.wrapping_add(1);
                continue;
            }
            self.scan_results[idx] = Some(candidate);
            idx += 1;
        }
    }

    fn connect_to_allowed(&mut self) {
        for entry in self.scan_results.iter().flatten() {
            if self.policy.is_allowed(entry.ssid()) {
                self.connected = Some(*entry);
                return;
            }
        }
        self.connected = None;
    }

    fn guard_against_unapproved(&mut self) {
        for entry in self.scan_results.iter().flatten() {
            if !self.policy.is_allowed(entry.ssid()) {
                self.policy.denylist_hits = self.policy.denylist_hits.wrapping_add(1);
                self.pending_intrusion_events = self.pending_intrusion_events.wrapping_add(1);
            }
        }
    }

    fn poll_wireless(&mut self) {
        self.perform_secure_scan();
        self.guard_against_unapproved();
        if self.pending_intrusion_events >= WIFI_HARD_DROP_THRESHOLD {
            // Too many hostile beacons; quarantine device.
            pci_quarantine_device(self.device.bus, self.device.device, self.device.func);
            self.connected = None;
            return;
        }

        if self.pending_intrusion_events >= WIFI_RECON_THRESHOLD {
            // Back off associations while hostile activity is present.
            self.connected = None;
            return;
        }

        self.connection_attempts = self.connection_attempts.wrapping_add(1);
        self.connect_to_allowed();
    }
}

pub struct Rtl8139 {
    io_base: u16,
    rx_buffer_phys: u32,
    // RX ring buffer pointer already in static RX_BUFFER
    rx_offset: usize,
    mac: [u8; 6],
    packet_queue: PacketQueue,
    // diagnostics
    rx_ovf_count: u32,
    rx_err_count: u32,
    rx_drop_count: u32,
    rx_policy_drop_count: u32,
    rx_spoof_drop_count: u32,
    tx_err_count: u32,
    tx_lengths: [usize; NUM_TX_DESC],
    fault_streak: u32,
    fault_reset_count: u32,
    last_cbr: usize,
    tamper_events: u32,
    high_flow_streak: u32,
    high_flow_events: u32,
    register_tamper_events: u32,
    rcr_shadow: u32,
}

impl Rtl8139 {
    /// rx_buffer_phys must be the physical address of RX_BUFFER
    /// assumes identity mapping and uses the address of `RX_BUFFER` as physical
    pub fn new(io_base: u16, rx_buffer_phys: u32) -> Self {
        let mut dev = Self {
            io_base,
            rx_buffer_phys,
            rx_offset: 0,
            mac: [0u8; 6],
            packet_queue: PacketQueue::new(),
            rx_ovf_count: 0,
            rx_err_count: 0,
            rx_drop_count: 0,
            rx_policy_drop_count: 0,
            rx_spoof_drop_count: 0,
            tx_err_count: 0,
            tx_lengths: [0usize; NUM_TX_DESC],
            fault_streak: 0,
            fault_reset_count: 0,
            last_cbr: 0,
            tamper_events: 0,
            high_flow_streak: 0,
            high_flow_events: 0,
            register_tamper_events: 0,
            rcr_shadow: 0,
        };

        dev.reset();
        dev.read_mac();
        dev.write_reg32(RBSTART, rx_buffer_phys);
        dev.scrub_dma_buffers();
        dev.optimize_throughput();
        dev.configure_receive_filters(false, false);
        dev.write_reg8(CR, CR_RX_ENABLE | CR_TX_ENABLE);
        dev.write_reg16(
            IMR,
            ISR_RX_OK | ISR_RX_ERR | ISR_TX_OK | ISR_TX_ERR | ISR_RX_OVERFLOW,
        );
        dev.write_reg16(ISR, 0xffff);
        compiler_fence(Ordering::SeqCst);
        dev
    }

    /// helper fn to create Port<u8> at base+offset
    fn port8(&self, offset: u16) -> Port<u8> {
        Port::new(self.io_base + offset)
    }

    fn port16(&self, offset: u16) -> Port<u16> {
        Port::new(self.io_base + offset)
    }

    fn port32(&self, offset: u16) -> Port<u32> {
        Port::new(self.io_base + offset)
    }

    fn reset(&mut self) {
        let mut p = self.port8(CR);
        unsafe {
            p.write(CR_RESET);
        }
        // wait for reset to clear
        loop {
            let r = unsafe { p.read() };
            if r & CR_RESET == 0 {
                break;
            }
        }
    }

    fn read_mac(&mut self) {
        for i in 0..6 {
            self.mac[i] = unsafe { self.port8(IDR0 + i as u16).read() };
        }
    }

    /// low-level register helpers
    fn write_reg8(&self, reg: u16, val: u8) {
        unsafe {
            self.port8(reg).write(val);
        }
    }
    fn write_reg16(&self, reg: u16, val: u16) {
        unsafe {
            self.port16(reg).write(val);
        }
    }
    fn write_reg32(&self, reg: u16, val: u32) {
        unsafe {
            self.port32(reg).write(val);
        }
    }
    fn read_reg8(&self, reg: u16) -> u8 {
        unsafe { self.port8(reg).read() }
    }
    fn read_reg16(&self, reg: u16) -> u16 {
        unsafe { self.port16(reg).read() }
    }
    fn read_reg32(&self, reg: u16) -> u32 {
        unsafe { self.port32(reg).read() }
    }

    pub fn irq_handler(&mut self) {
        self.audit_register_integrity();
        let isr = self.read_reg16(ISR);
        if isr & ISR_RX_OK != 0 {
            self.handle_rx();
        }
        if isr & ISR_RX_OVERFLOW != 0 {
            self.handle_overflow();
        }
        if isr & ISR_TX_OK != 0 {
            self.scrub_completed_tx();
        }
        if isr & ISR_TX_ERR != 0 {
            self.tx_err_count = self.tx_err_count.wrapping_add(1);
        }
        if isr & ISR_RX_ERR != 0 {
            self.rx_err_count = self.rx_err_count.wrapping_add(1);
        }

        unsafe {
            self.port16(ISR).write(isr);
        }

        compiler_fence(Ordering::SeqCst);
    }

    /// Configure transmit and receive DMA burst sizes for better throughput.
    fn optimize_throughput(&mut self) {
        // Max DMA burst for both TX and RX; standard IFG for interoperability.
        self.write_reg32(TCR, TCR_IFG_STD | TCR_MXDMA_UNLIMITED);
        let rcr = RCR_ACCEPT_BROADCAST | RCR_ACCEPT_PHYS_MATCH | RCR_WRAP | RCR_MXDMA_UNLIMITED;
        self.rcr_shadow = rcr;
        self.write_reg32(RCR, rcr);
    }

    fn recover_from_fault(&mut self) {
        self.reset();
        self.rx_offset = 0;
        self.fault_streak = 0;
        self.fault_reset_count = self.fault_reset_count.wrapping_add(1);
        self.packet_queue.clear();
        self.scrub_dma_buffers();
        self.write_reg32(RBSTART, self.rx_buffer_phys);
        self.optimize_throughput();
        self.configure_receive_filters(false, false);
        self.write_reg8(CR, CR_RX_ENABLE | CR_TX_ENABLE);
        self.last_cbr = 0;
        self.set_rx_offset(0);
        self.write_reg16(
            IMR,
            ISR_RX_OK | ISR_RX_ERR | ISR_TX_OK | ISR_TX_ERR | ISR_RX_OVERFLOW,
        );
        self.write_reg16(ISR, 0xffff);
        compiler_fence(Ordering::SeqCst);
    }

    /// Clear DMA-visible buffers to avoid leaking stale data after reset or reuse.
    fn scrub_dma_buffers(&mut self) {
        unsafe {
            RX_BUFFER.fill(0);
            for buf in TX_BUFFERS.iter_mut() {
                buf.fill(0);
            }
        }
        self.tx_lengths = [0usize; NUM_TX_DESC];
    }

    fn validate_cbr(&self, cbr: usize) -> bool {
        if cbr > MAX_VALID_CBR || (cbr & 0x3) != 0 {
            return false;
        }
        // Detect impossible multi-wrap jumps that suggest pointer corruption.
        let forward = if cbr >= self.last_cbr {
            cbr - self.last_cbr
        } else {
            (RX_BUFFER_SIZE - self.last_cbr) + cbr
        };

        forward < RX_BUFFER_SIZE
    }

    fn estimate_backlog_bytes(&self, cbr: usize) -> usize {
        if cbr >= self.rx_offset {
            cbr - self.rx_offset
        } else {
            (RX_BUFFER_SIZE - self.rx_offset) + cbr
        }
    }

    fn compute_rx_budget(&mut self, cbr: usize) -> usize {
        let backlog = self.estimate_backlog_bytes(cbr);
        let queue_len = self.packet_queue.len();
        let high_flow =
            backlog >= RX_BACKLOG_HIGH_WATER || queue_len >= (QUEUE_PRESSURE_THRESHOLD / 2);

        if high_flow {
            if self.high_flow_streak == 0 {
                self.high_flow_events = self.high_flow_events.wrapping_add(1);
            }
            self.high_flow_streak = self.high_flow_streak.saturating_add(1);
            if self.high_flow_streak >= RX_HIGH_FLOW_STREAK_TRIGGER {
                return RX_MAX_BUDGET;
            }
            return RX_BURST_BUDGET;
        }

        self.high_flow_streak = 0;
        RX_BASE_BUDGET
    }

    fn flag_tamper(&mut self) {
        self.tamper_events = self.tamper_events.wrapping_add(1);
        self.fault_streak = self.fault_streak.saturating_add(1);
    }

    /// Restrict or extend receive filters for security/performance needs.
    pub fn configure_receive_filters(&mut self, allow_multicast: bool, promiscuous: bool) {
        let mut rcr = RCR_ACCEPT_BROADCAST | RCR_ACCEPT_PHYS_MATCH | RCR_WRAP | RCR_MXDMA_UNLIMITED;
        if allow_multicast {
            rcr |= RCR_ACCEPT_MULTICAST;
        }
        if promiscuous {
            // Harden against attempts to force promiscuous capture; ignore and flag.
            self.register_tamper_events = self.register_tamper_events.wrapping_add(1);
            self.tamper_events = self.tamper_events.wrapping_add(1);
        }
        self.rcr_shadow = rcr;
        self.write_reg32(RCR, rcr);
    }

    fn audit_register_integrity(&mut self) {
        let current_rcr = self.read_reg32(RCR);
        if current_rcr != self.rcr_shadow {
            self.register_tamper_events = self.register_tamper_events.wrapping_add(1);
            self.flag_tamper();
            self.write_reg32(RCR, self.rcr_shadow);
        }

        let current_rbstart = self.read_reg32(RBSTART);
        if current_rbstart != self.rx_buffer_phys {
            self.register_tamper_events = self.register_tamper_events.wrapping_add(1);
            self.flag_tamper();
            self.write_reg32(RBSTART, self.rx_buffer_phys);
        }
    }

    fn handle_rx(&mut self) {
        let rx_buf = unsafe { &RX_BUFFER };
        let mut processed = 0usize;
        let mut broadcast_budget = BROADCAST_BUDGET_PER_IRQ;
        let mut budget = RX_BASE_BUDGET;

        loop {
            let cbr = self.read_reg16(CBR) as usize;
            if !self.validate_cbr(cbr) {
                self.rx_err_count = self.rx_err_count.wrapping_add(1);
                self.rx_drop_count = self.rx_drop_count.wrapping_add(1);
                self.flag_tamper();
                self.recover_from_fault();
                break;
            }
            self.last_cbr = cbr;
            budget = self.compute_rx_budget(cbr);
            if self.rx_offset == cbr || processed >= budget {
                break;
            }

            // read packet header: 2 bytes status, 2 bytes length (little endian)
            let offset = self.rx_offset;
            // RTL8139 uses ring buffer; must ensure offset + 4 <= RX_BUFFER_SIZE
            if offset + 4 > RX_BUFFER_SIZE {
                // reset pointer defensively to avoid reading beyond DMA buffer
                self.rx_offset = 0;
                self.flag_tamper();
                if self.fault_streak >= FAULT_RESET_THRESHOLD {
                    self.recover_from_fault();
                }
                break;
            }

            let status = u16::from_le_bytes([rx_buf[offset], rx_buf[offset + 1]]);
            let length = u16::from_le_bytes([rx_buf[offset + 2], rx_buf[offset + 3]]) as usize;

            // sanity checks >:)
            if length == 0 || length > MAX_PACKET_SIZE || offset + 4 + length > RX_BUFFER_SIZE {
                self.rx_err_count = self.rx_err_count.wrapping_add(1);
                self.rx_drop_count = self.rx_drop_count.wrapping_add(1);
                self.flag_tamper();
                self.advance_capr(4);
                if self.fault_streak >= FAULT_RESET_THRESHOLD {
                    self.recover_from_fault();
                }
                break;
            }

            if status & RX_STATUS_OK != 0
                && status & (RX_STATUS_CRC | RX_STATUS_FAE | RX_STATUS_LONG | RX_STATUS_RUNT) == 0
            {
                let start = offset + 4;
                let end = start + length;
                let packet = &rx_buf[start..end];
                if length < 14 {
                    // Not enough for an Ethernet header; drop as malformed
                    self.rx_err_count = self.rx_err_count.wrapping_add(1);
                    self.rx_drop_count = self.rx_drop_count.wrapping_add(1);
                    self.flag_tamper();
                } else {
                    let dst = &packet[0..6];
                    let src = &packet[6..12];
                    let ethertype = u16::from_be_bytes([packet[12], packet[13]]);

                    let is_broadcast = dst == [0xffu8; 6];
                    let is_unicast_to_me = dst == self.mac;
                    let src_is_broadcast = src == [0xffu8; 6];
                    let src_is_zero = src == [0u8; 6];
                    let src_is_self = src == self.mac;

                    let allowed_type = ALLOWED_ETHERTYPES.contains(&ethertype);
                    let allowed_destination = is_unicast_to_me || is_broadcast;

                    let should_drop_for_policy = !allowed_type || !allowed_destination;
                    let should_drop_for_spoof = src_is_broadcast || src_is_zero || src_is_self;
                    let broadcast_budget_exhausted = is_broadcast && broadcast_budget == 0;

                    if broadcast_budget_exhausted {
                        self.rx_policy_drop_count = self.rx_policy_drop_count.wrapping_add(1);
                        self.flag_tamper();
                    } else if should_drop_for_spoof {
                        self.rx_spoof_drop_count = self.rx_spoof_drop_count.wrapping_add(1);
                        self.flag_tamper();
                    } else if should_drop_for_policy {
                        self.rx_policy_drop_count = self.rx_policy_drop_count.wrapping_add(1);
                        self.flag_tamper();
                    } else if self.packet_queue.len() >= QUEUE_PRESSURE_THRESHOLD {
                        // queue backpressure; drop and treat as suspicious to avoid unbounded memory retention
                        self.rx_drop_count = self.rx_drop_count.wrapping_add(1);
                        self.flag_tamper();
                        if self.fault_streak >= FAULT_RESET_THRESHOLD {
                            self.recover_from_fault();
                            break;
                        }
                    } else {
                        // copy to packet_queue
                        self.packet_queue.push(packet);
                        self.fault_streak = 0;
                        if is_broadcast && broadcast_budget > 0 {
                            broadcast_budget -= 1;
                        }
                    }
                }
            } else {
                self.rx_err_count = self.rx_err_count.wrapping_add(1);
                self.rx_drop_count = self.rx_drop_count.wrapping_add(1);
                self.flag_tamper();
                if self.fault_streak >= FAULT_RESET_THRESHOLD {
                    self.recover_from_fault();
                    break;
                }
            }

            // advance rx_offset by frame length + header (4) and align to dword
            let new_offset = (offset + 4 + length + 3) & !3;
            self.set_rx_offset(new_offset);

            processed += 1;
        }
    }

    /// advance CAPR by `advance` bytes
    fn advance_capr(&mut self, advance: usize) {
        self.rx_offset = (self.rx_offset + advance) % RX_BUFFER_SIZE;
        self.sync_rx_offset_to_hw();
    }

    fn set_rx_offset(&mut self, new_offset: usize) {
        self.rx_offset = new_offset % RX_BUFFER_SIZE;
        self.sync_rx_offset_to_hw();
    }

    fn sync_rx_offset_to_hw(&self) {
        // write CAPR = rx_offset - 16 (per RTL8139 doc) (16 is recommended headroom)
        let capr_val = if self.rx_offset >= 16 {
            (self.rx_offset - 16) as u16
        } else {
            0u16
        };
        self.write_reg16(CAPR, capr_val);
    }

    fn handle_overflow(&mut self) {
        self.rx_ovf_count = self.rx_ovf_count.wrapping_add(1);
        self.rx_offset = 0;
        self.fault_streak = self.fault_streak.saturating_add(1);
        self.recover_from_fault();
    }

    fn scrub_completed_tx(&mut self) {
        for i in 0..NUM_TX_DESC {
            let tsd_off = TSD0 + (i as u16) * 4;
            if self.read_reg32(tsd_off) == 0 && self.tx_lengths[i] != 0 {
                self.scrub_tx_slot(i);
            }
        }
    }

    fn scrub_tx_slot(&mut self, idx: usize) {
        let len = self.tx_lengths[idx].min(MAX_PACKET_SIZE);
        unsafe {
            TX_BUFFERS[idx][..len].fill(0);
        }
        self.tx_lengths[idx] = 0;
    }

    pub fn transmit(&mut self, data: &[u8]) -> Result<(), ()> {
        if data.len() > MAX_PACKET_SIZE {
            return Err(());
        }

        // find free TX descriptor index
        for i in 0..NUM_TX_DESC {
            let tsd_off = TSD0 + (i as u16) * 4;
            let tsd_val = self.read_reg32(tsd_off);
            // TSDn bit 0 (OWN) is typically the "transmitting / length" indicator;
            // if TSDn == 0x00, it's free behavior varies - adjust if needed
            if tsd_val == 0 {
                unsafe {
                    TX_BUFFERS[i][..data.len()].copy_from_slice(data);
                }
                let tx_phys = unsafe { &TX_BUFFERS[i] as *const _ as u32 };
                let tsad_off = TSAD0 + (i as u16) * 4;
                self.write_reg32(tsad_off, tx_phys);
                self.tx_lengths[i] = data.len();
                self.write_reg32(tsd_off, data.len() as u32);
                let mut wait = 0usize;
                loop {
                    let tsd_check = self.read_reg32(tsd_off);
                    if tsd_check == 0 {
                        self.scrub_tx_slot(i);
                        return Ok(());
                    }
                    wait += 1;
                    if wait > TX_WAIT_LIMIT {
                        self.tx_err_count = self.tx_err_count.wrapping_add(1);
                        self.scrub_tx_slot(i);
                        self.write_reg32(tsd_off, 0);
                        return Err(());
                    }
                }
            }
        }
        Err(())
    }
    /*
        fn read_reg32(&self, reg: u16) -> u32 {
            unsafe { self.port32(reg).read() }
        }

        fn write_reg32(&self, reg: u16, val: u32) {
            unsafe { self.port32(reg).write(val); }
        }
    */
    pub fn poll_dequeue(&mut self) -> Option<&[u8]> {
        self.packet_queue.pop()
    }

    pub fn pending_packets(&self) -> usize {
        self.packet_queue.len()
    }

    pub fn mac_address(&self) -> [u8; 6] {
        self.mac
    }

    pub fn stats(&self) -> Rtl8139Stats {
        Rtl8139Stats {
            rx_overflows: self.rx_ovf_count,
            rx_errors: self.rx_err_count,
            rx_drops: self.rx_drop_count,
            rx_policy_drops: self.rx_policy_drop_count,
            rx_spoof_drops: self.rx_spoof_drop_count,
            tx_errors: self.tx_err_count,
            queued_packets: self.packet_queue.len(),
            fault_resets: self.fault_reset_count,
            tamper_events: self.tamper_events,
            high_flow_events: self.high_flow_events,
            register_tamper_events: self.register_tamper_events,
        }
    }

    pub fn parse_ethernet_frame(packet: &[u8]) -> Option<([u8; 6], [u8; 6], u16)> {
        if packet.len() < 14 {
            return None;
        }
        let mut dst = [0u8; 6];
        let mut src = [0u8; 6];
        dst.copy_from_slice(&packet[0..6]);
        src.copy_from_slice(&packet[6..12]);
        let eth = u16::from_be_bytes([packet[12], packet[13]]);
        Some((dst, src, eth))
    }
}

static mut GLOBAL_RTL: *mut Rtl8139 = ptr::null_mut();
static mut RTL_INSTANCE: Option<Rtl8139> = None;
static mut WIRELESS_CONTROLLER: Option<WirelessController> = None;

#[no_mangle]
pub extern "C" fn nic_irq_entry() {
    unsafe {
        if !GLOBAL_RTL.is_null() {
            let dev = &mut *GLOBAL_RTL;
            dev.irq_handler();
        }
    }
}

#[no_mangle]
pub extern "C" fn network_scan() {
    let mut rtl: Option<Rtl8139> = None;
    let mut wireless: Option<WirelessController> = None;

    pci_for_each_network_device(|dev| {
        if dev.vendor == RTL_VENDOR_ID
            && dev.device_id == RTL_DEVICE_ID
            && rtl.is_none()
            && dev.subclass == PCI_SUBCLASS_ETHERNET
        {
            if let Some(io_base) = validate_io_bar(dev.bar0) {
                let rx_phys = unsafe { &RX_BUFFER as *const _ as u32 };
                let mut driver = Rtl8139::new(io_base, rx_phys);
                pci_enable_io_and_bus_master(dev.bus, dev.device, dev.func);
                rtl = Some(driver);
            }
        } else if dev.subclass == PCI_SUBCLASS_WIRELESS && wireless.is_none() {
            // Harden wireless controllers with a passive-only, whitelist-first policy.
            if validate_io_bar(dev.bar0).is_some() {
                wireless = Some(WirelessController::new(dev));
            }
        } else {
            // Unsupported NICs are quarantined to avoid accidental enablement or hostile DMA surfaces.
            pci_quarantine_device(dev.bus, dev.device, dev.func);
        }
    });

    if let Some(dev) = rtl {
        unsafe {
            RTL_INSTANCE = Some(dev);
            GLOBAL_RTL = RTL_INSTANCE
                .as_mut()
                .map(|driver| driver as *mut Rtl8139)
                .unwrap_or(ptr::null_mut());
        }
    }

    if let Some(ctrl) = wireless {
        unsafe {
            WIRELESS_CONTROLLER = Some(ctrl);
        }
    }

    if unsafe { RTL_INSTANCE.is_some() || WIRELESS_CONTROLLER.is_some() } {
        for _ in 0..NETWORK_POLL_WARMUP {
            unsafe {
                if let Some(ctrl) = WIRELESS_CONTROLLER.as_mut() {
                    ctrl.poll_wireless();
                }

                if let Some(dev) = RTL_INSTANCE.as_mut() {
                    while let Some(pkt) = dev.poll_dequeue() {
                        if let Some((dst, src, ethertype)) =
                            Rtl8139::parse_ethernet_frame(pkt)
                        {
                            let is_broadcast = dst == [0xffu8; 6];
                            let is_for_me = dst == dev.mac;
                            if is_broadcast || is_for_me {
                                let _ = (dst, src, ethertype);
                            }
                        }
                    }
                }
            }

            hlt();
        }
    }

    unsafe {
        if RTL_INSTANCE.is_none() {
            GLOBAL_RTL = ptr::null_mut();
        }
    }
}

pub fn poll_input_event() -> Option<GuiEvent> {
    unsafe {
        let mut status_port = Port::<u8>::new(PS2_STATUS_PORT);
        let status = status_port.read();

        if status & 0x01 == 0 {
            return None; // no data
        }

        let mut data_port = Port::<u8>::new(PS2_DATA_PORT);
        let scancode: u8 = data_port.read();

        // Ignore key releases (0x80+)
        if scancode & 0x80 != 0 {
            return None;
        }

        if let Some(key) = scancode_to_ascii(scancode) {
            return Some(GuiEvent::KeyPress { key });
        }
    }
    None
}

fn scancode_to_ascii(scancode: u8) -> Option<char> {
    match scancode {
        0x1E => Some('a'),
        0x30 => Some('b'),
        0x2E => Some('c'),
        0x20 => Some('d'),
        0x12 => Some('e'),
        0x21 => Some('f'),
        0x22 => Some('g'),
        0x23 => Some('h'),
        0x17 => Some('i'),
        0x24 => Some('j'),
        0x25 => Some('k'),
        0x26 => Some('l'),
        0x32 => Some('m'),
        0x31 => Some('n'),
        0x18 => Some('o'),
        0x19 => Some('p'),
        0x10 => Some('q'),
        0x13 => Some('r'),
        0x1F => Some('s'),
        0x14 => Some('t'),
        0x16 => Some('u'),
        0x2F => Some('v'),
        0x11 => Some('w'),
        0x2D => Some('x'),
        0x15 => Some('y'),
        0x2C => Some('z'),
        0x02 => Some('1'),
        0x03 => Some('2'),
        0x04 => Some('3'),
        0x05 => Some('4'),
        0x06 => Some('5'),
        0x07 => Some('6'),
        0x08 => Some('7'),
        0x09 => Some('8'),
        0x0A => Some('9'),
        0x0B => Some('0'),
        0x39 => Some(' '),
        0x0E => Some('\x08'), // backspace
        0x1C => Some('\n'),   // enter
        _ => None,
    }
}

fn pci_config_address(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let l = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((func as u32) << 8)
        | ((offset as u32) & 0xFC);
    l
}

fn pci_read_u8(bus: u8, device: u8, func: u8, offset: u8) -> u8 {
    let shift = (offset & 0x3) * 8;
    ((pci_read_u32(bus, device, func, offset) >> shift) & 0xFF) as u8
}

fn pci_read_u32(bus: u8, device: u8, func: u8, offset: u8) -> u32 {
    let addr = pci_config_address(bus, device, func, offset);
    unsafe {
        let mut p_addr = Port::<u32>::new(PCI_CONFIG_ADDR);
        let mut p_data = Port::<u32>::new(PCI_CONFIG_DATA);
        p_addr.write(addr);
        p_data.read()
    }
}

fn pci_write_u32(bus: u8, device: u8, func: u8, offset: u8, val: u32) {
    let addr = pci_config_address(bus, device, func, offset);
    unsafe {
        let mut p_addr = Port::<u32>::new(PCI_CONFIG_ADDR);
        let mut p_data = Port::<u32>::new(PCI_CONFIG_DATA);
        p_addr.write(addr);
        p_data.write(val);
    }
}

#[derive(Clone, Copy)]
struct PciDeviceInfo {
    bus: u8,
    device: u8,
    func: u8,
    vendor: u16,
    device_id: u16,
    class: u8,
    subclass: u8,
    bar0: u32,
    command: u16,
}

fn pci_for_each_network_device<F: FnMut(PciDeviceInfo)>(mut f: F) {
    for bus in 0u8..=255 {
        for device in 0u8..32 {
            for func in 0u8..8 {
                let vendor_device = pci_read_u32(bus, device, func, 0x00);
                let vendor = (vendor_device & 0xFFFF) as u16;
                if vendor == 0xFFFF {
                    continue;
                }
                let device_id = ((vendor_device >> 16) & 0xFFFF) as u16;
                let class = pci_read_u8(bus, device, func, 0x0B);
                if class != PCI_CLASS_NETWORK {
                    continue;
                }
                let subclass = pci_read_u8(bus, device, func, 0x0A);

                let bar0 = pci_read_u32(bus, device, func, 0x10);
                let command = pci_read_u32(bus, device, func, 0x04) as u16;
                f(PciDeviceInfo {
                    bus,
                    device,
                    func,
                    vendor,
                    device_id,
                    class,
                    subclass,
                    bar0,
                    command,
                });
            }
        }
    }
}

fn pci_quarantine_device(bus: u8, device: u8, func: u8) {
    let command = pci_read_u32(bus, device, func, 0x04) as u16;
    let masked = command & !(0x0007); // disable IO space, memory space, and bus master
    if masked != command {
        pci_write_u32(bus, device, func, 0x04, masked as u32);
    }
}

fn pci_enable_io_and_bus_master(bus: u8, device: u8, func: u8) {
    let command = pci_read_u32(bus, device, func, 0x04) as u16;
    let desired = command | 0x0001 | 0x0004; // IO space + bus master enable
    if desired != command {
        pci_write_u32(bus, device, func, 0x04, desired as u32);
    }
}

fn validate_io_bar(bar0: u32) -> Option<u16> {
    // IO BAR must have bit0 set; strip flags and clamp to 16-bit IO space
    if bar0 & 0x1 == 0 {
        return None;
    }
    let base = (bar0 & 0xFFFF_FFFC) as u16;
    if base == 0 {
        return None;
    }
    Some(base)
}
