//! Networking: a small TCP/IP stack on top of the e1000 driver.
//!
//! Ethernet, ARP, IPv4, ICMP (ping), UDP, DHCP, DNS and a TCP that is good
//! enough for fetching web pages. Programs use the blocking calls at the
//! bottom ([`resolve`], [`tcp_connect`], [`tcp_send`], [`tcp_recv`], ...);
//! the stack is driven by a background thread ([`start`]) and by whoever is
//! waiting.

pub mod e1000;
pub mod http;

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

pub type Ip = [u8; 4];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    NoNetwork,
    NoRoute,
    Dns(String),
    Timeout,
    Refused,
    Closed,
}

impl core::fmt::Display for NetError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            NetError::NoNetwork => write!(f, "no network card or no link"),
            NetError::NoRoute => write!(f, "no route to host"),
            NetError::Dns(name) => write!(f, "cannot find {}", name),
            NetError::Timeout => write!(f, "timed out"),
            NetError::Refused => write!(f, "connection refused"),
            NetError::Closed => write!(f, "connection closed"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum State {
    SynSent,
    Established,
    /// We sent FIN, waiting for the peer's.
    FinWait,
    /// The peer closed; we may still send.
    CloseWait,
    LastAck,
    Closed,
}

struct Tcp {
    id: usize,
    state: State,
    local_port: u16,
    remote_ip: Ip,
    remote_port: u16,
    snd_una: u32,
    snd_nxt: u32,
    rcv_nxt: u32,
    /// Bytes from `snd_una` on: sent but unacknowledged first, then unsent.
    tx: VecDeque<u8>,
    rx: VecDeque<u8>,
    peer_window: u32,
    last_send: u64,
    retries: u32,
    want_close: bool,
    fin_sent: bool,
    reset: bool,
}

struct Pending {
    id: u16,
    sent_at: u64,
    reply_at: Option<u64>,
}

pub struct Stack {
    nic: e1000::E1000,
    pub mac: [u8; 6],
    pub ip: Ip,
    pub mask: Ip,
    pub gateway: Ip,
    pub dns: Ip,
    pub configured: bool,
    pub dhcp: bool,
    arp: BTreeMap<Ip, [u8; 6]>,
    tcp: Vec<Tcp>,
    udp: VecDeque<(Ip, u16, u16, Vec<u8>)>,
    dns_cache: BTreeMap<String, Ip>,
    next_port: u16,
    next_id: usize,
    ip_id: u16,
    ping: Option<Pending>,
    dhcp_xid: u32,
    dhcp_offer: Option<(Ip, Ip, Ip, Ip, Ip)>,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

static STACK: Mutex<Option<Stack>> = Mutex::new(None);
static STARTED: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------- helpers

fn be16(b: &[u8], o: usize) -> u16 {
    (b[o] as u16) << 8 | b[o + 1] as u16
}

fn be32(b: &[u8], o: usize) -> u32 {
    (b[o] as u32) << 24 | (b[o + 1] as u32) << 16 | (b[o + 2] as u32) << 8 | b[o + 3] as u32
}

fn put16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn put32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn checksum(parts: &[&[u8]]) -> u16 {
    let mut sum: u32 = 0;
    let mut odd: Option<u8> = None;
    for part in parts {
        for &b in part.iter() {
            match odd.take() {
                Some(hi) => sum += (hi as u32) << 8 | b as u32,
                None => odd = Some(b),
            }
        }
    }
    if let Some(hi) = odd {
        sum += (hi as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

fn pseudo_header(src: Ip, dst: Ip, proto: u8, len: usize) -> [u8; 12] {
    let mut h = [0u8; 12];
    h[0..4].copy_from_slice(&src);
    h[4..8].copy_from_slice(&dst);
    h[9] = proto;
    h[10..12].copy_from_slice(&(len as u16).to_be_bytes());
    h
}

pub fn format_ip(ip: Ip) -> String {
    alloc::format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

pub fn parse_ip(s: &str) -> Option<Ip> {
    let mut out = [0u8; 4];
    let mut n = 0;
    for part in s.split('.') {
        if n >= 4 {
            return None;
        }
        out[n] = part.parse().ok()?;
        n += 1;
    }
    if n == 4 { Some(out) } else { None }
}

const BROADCAST_MAC: [u8; 6] = [0xFF; 6];

// ---------------------------------------------------------------- the stack

impl Stack {
    fn same_subnet(&self, ip: Ip) -> bool {
        (0..4).all(|i| ip[i] & self.mask[i] == self.ip[i] & self.mask[i])
    }

    fn send_frame(&mut self, dst: [u8; 6], ethertype: u16, payload: &[u8]) {
        let mut frame = Vec::with_capacity(14 + payload.len());
        frame.extend_from_slice(&dst);
        frame.extend_from_slice(&self.mac);
        put16(&mut frame, ethertype);
        frame.extend_from_slice(payload);
        // wait briefly if the ring is full
        for _ in 0..2000 {
            if self.nic.transmit(&frame) {
                self.tx_packets += 1;
                return;
            }
            core::hint::spin_loop();
        }
    }

    fn arp_request(&mut self, target: Ip) {
        let mut p = Vec::new();
        put16(&mut p, 1);
        put16(&mut p, 0x0800);
        p.push(6);
        p.push(4);
        put16(&mut p, 1);
        p.extend_from_slice(&self.mac);
        p.extend_from_slice(&self.ip);
        p.extend_from_slice(&[0; 6]);
        p.extend_from_slice(&target);
        self.send_frame(BROADCAST_MAC, 0x0806, &p);
    }

    /// Sends an IPv4 packet. Returns false if the next hop's address is not known yet.
    fn send_ip(&mut self, dst: Ip, proto: u8, payload: &[u8]) -> bool {
        let src = self.ip;
        self.send_ip_from(src, dst, proto, payload)
    }

    fn send_ip_from(&mut self, src: Ip, dst: Ip, proto: u8, payload: &[u8]) -> bool {
        let broadcast = dst == [255; 4];
        let hop = if self.same_subnet(dst) || broadcast { dst } else { self.gateway };
        let mac = if broadcast {
            BROADCAST_MAC
        } else {
            match self.arp.get(&hop) {
                Some(m) => *m,
                None => {
                    self.arp_request(hop);
                    return false;
                }
            }
        };
        let mut p = Vec::with_capacity(20 + payload.len());
        p.push(0x45);
        p.push(0);
        put16(&mut p, (20 + payload.len()) as u16);
        self.ip_id = self.ip_id.wrapping_add(1);
        put16(&mut p, self.ip_id);
        put16(&mut p, 0x4000);
        p.push(64);
        p.push(proto);
        put16(&mut p, 0);
        p.extend_from_slice(&src);
        p.extend_from_slice(&dst);
        let c = checksum(&[&p[..20]]);
        p[10..12].copy_from_slice(&c.to_be_bytes());
        p.extend_from_slice(payload);
        self.send_frame(mac, 0x0800, &p);
        true
    }

    fn send_udp(&mut self, src_ip: Ip, dst: Ip, src_port: u16, dst_port: u16, data: &[u8]) -> bool {
        let len = 8 + data.len();
        let mut u = Vec::with_capacity(len);
        put16(&mut u, src_port);
        put16(&mut u, dst_port);
        put16(&mut u, len as u16);
        put16(&mut u, 0);
        u.extend_from_slice(data);
        let c = checksum(&[&pseudo_header(src_ip, dst, 17, len), &u]);
        let c = if c == 0 { 0xFFFF } else { c };
        u[6..8].copy_from_slice(&c.to_be_bytes());
        self.send_ip_from(src_ip, dst, 17, &u)
    }

    fn send_tcp_segment(&mut self, idx: usize, flags: u8, seq: u32, data: &[u8]) {
        let c = &self.tcp[idx];
        let (remote, sport, dport, ack) = (c.remote_ip, c.local_port, c.remote_port, c.rcv_nxt);
        let window = (32768usize.saturating_sub(c.rx.len())).min(65535) as u16;
        let mut t = Vec::with_capacity(24 + data.len());
        put16(&mut t, sport);
        put16(&mut t, dport);
        put32(&mut t, seq);
        put32(&mut t, if flags & 0x10 != 0 { ack } else { 0 });
        let header_words = if flags & 0x02 != 0 { 6 } else { 5 };
        t.push(header_words << 4);
        t.push(flags);
        put16(&mut t, window);
        put16(&mut t, 0);
        put16(&mut t, 0);
        if flags & 0x02 != 0 {
            // maximum segment size option
            t.extend_from_slice(&[2, 4, 0x05, 0xB4]);
        }
        t.extend_from_slice(data);
        let len = t.len();
        let src = self.ip;
        let sum = checksum(&[&pseudo_header(src, remote, 6, len), &t]);
        t[16..18].copy_from_slice(&sum.to_be_bytes());
        self.send_ip(remote, 6, &t);
    }

    /// Sends whatever the connection may send now: data, then a FIN if asked.
    fn tcp_flush(&mut self, idx: usize) {
        let now = crate::time::uptime_ms();
        loop {
            let c = &self.tcp[idx];
            if !matches!(c.state, State::Established | State::CloseWait) {
                break;
            }
            let in_flight = c.snd_nxt.wrapping_sub(c.snd_una) as usize;
            let unsent = c.tx.len().saturating_sub(in_flight);
            let window = (c.peer_window as usize).min(8 * 1460);
            if unsent == 0 || in_flight >= window {
                break;
            }
            let n = unsent.min(1460).min(window - in_flight);
            let data: Vec<u8> = c.tx.iter().skip(in_flight).take(n).copied().collect();
            let seq = c.snd_nxt;
            self.send_tcp_segment(idx, 0x18, seq, &data);
            let c = &mut self.tcp[idx];
            c.snd_nxt = c.snd_nxt.wrapping_add(n as u32);
            c.last_send = now;
        }
        let c = &self.tcp[idx];
        let all_sent = c.tx.len() == c.snd_nxt.wrapping_sub(c.snd_una) as usize;
        if c.want_close && !c.fin_sent && all_sent && matches!(c.state, State::Established | State::CloseWait) {
            let seq = c.snd_nxt;
            let next_state = if c.state == State::CloseWait { State::LastAck } else { State::FinWait };
            self.send_tcp_segment(idx, 0x11, seq, &[]);
            let c = &mut self.tcp[idx];
            c.snd_nxt = c.snd_nxt.wrapping_add(1);
            c.fin_sent = true;
            c.state = next_state;
            c.last_send = now;
        }
    }

    fn handle_tcp(&mut self, src: Ip, data: &[u8]) {
        if data.len() < 20 {
            return;
        }
        let (sport, dport) = (be16(data, 0), be16(data, 2));
        let (seq, ack) = (be32(data, 4), be32(data, 8));
        let offset = (data[12] >> 4) as usize * 4;
        let flags = data[13];
        let window = be16(data, 14) as u32;
        if offset > data.len() {
            return;
        }
        let payload = &data[offset..];
        let Some(idx) = self.tcp.iter().position(|c| c.local_port == dport && c.remote_port == sport && c.remote_ip == src && c.state != State::Closed) else {
            return;
        };
        if flags & 0x04 != 0 {
            let c = &mut self.tcp[idx];
            c.reset = true;
            c.state = State::Closed;
            return;
        }
        match self.tcp[idx].state {
            State::SynSent => {
                if flags & 0x12 == 0x12 && ack == self.tcp[idx].snd_nxt {
                    let c = &mut self.tcp[idx];
                    c.rcv_nxt = seq.wrapping_add(1);
                    c.snd_una = ack;
                    c.state = State::Established;
                    c.peer_window = window;
                    c.retries = 0;
                    let s = c.snd_nxt;
                    self.send_tcp_segment(idx, 0x10, s, &[]);
                }
            }
            State::Closed => {}
            _ => {
                let c = &mut self.tcp[idx];
                c.peer_window = window;
                // acknowledged data leaves the send queue
                if flags & 0x10 != 0 {
                    let acked = ack.wrapping_sub(c.snd_una);
                    let outstanding = c.snd_nxt.wrapping_sub(c.snd_una);
                    if acked > 0 && acked <= outstanding {
                        let data_acked = if c.fin_sent && ack == c.snd_nxt { acked - 1 } else { acked } as usize;
                        for _ in 0..data_acked.min(c.tx.len()) {
                            c.tx.pop_front();
                        }
                        c.snd_una = ack;
                        c.retries = 0;
                        c.last_send = crate::time::uptime_ms();
                        if c.fin_sent && ack == c.snd_nxt && c.state == State::LastAck {
                            c.state = State::Closed;
                            return;
                        }
                    }
                }
                let mut send_ack = false;
                let c = &mut self.tcp[idx];
                if !payload.is_empty() {
                    if seq == c.rcv_nxt {
                        if c.rx.len() + payload.len() <= 262144 {
                            c.rx.extend(payload.iter());
                            c.rcv_nxt = c.rcv_nxt.wrapping_add(payload.len() as u32);
                        }
                    }
                    send_ack = true;
                }
                if flags & 0x01 != 0 && seq.wrapping_add(payload.len() as u32) == c.rcv_nxt {
                    c.rcv_nxt = c.rcv_nxt.wrapping_add(1);
                    send_ack = true;
                    match c.state {
                        State::Established => c.state = State::CloseWait,
                        State::FinWait => {
                            c.state = State::Closed;
                        }
                        _ => {}
                    }
                }
                if send_ack {
                    let s = self.tcp[idx].snd_nxt;
                    self.send_tcp_segment(idx, 0x10, s, &[]);
                }
                self.tcp_flush(idx);
            }
        }
    }

    fn handle_ip(&mut self, frame: &[u8]) {
        if frame.len() < 34 {
            return;
        }
        let ip = &frame[14..];
        if ip[0] >> 4 != 4 {
            return;
        }
        let ihl = (ip[0] & 15) as usize * 4;
        let total = be16(ip, 2) as usize;
        if ihl < 20 || total > ip.len() || total < ihl {
            return;
        }
        // fragments are not supported
        if be16(ip, 6) & 0x3FFF != 0 {
            return;
        }
        let src: Ip = [ip[12], ip[13], ip[14], ip[15]];
        let dst: Ip = [ip[16], ip[17], ip[18], ip[19]];
        if dst != self.ip && dst != [255; 4] && self.configured {
            return;
        }
        let body = &ip[ihl..total];
        match ip[9] {
            1 if body.len() >= 8 => {
                if body[0] == 8 {
                    // echo request: answer
                    let mut reply = body.to_vec();
                    reply[0] = 0;
                    reply[2] = 0;
                    reply[3] = 0;
                    let c = checksum(&[&reply]);
                    reply[2..4].copy_from_slice(&c.to_be_bytes());
                    self.send_ip(src, 1, &reply);
                } else if body[0] == 0 {
                    let id = be16(body, 4);
                    if let Some(p) = self.ping.as_mut() {
                        if p.id == id && p.reply_at.is_none() {
                            p.reply_at = Some(crate::time::uptime_ms());
                        }
                    }
                }
            }
            6 => self.handle_tcp(src, body),
            17 if body.len() >= 8 => {
                let (sport, dport) = (be16(body, 0), be16(body, 2));
                let len = (be16(body, 4) as usize).min(body.len());
                if len >= 8 {
                    let data = body[8..len].to_vec();
                    if dport == 68 {
                        self.handle_dhcp(&data);
                    } else if self.udp.len() < 64 {
                        self.udp.push_back((src, sport, dport, data));
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_arp(&mut self, frame: &[u8]) {
        if frame.len() < 42 {
            return;
        }
        let a = &frame[14..];
        let op = be16(a, 6);
        let sender_mac: [u8; 6] = [a[8], a[9], a[10], a[11], a[12], a[13]];
        let sender_ip: Ip = [a[14], a[15], a[16], a[17]];
        let target_ip: Ip = [a[24], a[25], a[26], a[27]];
        self.arp.insert(sender_ip, sender_mac);
        if op == 1 && target_ip == self.ip && self.configured {
            let mut p = Vec::new();
            put16(&mut p, 1);
            put16(&mut p, 0x0800);
            p.push(6);
            p.push(4);
            put16(&mut p, 2);
            p.extend_from_slice(&self.mac);
            p.extend_from_slice(&self.ip);
            p.extend_from_slice(&sender_mac);
            p.extend_from_slice(&sender_ip);
            self.send_frame(sender_mac, 0x0806, &p);
        }
    }

    fn poll_inner(&mut self) {
        for _ in 0..32 {
            let Some(frame) = self.nic.receive() else { break };
            self.rx_packets += 1;
            if frame.len() < 14 {
                continue;
            }
            match be16(&frame, 12) {
                0x0806 => self.handle_arp(&frame),
                0x0800 => self.handle_ip(&frame),
                _ => {}
            }
        }
        // retransmissions
        let now = crate::time::uptime_ms();
        for idx in 0..self.tcp.len() {
            let (state, outstanding, last, retries) = {
                let c = &self.tcp[idx];
                (c.state, c.snd_nxt != c.snd_una, c.last_send, c.retries)
            };
            if state == State::Closed || !outstanding || now < last + 500 {
                continue;
            }
            if retries >= 8 {
                let c = &mut self.tcp[idx];
                c.state = State::Closed;
                c.reset = true;
                continue;
            }
            self.tcp[idx].retries += 1;
            self.tcp[idx].last_send = now;
            let (fin_only, una) = {
                let c = &self.tcp[idx];
                // everything but the FIN has been acknowledged
                (c.fin_sent && c.tx.is_empty(), c.snd_una)
            };
            match state {
                State::SynSent => {
                    self.send_tcp_segment(idx, 0x02, una, &[]);
                }
                State::FinWait | State::LastAck if fin_only => {
                    self.send_tcp_segment(idx, 0x11, una, &[]);
                }
                _ => {
                    // go back and send the unacknowledged data again
                    self.tcp[idx].snd_nxt = una;
                    self.tcp_flush(idx);
                }
            }
        }
        // finished connections nobody references are dropped by tcp_close
    }

    // ---- DHCP --------------------------------------------------------------

    fn dhcp_message(&mut self, kind: u8, request: Option<(Ip, Ip)>) -> Vec<u8> {
        let mut m = Vec::with_capacity(300);
        m.push(1);
        m.push(1);
        m.push(6);
        m.push(0);
        put32(&mut m, self.dhcp_xid);
        put16(&mut m, 0);
        put16(&mut m, 0x8000); // ask for a broadcast reply
        m.extend_from_slice(&[0; 16]);
        m.extend_from_slice(&self.mac);
        m.extend_from_slice(&[0; 10]);
        m.extend_from_slice(&[0; 192]);
        m.extend_from_slice(&[99, 130, 83, 99]);
        m.extend_from_slice(&[53, 1, kind]);
        if let Some((ip, server)) = request {
            m.extend_from_slice(&[50, 4]);
            m.extend_from_slice(&ip);
            m.extend_from_slice(&[54, 4]);
            m.extend_from_slice(&server);
        }
        m.extend_from_slice(&[55, 3, 1, 3, 6]);
        m.push(255);
        m
    }

    fn handle_dhcp(&mut self, d: &[u8]) {
        if d.len() < 240 || d[0] != 2 || be32(d, 4) != self.dhcp_xid || d[236..240] != [99, 130, 83, 99] {
            return;
        }
        let yiaddr: Ip = [d[16], d[17], d[18], d[19]];
        let (mut kind, mut mask, mut router, mut dns, mut server) = (0u8, [255, 255, 255, 0], [0; 4], [0; 4], [0; 4]);
        let mut i = 240;
        while i + 1 < d.len() {
            let code = d[i];
            if code == 255 {
                break;
            }
            if code == 0 {
                i += 1;
                continue;
            }
            let len = d[i + 1] as usize;
            if i + 2 + len > d.len() {
                break;
            }
            let v = &d[i + 2..i + 2 + len];
            match code {
                53 if len == 1 => kind = v[0],
                1 if len == 4 => mask = [v[0], v[1], v[2], v[3]],
                3 if len >= 4 => router = [v[0], v[1], v[2], v[3]],
                6 if len >= 4 => dns = [v[0], v[1], v[2], v[3]],
                54 if len == 4 => server = [v[0], v[1], v[2], v[3]],
                _ => {}
            }
            i += 2 + len;
        }
        match kind {
            2 => self.dhcp_offer = Some((yiaddr, mask, router, dns, server)),
            5 => {
                self.ip = yiaddr;
                self.mask = mask;
                self.gateway = router;
                if dns != [0; 4] {
                    self.dns = dns;
                }
                self.configured = true;
                self.dhcp = true;
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- public API

fn with_stack<R>(f: impl FnOnce(&mut Stack) -> R) -> Option<R> {
    STACK.lock().as_mut().map(f)
}

/// Finds the network card and sets it up. Returns false if there is none.
pub fn init() -> bool {
    if STACK.lock().is_some() {
        return true;
    }
    let Some(nic) = e1000::E1000::init() else { return false };
    let mac = nic.mac;
    *STACK.lock() = Some(Stack {
        nic,
        mac,
        ip: [0; 4],
        mask: [255, 255, 255, 0],
        gateway: [0; 4],
        dns: [0; 4],
        configured: false,
        dhcp: false,
        arp: BTreeMap::new(),
        tcp: Vec::new(),
        udp: VecDeque::new(),
        dns_cache: BTreeMap::new(),
        next_port: 49152,
        next_id: 1,
        ip_id: 1,
        ping: None,
        dhcp_xid: 0x5A45_5230 ^ crate::time::uptime_ms() as u32,
        dhcp_offer: None,
        rx_packets: 0,
        tx_packets: 0,
    });
    true
}

/// Handles pending packets and timers.
pub fn poll() {
    if let Some(mut guard) = STACK.try_lock() {
        if let Some(s) = guard.as_mut() {
            s.poll_inner();
        }
    }
}

/// Gets an address (DHCP, or the settings QEMU's built-in network uses) and
/// starts the background thread that keeps the stack running.
pub fn start() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    if !init() {
        return;
    }
    crate::sched::spawn("network", 256, || {
        configure();
        loop {
            poll();
            crate::sched::yield_idle();
        }
    });
}

fn configure() {
    // DHCP: discover, offer, request, acknowledge
    let start = crate::time::uptime_ms();
    let mut stage = 0;
    let mut last = 0u64;
    while crate::time::uptime_ms() < start + 4000 {
        let now = crate::time::uptime_ms();
        let done = with_stack(|s| {
            s.poll_inner();
            if s.configured {
                return true;
            }
            if now >= last + 1000 {
                last = now;
                if let (1, Some((ip, _, _, _, server))) = (stage, s.dhcp_offer) {
                    let m = s.dhcp_message(3, Some((ip, server)));
                    s.send_udp([0; 4], [255; 4], 68, 67, &m);
                } else if stage == 0 {
                    let m = s.dhcp_message(1, None);
                    s.send_udp([0; 4], [255; 4], 68, 67, &m);
                }
            }
            if s.dhcp_offer.is_some() {
                stage = 1;
            }
            false
        })
        .unwrap_or(true);
        if done {
            break;
        }
        crate::sched::yield_idle();
    }
    with_stack(|s| {
        if !s.configured {
            // QEMU's user-mode network
            s.ip = [10, 0, 2, 15];
            s.mask = [255, 255, 255, 0];
            s.gateway = [10, 0, 2, 2];
            s.dns = [10, 0, 2, 3];
            s.configured = true;
        }
        let gw = s.gateway;
        s.arp_request(gw);
    });
}

/// Whether the network is ready to use.
pub fn is_up() -> bool {
    with_stack(|s| s.configured && s.nic.link_up()).unwrap_or(false)
}

pub struct Info {
    pub mac: [u8; 6],
    pub ip: Ip,
    pub gateway: Ip,
    pub dns: Ip,
    pub dhcp: bool,
    pub up: bool,
    pub rx: u64,
    pub tx: u64,
}

pub fn info() -> Option<Info> {
    with_stack(|s| Info { mac: s.mac, ip: s.ip, gateway: s.gateway, dns: s.dns, dhcp: s.dhcp, up: s.configured && s.nic.link_up(), rx: s.rx_packets, tx: s.tx_packets })
}

/// Waits until `check` gives a value, driving the stack meanwhile.
fn wait_for<T>(timeout_ms: u64, mut check: impl FnMut(&mut Stack) -> Option<T>) -> Option<T> {
    let end = crate::time::uptime_ms() + timeout_ms;
    loop {
        if let Some(r) = with_stack(|s| {
            s.poll_inner();
            check(s)
        })
        .flatten()
        {
            return Some(r);
        }
        if crate::time::uptime_ms() >= end {
            return None;
        }
        crate::sched::yield_idle();
    }
}

/// Looks a name up (or reads a dotted address).
pub fn resolve(name: &str) -> Result<Ip, NetError> {
    if let Some(ip) = parse_ip(name) {
        return Ok(ip);
    }
    if !is_up() {
        // the background thread may still be configuring
        wait_for(5000, |s| if s.configured { Some(()) } else { None }).ok_or(NetError::NoNetwork)?;
    }
    let lower = name.to_ascii_lowercase();
    if let Some(ip) = with_stack(|s| s.dns_cache.get(&lower).copied()).flatten() {
        return Ok(ip);
    }
    let port = with_stack(|s| {
        s.next_port = s.next_port.wrapping_add(1).max(49152);
        s.next_port
    })
    .ok_or(NetError::NoNetwork)?;
    let id = (crate::time::uptime_ms() & 0xFFFF) as u16;
    let mut q = Vec::new();
    put16(&mut q, id);
    put16(&mut q, 0x0100);
    put16(&mut q, 1);
    put16(&mut q, 0);
    put16(&mut q, 0);
    put16(&mut q, 0);
    for label in lower.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(NetError::Dns(String::from(name)));
        }
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    put16(&mut q, 1);
    put16(&mut q, 1);

    for _attempt in 0..3 {
        with_stack(|s| {
            let (ip, dns) = (s.ip, s.dns);
            s.send_udp(ip, dns, port, 53, &q);
        });
        let answer = wait_for(1500, |s| {
            let pos = s.udp.iter().position(|(_, sp, dp, d)| *sp == 53 && *dp == port && d.len() > 12 && be16(d, 0) == id)?;
            let (_, _, _, d) = s.udp.remove(pos)?;
            Some(d)
        });
        if let Some(d) = answer {
            if let Some(ip) = parse_dns_answer(&d) {
                with_stack(|s| s.dns_cache.insert(lower.clone(), ip));
                return Ok(ip);
            }
            return Err(NetError::Dns(String::from(name)));
        }
    }
    Err(NetError::Timeout)
}

fn skip_name(d: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *d.get(i)? as usize;
        if len == 0 {
            return Some(i + 1);
        }
        if len & 0xC0 == 0xC0 {
            return Some(i + 2);
        }
        i += 1 + len;
    }
}

fn parse_dns_answer(d: &[u8]) -> Option<Ip> {
    let (qd, an) = (be16(d, 4) as usize, be16(d, 6) as usize);
    let mut i = 12;
    for _ in 0..qd {
        i = skip_name(d, i)? + 4;
    }
    for _ in 0..an {
        i = skip_name(d, i)?;
        if i + 10 > d.len() {
            return None;
        }
        let (ty, rdlen) = (be16(d, i), be16(d, i + 8) as usize);
        i += 10;
        if ty == 1 && rdlen == 4 && i + 4 <= d.len() {
            return Some([d[i], d[i + 1], d[i + 2], d[i + 3]]);
        }
        i += rdlen;
    }
    None
}

/// Sends a ping and waits for the answer; returns the round trip in milliseconds.
pub fn ping(ip: Ip) -> Option<u64> {
    let id = (crate::time::uptime_ms() & 0xFFFF) as u16 | 1;
    let mut p = alloc::vec![8u8, 0, 0, 0];
    put16(&mut p, id);
    put16(&mut p, 1);
    p.extend_from_slice(b"zero ping payload 0123456789abcd");
    let c = checksum(&[&p]);
    p[2..4].copy_from_slice(&c.to_be_bytes());
    let start = crate::time::uptime_ms();
    with_stack(|s| s.ping = Some(Pending { id, sent_at: start, reply_at: None }))?;
    for _ in 0..3 {
        let sent = with_stack(|s| s.send_ip(ip, 1, &p)).unwrap_or(false);
        let _ = sent;
        let r = wait_for(1000, |s| s.ping.as_ref().and_then(|p| p.reply_at.map(|r| r - p.sent_at)));
        if r.is_some() {
            with_stack(|s| s.ping = None);
            return r;
        }
    }
    with_stack(|s| s.ping = None);
    None
}

#[derive(Clone, Copy)]
pub struct Socket(usize);

/// Opens a TCP connection.
pub fn tcp_connect(ip: Ip, port: u16, timeout_ms: u64) -> Result<Socket, NetError> {
    if !is_up() {
        wait_for(5000, |s| if s.configured { Some(()) } else { None }).ok_or(NetError::NoNetwork)?;
    }
    let id = with_stack(|s| {
        s.next_port = s.next_port.wrapping_add(1).max(49152);
        let id = s.next_id;
        s.next_id += 1;
        let iss = (crate::time::uptime_ms() as u32).wrapping_mul(2654435761);
        s.tcp.retain(|c| c.state != State::Closed);
        s.tcp.push(Tcp {
            id,
            state: State::SynSent,
            local_port: s.next_port,
            remote_ip: ip,
            remote_port: port,
            snd_una: iss,
            snd_nxt: iss.wrapping_add(1),
            rcv_nxt: 0,
            tx: VecDeque::new(),
            rx: VecDeque::new(),
            peer_window: 1460,
            last_send: crate::time::uptime_ms(),
            retries: 0,
            want_close: false,
            fin_sent: false,
            reset: false,
        });
        let idx = s.tcp.len() - 1;
        s.send_tcp_segment(idx, 0x02, iss, &[]);
        id
    })
    .ok_or(NetError::NoNetwork)?;
    let result = wait_for(timeout_ms, |s| {
        let c = s.tcp.iter().find(|c| c.id == id)?;
        match c.state {
            State::Established => Some(Ok(())),
            State::Closed => Some(Err(NetError::Refused)),
            _ => None,
        }
    });
    match result {
        Some(Ok(())) => Ok(Socket(id)),
        Some(Err(e)) => Err(e),
        None => {
            tcp_close(Socket(id));
            Err(NetError::Timeout)
        }
    }
}

/// Queues data for sending (waits if the send queue is full).
pub fn tcp_send(sock: Socket, data: &[u8]) -> Result<(), NetError> {
    let mut sent = 0;
    while sent < data.len() {
        let n = wait_for(10_000, |s| {
            let idx = s.tcp.iter().position(|c| c.id == sock.0)?;
            let c = &mut s.tcp[idx];
            if c.state == State::Closed || c.reset {
                return Some(Err(NetError::Closed));
            }
            if !matches!(c.state, State::Established | State::CloseWait) {
                return Some(Err(NetError::Closed));
            }
            let room = 65536usize.saturating_sub(c.tx.len());
            if room == 0 {
                return None;
            }
            let n = room.min(data.len() - sent);
            c.tx.extend(data[sent..sent + n].iter());
            s.tcp_flush(idx);
            Some(Ok(n))
        })
        .ok_or(NetError::Timeout)??;
        sent += n;
    }
    Ok(())
}

pub enum Received {
    Data,
    /// The peer closed the connection and everything was read.
    Closed,
    Timeout,
}

/// Appends received bytes to `out`, waiting up to `timeout_ms` for some.
pub fn tcp_recv(sock: Socket, out: &mut Vec<u8>, timeout_ms: u64) -> Received {
    let r = wait_for(timeout_ms, |s| {
        let idx = s.tcp.iter().position(|c| c.id == sock.0)?;
        let c = &mut s.tcp[idx];
        if !c.rx.is_empty() {
            out.extend(c.rx.drain(..));
            // the window opened again
            let seq = c.snd_nxt;
            s.send_tcp_segment(idx, 0x10, seq, &[]);
            return Some(Received::Data);
        }
        match c.state {
            State::CloseWait | State::Closed | State::LastAck => Some(Received::Closed),
            _ => None,
        }
    });
    r.unwrap_or(Received::Timeout)
}

/// Closes a connection (after sending what is queued).
pub fn tcp_close(sock: Socket) {
    with_stack(|s| {
        if let Some(idx) = s.tcp.iter().position(|c| c.id == sock.0) {
            match s.tcp[idx].state {
                State::SynSent | State::Closed => s.tcp[idx].state = State::Closed,
                _ => {
                    s.tcp[idx].want_close = true;
                    s.tcp_flush(idx);
                }
            }
        }
    });
    // give the closing handshake a moment, then forget the connection
    let _ = wait_for(800, |s| {
        let idx = s.tcp.iter().position(|c| c.id == sock.0)?;
        if s.tcp[idx].state == State::Closed { Some(()) } else { None }
    });
    with_stack(|s| s.tcp.retain(|c| c.id != sock.0));
}
