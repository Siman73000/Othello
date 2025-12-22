#![allow(dead_code)]

//! Minimal TCP client (polling, single-thread friendly).
//!
//! Supports:
//! - Active open (SYN -> SYN/ACK -> ACK)
//! - In-order receive buffering
//! - Basic transmit (chunked to ~MSS)
//!
//! Limitations:
//! - No out-of-order reassembly
//! - No full retransmission strategy
//! - Small fixed window

extern crate alloc;

use alloc::vec::Vec;
use crate::time;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpError {
    NoNic,
    NotConfigured,
    ArpTimeout,
    Timeout,
    TxFail,
    Reset,
    Proto,
}

fn write_u16_be(buf: &mut [u8], off: usize, v: u16) { buf[off..off+2].copy_from_slice(&v.to_be_bytes()); }
fn write_u32_be(buf: &mut [u8], off: usize, v: u32) { buf[off..off+4].copy_from_slice(&v.to_be_bytes()); }

fn tcp_checksum(src_ip: [u8;4], dst_ip: [u8;4], seg: &[u8]) -> u16 {
    let len = seg.len() as u16;
    let mut tmp = Vec::with_capacity(12 + seg.len());
    tmp.extend_from_slice(&src_ip);
    tmp.extend_from_slice(&dst_ip);
    tmp.push(0);
    tmp.push(6);
    tmp.extend_from_slice(&len.to_be_bytes());
    tmp.extend_from_slice(seg);
    super::checksum16(&tmp)
}

fn route_next_hop(src_ip: [u8;4], dst_ip: [u8;4], mask: [u8;4], gw: [u8;4]) -> [u8;4] {
    // If mask is 0.0.0.0 (some DHCP failures), treat everything as off-subnet and use the gateway.
    let mask_is_zero = mask == [0,0,0,0];
    if !mask_is_zero && (super::same_subnet(src_ip, dst_ip, mask) || gw == [0,0,0,0]) {
        dst_ip
    } else if gw != [0,0,0,0] {
        gw
    } else {
        dst_ip
    }
}

pub struct TcpStream {
    remote_ip: [u8;4],
    remote_port: u16,
    local_port: u16,
    seq: u32,
    ack: u32,
    rx: Vec<u8>,
    fin_seen: bool,
    rst_seen: bool,
}

impl TcpStream {
    pub fn connect(remote_ip: [u8;4], remote_port: u16, timeout_spins: u32) -> Result<Self, TcpError> {
        super::init();

        let (src_ip, mask, gw, our_mac) = unsafe {
            if super::NET.rtl.is_none() { return Err(TcpError::NoNic); }
            if super::NET.cfg.ip == [0,0,0,0] { return Err(TcpError::NotConfigured); }
            (super::NET.cfg.ip, super::NET.cfg.mask, super::NET.cfg.gateway, super::NET.cfg.mac)
        };

        let next_hop = route_next_hop(src_ip, remote_ip, mask, gw);
        let dst_mac = match super::arp_resolve(next_hop, our_mac, src_ip) {
            Ok(m) => m,
            Err(super::PingError::ArpTimeout) => return Err(TcpError::ArpTimeout),
            Err(super::PingError::NoNic) => return Err(TcpError::NoNic),
            Err(super::PingError::NotConfigured) => return Err(TcpError::NotConfigured),
            _ => return Err(TcpError::Timeout),
        };

        let local_port: u16 = 49152u16.wrapping_add((time::rdtsc() as u16) & 0x3FFF);
        let iss: u32 = (time::rdtsc() as u32) ^ 0xA5A5_5A5A;

        // SYN options: MSS 1460 (kind=2,len=4,val=0x05B4) + pad to 4 bytes
        let opts = [2u8, 4u8, 0x05u8, 0xB4u8];

        // SYN retry loop
        for _ in 0..3 {
            Self::send_segment_raw(src_ip, remote_ip, dst_mac, local_port, remote_port, iss, 0, 0x02, 4096, &opts, &[])?;

            let mut spins: u32 = 0;
            while spins < timeout_spins {
                spins = spins.wrapping_add(1);
                time::cpu_pause();

                if let Some((flags, seq_r, ack_r, payload)) = Self::poll_for_segment(src_ip, remote_ip, local_port, remote_port) {
                    if (flags & 0x04) != 0 { return Err(TcpError::Reset); } // RST
                    if (flags & 0x12) == 0x12 { // SYN|ACK
                        if ack_r != iss.wrapping_add(1) { continue; }
                        let ack = seq_r.wrapping_add(1);

                        // ACK to finish handshake
                        Self::send_segment_raw(src_ip, remote_ip, dst_mac, local_port, remote_port, iss.wrapping_add(1), ack, 0x10, 4096, &[], &[])?;

                        let mut s = TcpStream {
                            remote_ip,
                            remote_port,
                            local_port,
                            seq: iss.wrapping_add(1),
                            ack,
                            rx: Vec::new(),
                            fin_seen: false,
                            rst_seen: false,
                        };

                        if !payload.is_empty() {
                            s.rx.extend_from_slice(payload);
                            s.ack = s.ack.wrapping_add(payload.len() as u32);
                            let _ = s.send_ack();
                        }

                        return Ok(s);
                    }
                }
            }
        }

        Err(TcpError::Timeout)
    }

    pub fn write_all(&mut self, data: &[u8]) -> Result<(), TcpError> {
        let (src_ip, mask, gw, our_mac) = unsafe {
            if super::NET.rtl.is_none() { return Err(TcpError::NoNic); }
            if super::NET.cfg.ip == [0,0,0,0] { return Err(TcpError::NotConfigured); }
            (super::NET.cfg.ip, super::NET.cfg.mask, super::NET.cfg.gateway, super::NET.cfg.mac)
        };

        let next_hop = route_next_hop(src_ip, self.remote_ip, mask, gw);
        let dst_mac = match super::arp_resolve(next_hop, our_mac, src_ip) {
            Ok(m) => m,
            Err(super::PingError::ArpTimeout) => return Err(TcpError::ArpTimeout),
            Err(super::PingError::NoNic) => return Err(TcpError::NoNic),
            Err(super::PingError::NotConfigured) => return Err(TcpError::NotConfigured),
            _ => return Err(TcpError::Timeout),
        };

        let mut off = 0usize;
        while off < data.len() {
            let take = (data.len() - off).min(1460);
            let chunk = &data[off..off+take];

            Self::send_segment_raw(src_ip, self.remote_ip, dst_mac, self.local_port, self.remote_port,
                                   self.seq, self.ack, 0x18, 4096, &[], chunk)?;
            self.seq = self.seq.wrapping_add(take as u32);
            off += take;

            self.poll_once();
        }

        Ok(())
    }

    pub fn read_to_end(&mut self, max_bytes: usize, timeout_spins: u32) -> Result<Vec<u8>, TcpError> {
        // NOTE: the old implementation returned Ok(empty) on a read timeout,
        // which then caused the HTTP layer to report a confusing Parse error.
        // We now surface real TCP timeouts/resets.

        let mut idle: u32 = 0;
        let mut got_any = !self.rx.is_empty();

        while !self.fin_seen && self.rx.len() < max_bytes {
            let mut spins: u32 = 0;
            let mut progressed = false;
            while spins < timeout_spins {
                spins = spins.wrapping_add(1);
                time::cpu_pause();
                if self.poll_once() {
                    progressed = true;
                    got_any = true;
                    break;
                }
            }

            if progressed {
                idle = 0;
            } else {
                idle = idle.wrapping_add(1);
                if idle >= 200 { break; }
            }
        }

        if self.rst_seen { return Err(TcpError::Reset); }
        if !got_any { return Err(TcpError::Timeout); }
        Ok(core::mem::take(&mut self.rx))
    }

    pub fn close(&mut self) -> Result<(), TcpError> {
        let (src_ip, mask, gw, our_mac) = unsafe {
            if super::NET.rtl.is_none() { return Err(TcpError::NoNic); }
            if super::NET.cfg.ip == [0,0,0,0] { return Err(TcpError::NotConfigured); }
            (super::NET.cfg.ip, super::NET.cfg.mask, super::NET.cfg.gateway, super::NET.cfg.mac)
        };

        let next_hop = route_next_hop(src_ip, self.remote_ip, mask, gw);
        let dst_mac = match super::arp_resolve(next_hop, our_mac, src_ip) {
            Ok(m) => m,
            Err(_) => return Err(TcpError::ArpTimeout),
        };

        Self::send_segment_raw(src_ip, self.remote_ip, dst_mac, self.local_port, self.remote_port,
                               self.seq, self.ack, 0x11, 4096, &[], &[])?;
        self.seq = self.seq.wrapping_add(1);
        Ok(())
    }

    fn send_ack(&mut self) -> Result<(), TcpError> {
        let (src_ip, mask, gw, our_mac) = unsafe {
            if super::NET.rtl.is_none() { return Err(TcpError::NoNic); }
            if super::NET.cfg.ip == [0,0,0,0] { return Err(TcpError::NotConfigured); }
            (super::NET.cfg.ip, super::NET.cfg.mask, super::NET.cfg.gateway, super::NET.cfg.mac)
        };

        let next_hop = route_next_hop(src_ip, self.remote_ip, mask, gw);
        let dst_mac = match super::arp_resolve(next_hop, our_mac, src_ip) {
            Ok(m) => m,
            Err(_) => return Err(TcpError::ArpTimeout),
        };

        Self::send_segment_raw(src_ip, self.remote_ip, dst_mac, self.local_port, self.remote_port,
                               self.seq, self.ack, 0x10, 4096, &[], &[])?;
        Ok(())
    }

    fn poll_once(&mut self) -> bool {
        let our_ip = unsafe { super::NET.cfg.ip };
        let Some((flags, seq_r, _ack_r, payload)) = Self::poll_for_segment(our_ip, self.remote_ip, self.local_port, self.remote_port) else {
            return false;
        };

        if (flags & 0x04) != 0 {
            // RST: surface as an error instead of silently turning into an HTTP Parse.
            self.rst_seen = true;
            self.fin_seen = true;
            return true;
        }

        if seq_r == self.ack {
    let mut did_any = false;

    if !payload.is_empty() {
        self.rx.extend_from_slice(payload);
        self.ack = self.ack.wrapping_add(payload.len() as u32);
        did_any = true;
    }

    // FIN can be piggy-backed on the last data segment. If we don't account for it,
    // higher layers can hang forever waiting for EOF.
    if (flags & 0x01) != 0 {
        self.ack = self.ack.wrapping_add(1);
        self.fin_seen = true;
        did_any = true;
    }

    if did_any {
        let _ = self.send_ack();
        return true;
    }
}


        false
    }

    fn poll_for_segment(our_ip: [u8;4], _remote_ip: [u8;4], local_port: u16, remote_port: u16)
        -> Option<(u16, u32, u32, &'static [u8])>
    {
        let frame_opt = unsafe { super::NET.rtl.as_mut().unwrap().poll_recv() };
        let Some(frame) = frame_opt else { return None; };

        if frame.len() < 14 + 20 { return None; }
        let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
        if ethertype != 0x0800 { return None; }

        let ip_off = 14;
        let ver_ihl = frame[ip_off];
        if (ver_ihl >> 4) != 4 { return None; }
        let ihl = ((ver_ihl & 0x0F) as usize) * 4;
        if frame.len() < ip_off + ihl + 20 { return None; }
        if frame[ip_off + 9] != 6 { return None; }

        let _src = [frame[ip_off + 12], frame[ip_off + 13], frame[ip_off + 14], frame[ip_off + 15]];
        let dst = [frame[ip_off + 16], frame[ip_off + 17], frame[ip_off + 18], frame[ip_off + 19]];
        if dst != our_ip { return None; }
        // Some user-mode NATs / emulations can present replies with unexpected L3 sources.
        // As long as it is destined to us and the TCP ports match, accept it.

        let tcp_off = ip_off + ihl;
        let sp = u16::from_be_bytes([frame[tcp_off], frame[tcp_off + 1]]);
        let dp = u16::from_be_bytes([frame[tcp_off + 2], frame[tcp_off + 3]]);
        if sp != remote_port || dp != local_port { return None; }

        let seq = u32::from_be_bytes([frame[tcp_off + 4], frame[tcp_off + 5], frame[tcp_off + 6], frame[tcp_off + 7]]);
        let ack = u32::from_be_bytes([frame[tcp_off + 8], frame[tcp_off + 9], frame[tcp_off + 10], frame[tcp_off + 11]]);
        let data_off = ((frame[tcp_off + 12] >> 4) as usize) * 4;
        let flags = frame[tcp_off + 13] as u16;

        if data_off < 20 { return None; }
        let payload_off = tcp_off + data_off;
        if payload_off > frame.len() { return None; }

        let payload = &frame[payload_off..];

        Some((flags, seq, ack, payload))
    }

    fn send_segment_raw(
        src_ip: [u8;4],
        dst_ip: [u8;4],
        dst_mac: [u8;6],
        src_port: u16,
        dst_port: u16,
        seq: u32,
        ack: u32,
        flags: u16,
        window: u16,
        options: &[u8],
        payload: &[u8],
    ) -> Result<(), TcpError> {
        let opt_len = options.len();
        let hdr_len = 20 + opt_len;
        let tcp_len = hdr_len + payload.len();
        let ip_len = 20 + tcp_len;

        let mut seg = Vec::with_capacity(tcp_len);
        seg.resize(hdr_len, 0);
        write_u16_be(&mut seg, 0, src_port);
        write_u16_be(&mut seg, 2, dst_port);
        write_u32_be(&mut seg, 4, seq);
        write_u32_be(&mut seg, 8, ack);

        let data_off_words = (hdr_len / 4) as u8;
        seg[12] = data_off_words << 4;
        seg[13] = (flags & 0xFF) as u8;
        write_u16_be(&mut seg, 14, window);
        write_u16_be(&mut seg, 16, 0);
        write_u16_be(&mut seg, 18, 0);

        if opt_len > 0 {
            seg[20..20+opt_len].copy_from_slice(options);
        }
        if !payload.is_empty() {
            seg.extend_from_slice(payload);
        }

        let csum = tcp_checksum(src_ip, dst_ip, &seg);
        write_u16_be(&mut seg, 16, csum);

        let mut ip = Vec::with_capacity(ip_len);
        ip.resize(20, 0);
        ip[0] = 0x45;
        ip[1] = 0;
        write_u16_be(&mut ip, 2, ip_len as u16);
        write_u16_be(&mut ip, 4, (time::rdtsc() as u16) ^ 0x1234);
        write_u16_be(&mut ip, 6, 0);
        ip[8] = 64;
        ip[9] = 6;
        write_u16_be(&mut ip, 10, 0);
        ip[12..16].copy_from_slice(&src_ip);
        ip[16..20].copy_from_slice(&dst_ip);
        let ip_csum = super::checksum16(&ip[..20]);
        write_u16_be(&mut ip, 10, ip_csum);

        ip.extend_from_slice(&seg);

        let ok = unsafe {
            let rtl = super::NET.rtl.as_mut().unwrap();
            rtl.send_frame(dst_mac, 0x0800, &ip)
        };
        if !ok { return Err(TcpError::TxFail); }
        Ok(())
    }
}
