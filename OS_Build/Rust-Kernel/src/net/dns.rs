#![allow(dead_code)]

//! Minimal DNS A record resolver over UDP.
//!
//! Blocking/polling implementation intended for early-boot/OS-dev use.

extern crate alloc;

use alloc::vec::Vec;

use crate::time;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnsError {
    NoNic,
    NotConfigured,
    Timeout,
    Malformed,
    NoAnswer,
    TxFail,
}

fn parse_ipv4_literal(host: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut idx = 0usize;
    let mut acc: u16 = 0;
    let mut saw = false;

    for b in host.bytes() {
        match b {
            b'0'..=b'9' => {
                saw = true;
                acc = (acc * 10).saturating_add((b - b'0') as u16);
                if acc > 255 { return None; }
            }
            b'.' => {
                if !saw || idx >= 4 { return None; }
                out[idx] = acc as u8;
                idx += 1;
                acc = 0;
                saw = false;
            }
            _ => return None,
        }
    }
    if idx != 3 || !saw { return None; }
    out[3] = acc as u8;
    Some(out)
}

fn write_u16_be(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_be_bytes());
}

fn udp_checksum(src_ip: [u8; 4], dst_ip: [u8; 4], udp_hdr_and_payload: &[u8]) -> u16 {
    // Pseudoheader: src(4), dst(4), zero(1), proto(1), len(2)
    let len = udp_hdr_and_payload.len() as u16;
    let mut tmp = Vec::with_capacity(12 + udp_hdr_and_payload.len());
    tmp.extend_from_slice(&src_ip);
    tmp.extend_from_slice(&dst_ip);
    tmp.push(0);
    tmp.push(17);
    tmp.extend_from_slice(&len.to_be_bytes());
    tmp.extend_from_slice(udp_hdr_and_payload);
    super::checksum16(&tmp)
}

fn build_dns_query(id: u16, host: &str) -> Vec<u8> {
    let mut q = Vec::new();
    q.resize(12, 0);

    write_u16_be(&mut q, 0, id);
    write_u16_be(&mut q, 2, 0x0100); // RD
    write_u16_be(&mut q, 4, 1);      // QDCOUNT

    // QNAME
    for label in host.split('.') {
        let bytes = label.as_bytes();
        let n = bytes.len().min(63);
        q.push(n as u8);
        q.extend_from_slice(&bytes[..n]);
    }
    q.push(0);

    // QTYPE=A, QCLASS=IN
    q.extend_from_slice(&1u16.to_be_bytes());
    q.extend_from_slice(&1u16.to_be_bytes());

    q
}

fn skip_name(msg: &[u8], mut off: usize) -> Option<usize> {
    let mut jumps = 0usize;
    loop {
        if off >= msg.len() { return None; }
        let b = msg[off];
        if b & 0xC0 == 0xC0 {
            if off + 1 >= msg.len() { return None; }
            return Some(off + 2);
        }
        if b == 0 {
            return Some(off + 1);
        }
        let len = b as usize;
        off += 1;
        if off + len > msg.len() { return None; }
        off += len;

        jumps += 1;
        if jumps > 255 { return None; }
    }
}

pub fn resolve_a(host: &str) -> Result<[u8; 4], DnsError> {
    if let Some(ip) = parse_ipv4_literal(host) {
        return Ok(ip);
    }

    super::init();

    let (src_ip, mask, gateway, our_mac, dns_ip) = unsafe {
        if super::NET.rtl.is_none() { return Err(DnsError::NoNic); }
        if super::NET.cfg.ip == [0, 0, 0, 0] { return Err(DnsError::NotConfigured); }
        (super::NET.cfg.ip, super::NET.cfg.mask, super::NET.cfg.gateway, super::NET.cfg.mac, super::NET.cfg.dns)
    };

    let dst_ip = if dns_ip == [0, 0, 0, 0] { gateway } else { dns_ip };
    if dst_ip == [0, 0, 0, 0] { return Err(DnsError::NotConfigured); }

    let next_hop = if super::same_subnet(src_ip, dst_ip, mask) || gateway == [0, 0, 0, 0] {
        dst_ip
    } else {
        gateway
    };

    let dst_mac = match super::arp_resolve(next_hop, our_mac, src_ip) {
        Ok(m) => m,
        Err(super::PingError::ArpTimeout) => return Err(DnsError::Timeout),
        Err(super::PingError::NoNic) => return Err(DnsError::NoNic),
        Err(super::PingError::NotConfigured) => return Err(DnsError::NotConfigured),
        _ => return Err(DnsError::Timeout),
    };

    let id = (time::rdtsc() as u16) ^ 0xBEEF;
    let q = build_dns_query(id, host);

    let src_port: u16 = 53000u16.wrapping_add((time::rdtsc() as u16) & 0x0FFF);
    let dst_port: u16 = 53;
    let udp_len = (8 + q.len()) as u16;

    let mut udp = Vec::with_capacity(8 + q.len());
    udp.resize(8, 0);
    write_u16_be(&mut udp, 0, src_port);
    write_u16_be(&mut udp, 2, dst_port);
    write_u16_be(&mut udp, 4, udp_len);
    write_u16_be(&mut udp, 6, 0);
    udp.extend_from_slice(&q);

    let csum = udp_checksum(src_ip, dst_ip, &udp);
    write_u16_be(&mut udp, 6, csum);

    let ip_len = 20 + udp.len();
    let mut ip = Vec::with_capacity(ip_len);
    ip.resize(20, 0);
    ip[0] = 0x45;
    ip[1] = 0;
    write_u16_be(&mut ip, 2, ip_len as u16);
    write_u16_be(&mut ip, 4, id);
    write_u16_be(&mut ip, 6, 0);
    ip[8] = 64;
    ip[9] = 17;
    write_u16_be(&mut ip, 10, 0);
    ip[12..16].copy_from_slice(&src_ip);
    ip[16..20].copy_from_slice(&dst_ip);
    let ip_csum = super::checksum16(&ip[..20]);
    write_u16_be(&mut ip, 10, ip_csum);
    ip.extend_from_slice(&udp);

    let sent = unsafe {
        let rtl = super::NET.rtl.as_mut().unwrap();
        rtl.send_frame(dst_mac, 0x0800, &ip)
    };
    if !sent { return Err(DnsError::TxFail); }

    let mut spins: u32 = 0;
    while spins < 18_000_000 {
        spins = spins.wrapping_add(1);
        unsafe { time::cpu_pause(); }

        let frame_opt = unsafe { super::NET.rtl.as_mut().unwrap().poll_recv() };
        let Some(frame) = frame_opt else { continue; };

        if frame.len() < 14 + 20 { continue; }
        let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
        if ethertype != 0x0800 { continue; }

        let ip_off = 14;
        let ver_ihl = frame[ip_off];
        if (ver_ihl >> 4) != 4 { continue; }
        let ihl = ((ver_ihl & 0x0F) as usize) * 4;
        if frame.len() < ip_off + ihl + 8 { continue; }
        if frame[ip_off + 9] != 17 { continue; }

        let src = [frame[ip_off + 12], frame[ip_off + 13], frame[ip_off + 14], frame[ip_off + 15]];
        let dst = [frame[ip_off + 16], frame[ip_off + 17], frame[ip_off + 18], frame[ip_off + 19]];
        if src != dst_ip || dst != src_ip { continue; }

        let udp_off = ip_off + ihl;
        let got_src_port = u16::from_be_bytes([frame[udp_off], frame[udp_off + 1]]);
        let got_dst_port = u16::from_be_bytes([frame[udp_off + 2], frame[udp_off + 3]]);
        if got_src_port != 53 || got_dst_port != src_port { continue; }

        let dns_off = udp_off + 8;
        if frame.len() < dns_off + 12 { continue; }
        let got_id = u16::from_be_bytes([frame[dns_off], frame[dns_off + 1]]);
        if got_id != id { continue; }

        let flags = u16::from_be_bytes([frame[dns_off + 2], frame[dns_off + 3]]);
        if (flags & 0x8000) == 0 { continue; }
        if (flags & 0x000F) != 0 { return Err(DnsError::NoAnswer); }

        let qd = u16::from_be_bytes([frame[dns_off + 4], frame[dns_off + 5]]) as usize;
        let an = u16::from_be_bytes([frame[dns_off + 6], frame[dns_off + 7]]) as usize;
        if an == 0 { return Err(DnsError::NoAnswer); }

        let msg = &frame[dns_off..];
        let mut off = 12usize;

        for _ in 0..qd {
            off = skip_name(msg, off).ok_or(DnsError::Malformed)?;
            if off + 4 > msg.len() { return Err(DnsError::Malformed); }
            off += 4;
        }

        for _ in 0..an {
            off = skip_name(msg, off).ok_or(DnsError::Malformed)?;
            if off + 10 > msg.len() { return Err(DnsError::Malformed); }
            let typ = u16::from_be_bytes([msg[off], msg[off + 1]]); off += 2;
            let cls = u16::from_be_bytes([msg[off], msg[off + 1]]); off += 2;
            off += 4;
            let rdlen = u16::from_be_bytes([msg[off], msg[off + 1]]) as usize; off += 2;
            if off + rdlen > msg.len() { return Err(DnsError::Malformed); }

            if typ == 1 && cls == 1 && rdlen == 4 {
                return Ok([msg[off], msg[off + 1], msg[off + 2], msg[off + 3]]);
            }
            off += rdlen;
        }

        return Err(DnsError::NoAnswer);
    }

    Err(DnsError::Timeout)
}
