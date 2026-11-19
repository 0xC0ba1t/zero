//! Driver for the Intel 82540EM ("e1000") network card, which is what QEMU
//! emulates by default. Packets move by DMA through two rings of
//! descriptors; this driver polls them (no interrupt).

use crate::{memory, pci};
use alloc::alloc::{alloc_zeroed, Layout};
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};

const CTRL: usize = 0x0000;
const STATUS: usize = 0x0008;
const IMC: usize = 0x00D8;
const RCTL: usize = 0x0100;
const TCTL: usize = 0x0400;
const TIPG: usize = 0x0410;
const RDBAL: usize = 0x2800;
const RDBAH: usize = 0x2804;
const RDLEN: usize = 0x2808;
const RDH: usize = 0x2810;
const RDT: usize = 0x2818;
const TDBAL: usize = 0x3800;
const TDBAH: usize = 0x3804;
const TDLEN: usize = 0x3808;
const TDH: usize = 0x3810;
const TDT: usize = 0x3818;
const MTA: usize = 0x5200;
const RAL0: usize = 0x5400;
const RAH0: usize = 0x5404;

const RX_COUNT: usize = 32;
const TX_COUNT: usize = 16;
const BUF: usize = 2048;

#[repr(C)]
#[derive(Clone, Copy)]
struct RxDesc {
    addr: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

pub struct E1000 {
    regs: *mut u8,
    rx: *mut RxDesc,
    tx: *mut TxDesc,
    rx_bufs: *mut u8,
    tx_bufs: *mut u8,
    rx_cur: usize,
    tx_cur: usize,
    pub mac: [u8; 6],
}

unsafe impl Send for E1000 {}

impl E1000 {
    fn read(&self, reg: usize) -> u32 {
        unsafe { read_volatile(self.regs.add(reg) as *const u32) }
    }

    fn write(&self, reg: usize, value: u32) {
        unsafe { write_volatile(self.regs.add(reg) as *mut u32, value) }
    }

    pub fn init() -> Option<E1000> {
        let dev = pci::find(&[(0x8086, 0x100E), (0x8086, 0x100F), (0x8086, 0x10D3), (0x8086, 0x153A), (0x8086, 0x1503)])?;
        pci::enable(&dev);
        let (phys, is_memory) = pci::bar(&dev, 0);
        if !is_memory {
            return None;
        }
        let regs = memory::map_mmio(phys, 0x20000)?;

        let ring_layout = Layout::from_size_align(4096, 4096).ok()?;
        let rx = unsafe { alloc_zeroed(ring_layout) } as *mut RxDesc;
        let tx = unsafe { alloc_zeroed(ring_layout) } as *mut TxDesc;
        let rx_bufs = unsafe { alloc_zeroed(Layout::from_size_align(RX_COUNT * BUF, 4096).ok()?) };
        let tx_bufs = unsafe { alloc_zeroed(Layout::from_size_align(TX_COUNT * BUF, 4096).ok()?) };
        if rx.is_null() || tx.is_null() || rx_bufs.is_null() || tx_bufs.is_null() {
            return None;
        }
        let mut nic = E1000 { regs, rx, tx, rx_bufs, tx_bufs, rx_cur: 0, tx_cur: 0, mac: [0; 6] };

        // reset, then switch the interrupts off (this driver polls)
        nic.write(CTRL, nic.read(CTRL) | 1 << 26);
        let t = crate::time::uptime_ms();
        while crate::time::uptime_ms() < t + 20 {
            core::hint::spin_loop();
        }
        nic.write(IMC, 0xFFFF_FFFF);
        // link up
        nic.write(CTRL, (nic.read(CTRL) | 1 << 6 | 1 << 5) & !(1 << 3));

        let ral = nic.read(RAL0);
        let rah = nic.read(RAH0);
        nic.mac = [ral as u8, (ral >> 8) as u8, (ral >> 16) as u8, (ral >> 24) as u8, rah as u8, (rah >> 8) as u8];
        nic.write(RAH0, (rah & 0xFFFF) | 1 << 31);
        for i in 0..128 {
            nic.write(MTA + i * 4, 0);
        }

        // receive ring
        for i in 0..RX_COUNT {
            let virt = unsafe { rx_bufs.add(i * BUF) } as u64;
            let phys = memory::virt_to_phys(virt)?;
            unsafe { write_volatile(rx.add(i), RxDesc { addr: phys, length: 0, checksum: 0, status: 0, errors: 0, special: 0 }) };
        }
        let rx_phys = memory::virt_to_phys(rx as u64)?;
        nic.write(RDBAL, rx_phys as u32);
        nic.write(RDBAH, (rx_phys >> 32) as u32);
        nic.write(RDLEN, (RX_COUNT * 16) as u32);
        nic.write(RDH, 0);
        nic.write(RDT, (RX_COUNT - 1) as u32);
        // enable, accept broadcast, strip the CRC, 2048 byte buffers
        nic.write(RCTL, 1 << 1 | 1 << 15 | 1 << 26);

        // transmit ring
        for i in 0..TX_COUNT {
            let virt = unsafe { tx_bufs.add(i * BUF) } as u64;
            let phys = memory::virt_to_phys(virt)?;
            unsafe { write_volatile(tx.add(i), TxDesc { addr: phys, length: 0, cso: 0, cmd: 0, status: 1, css: 0, special: 0 }) };
        }
        let tx_phys = memory::virt_to_phys(tx as u64)?;
        nic.write(TDBAL, tx_phys as u32);
        nic.write(TDBAH, (tx_phys >> 32) as u32);
        nic.write(TDLEN, (TX_COUNT * 16) as u32);
        nic.write(TDH, 0);
        nic.write(TDT, 0);
        nic.write(TCTL, 1 << 1 | 1 << 3 | 0x0F << 4 | 0x40 << 12);
        nic.write(TIPG, 0x0060_200A);
        Some(nic)
    }

    /// Whether the cable (virtual) is plugged in.
    pub fn link_up(&self) -> bool {
        self.read(STATUS) & 2 != 0
    }

    /// The next received frame, if one arrived.
    pub fn receive(&mut self) -> Option<Vec<u8>> {
        let desc = unsafe { read_volatile(self.rx.add(self.rx_cur)) };
        if desc.status & 1 == 0 {
            return None;
        }
        let len = (desc.length as usize).min(BUF);
        let mut frame = alloc::vec![0u8; len];
        unsafe { core::ptr::copy_nonoverlapping(self.rx_bufs.add(self.rx_cur * BUF), frame.as_mut_ptr(), len) };
        // give the descriptor back to the card
        unsafe {
            let d = self.rx.add(self.rx_cur);
            write_volatile(&mut (*d).status, 0);
        }
        self.write(RDT, self.rx_cur as u32);
        self.rx_cur = (self.rx_cur + 1) % RX_COUNT;
        Some(frame)
    }

    /// Sends a frame (without CRC). Returns false if the ring is full.
    pub fn transmit(&mut self, frame: &[u8]) -> bool {
        if frame.len() > BUF - 4 {
            return false;
        }
        let i = self.tx_cur;
        let desc = unsafe { read_volatile(self.tx.add(i)) };
        if desc.status & 1 == 0 {
            return false;
        }
        unsafe {
            core::ptr::copy_nonoverlapping(frame.as_ptr(), self.tx_bufs.add(i * BUF), frame.len());
            let d = self.tx.add(i);
            write_volatile(&mut (*d).length, frame.len().max(60) as u16);
            write_volatile(&mut (*d).status, 0);
            // end of packet, insert CRC, report status
            write_volatile(&mut (*d).cmd, 1 | 2 | 8);
        }
        self.tx_cur = (i + 1) % TX_COUNT;
        self.write(TDT, self.tx_cur as u32);
        true
    }
}
