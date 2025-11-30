use crate::window::GuiEvent;
use core::ptr;
use core::sync::atomic::{compiler_fence, Ordering};
use x86_64::instructions::hlt;
use x86_64::instructions::port::Port;

const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

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

const RX_BUFFER_SIZE: usize = 8192 + 16 + 1500;
const MAX_PACKETS: usize = 64;
const MAX_PACKET_SIZE: usize = 1536;
const NUM_TX_DESC: usize = 4;

const TX_WAIT_LIMIT: usize = 200_000;
const PCI_SCAN_TIMEOUT: usize = 1000;

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
    pub tx_errors: u32,
    pub queued_packets: usize,
}

pub struct Rtl8139 {
    io_base: u16,
    // RX ring buffer pointer already in static RX_BUFFER
    rx_offset: usize,
    mac: [u8; 6],
    packet_queue: PacketQueue,
    // diagnostics
    rx_ovf_count: u32,
    rx_err_count: u32,
    tx_err_count: u32,
}

impl Rtl8139 {
    /// rx_buffer_phys must be the physical address of RX_BUFFER
    /// assumes identity mapping and uses the address of `RX_BUFFER` as physical
    pub fn new(io_base: u16, rx_buffer_phys: u32) -> Self {
        let mut dev = Self {
            io_base,
            rx_offset: 0,
            mac: [0u8; 6],
            packet_queue: PacketQueue::new(),
            rx_ovf_count: 0,
            rx_err_count: 0,
            tx_err_count: 0,
        };

        dev.reset();
        dev.read_mac();
        dev.write_reg32(RBSTART, rx_buffer_phys);
        dev.write_reg32(
            RCR,
            RCR_ACCEPT_BROADCAST | RCR_ACCEPT_PHYS_MATCH | RCR_ACCEPT_MULTICAST | RCR_WRAP,
        );
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
        let isr = self.read_reg16(ISR);
        if isr & ISR_RX_OK != 0 {
            self.handle_rx();
        }
        if isr & ISR_RX_OVERFLOW != 0 {
            self.handle_overflow();
        }
        if isr & ISR_TX_OK != 0 {
            // TX complete - could clear counters or notify upper layers
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

    fn handle_rx(&mut self) {
        let rx_buf = unsafe { &RX_BUFFER };
        // read packet header: 2 bytes status, 2 bytes length (little endian)
        let offset = self.rx_offset;
        // RTL8139 uses ring buffer; must ensure offset + 4 <= RX_BUFFER_SIZE
        if offset + 4 > RX_BUFFER_SIZE {
            // reset pointer
            self.rx_offset = 0;
            return;
        }
        let status = u16::from_le_bytes([rx_buf[offset], rx_buf[offset + 1]]);
        let length = u16::from_le_bytes([rx_buf[offset + 2], rx_buf[offset + 3]]) as usize;

        // sanity checks >:)
        if length == 0 || length > MAX_PACKET_SIZE || offset + 4 + length > RX_BUFFER_SIZE {
            self.rx_err_count = self.rx_err_count.wrapping_add(1);
            self.advance_capr(4);
            return;
        }

        if status & 0x01 != 0 {
            let start = offset + 4;
            let end = start + length;
            let packet = &rx_buf[start..end];
            // copy to packet_queue
            self.packet_queue.push(packet);
        } else {
            self.rx_err_count = self.rx_err_count.wrapping_add(1);
        }

        // advance rx_offset by frame length + header (4) and align to dword
        let new_offset = (offset + 4 + length + 3) & !3;
        self.rx_offset = new_offset % RX_BUFFER_SIZE;

        // write CAPR = rx_offset - 16 (per RTL8139 doc) (16 is recommended headroom)
        let capr_val = if self.rx_offset >= 16 {
            (self.rx_offset - 16) as u16
        } else {
            0u16
        };
        self.write_reg16(CAPR, capr_val);
    }

    /// advance CAPR by `advance` bytes
    fn advance_capr(&mut self, advance: usize) {
        self.rx_offset = (self.rx_offset + advance) % RX_BUFFER_SIZE;
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
        unsafe {
            RX_BUFFER.fill(0u8);
        }
        self.write_reg8(CR, CR_RX_ENABLE | CR_TX_ENABLE);
        self.write_reg16(CAPR, 0);
        let isr = self.read_reg16(ISR);
        unsafe {
            self.port16(ISR).write(isr);
        }
        compiler_fence(Ordering::SeqCst);
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
                self.write_reg32(tsd_off, data.len() as u32);
                let mut wait = 0usize;
                loop {
                    let tsd_check = self.read_reg32(tsd_off);
                    if tsd_check == 0 {
                        return Ok(());
                    }
                    wait += 1;
                    if wait > TX_WAIT_LIMIT {
                        self.tx_err_count = self.tx_err_count.wrapping_add(1);
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
            tx_errors: self.tx_err_count,
            queued_packets: self.packet_queue.len(),
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
pub extern "C" fn network_scan() -> ! {
    if let Some((bus, device, function, bar0)) = pci_find_rtl8139() {
        let io_bar = (bar0 & 0xFFFF_FFFC) as u32;
        let io_base = (io_bar & 0xFFFF) as u16;
        let rx_phys = unsafe { &RX_BUFFER as *const _ as u32 };
        let mut dev = Rtl8139::new(io_base, rx_phys);
        unsafe {
            GLOBAL_RTL = &mut dev as *mut Rtl8139;
        }
        loop {
            while let Some(pkt) = dev.poll_dequeue() {
                if let Some((dst, src, ethertype)) = Rtl8139::parse_ethernet_frame(pkt) {
                    let is_broadcast = dst == [0xffu8; 6];
                    let is_for_me = dst == dev.mac;
                    if is_broadcast || is_for_me {
                        let _ = (dst, src, ethertype);
                    }
                }
            }
            hlt();
        }
    } else {
        loop {
            hlt();
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

fn pci_find_rtl8139() -> Option<(u8, u8, u8, u32)> {
    for bus in 0u8..=255 {
        for device in 0u8..32 {
            for func in 0u8..8 {
                let vendor_device = pci_read_u32(bus, device, func, 0x00);
                let vendor = (vendor_device & 0xFFFF) as u16;
                if vendor == 0xFFFF {
                    continue;
                }
                let device_id = ((vendor_device >> 16) & 0xFFFF) as u16;
                if vendor == RTL_VENDOR_ID && device_id == RTL_DEVICE_ID {
                    let bar0 = pci_read_u32(bus, device, func, 0x10);
                    let cmd = pci_read_u32(bus, device, func, 0x04) as u16;
                    let new_cmd = cmd | 0x0001 | 0x0004;
                    pci_write_u32(bus, device, func, 0x04, new_cmd as u32);
                    return Some((bus, device, func, bar0));
                }
            }
        }
    }
    None
}
