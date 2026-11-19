//! PCI configuration space, through the classic I/O ports.

use x86_64::instructions::port::Port;

#[derive(Clone, Copy, Debug)]
pub struct Device {
    pub bus: u8,
    pub slot: u8,
    pub function: u8,
    pub vendor: u16,
    pub device: u16,
}

fn address(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000 | (bus as u32) << 16 | (slot as u32) << 11 | (function as u32) << 8 | (offset as u32 & 0xFC)
}

pub fn read32(d: &Device, offset: u8) -> u32 {
    x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        Port::<u32>::new(0xCF8).write(address(d.bus, d.slot, d.function, offset));
        Port::<u32>::new(0xCFC).read()
    })
}

pub fn write32(d: &Device, offset: u8, value: u32) {
    x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        Port::<u32>::new(0xCF8).write(address(d.bus, d.slot, d.function, offset));
        Port::<u32>::new(0xCFC).write(value);
    })
}

/// Finds the first device with one of the given (vendor, device) ids.
pub fn find(ids: &[(u16, u16)]) -> Option<Device> {
    for bus in 0..8u8 {
        for slot in 0..32u8 {
            for function in 0..8u8 {
                let probe = Device { bus, slot, function, vendor: 0, device: 0 };
                let id = read32(&probe, 0);
                if id & 0xFFFF == 0xFFFF {
                    if function == 0 {
                        break;
                    }
                    continue;
                }
                let (vendor, device) = ((id & 0xFFFF) as u16, (id >> 16) as u16);
                if ids.iter().any(|&(v, d)| v == vendor && d == device) {
                    return Some(Device { bus, slot, function, vendor, device });
                }
            }
        }
    }
    None
}

/// Turns on I/O and memory access and bus mastering (DMA) for a device.
pub fn enable(d: &Device) {
    let command = read32(d, 0x04) & 0xFFFF;
    write32(d, 0x04, command | 0x07);
}

/// The base address register `n` (0 to 5): (address, is_memory).
pub fn bar(d: &Device, n: u8) -> (u64, bool) {
    let raw = read32(d, 0x10 + n * 4);
    if raw & 1 == 1 {
        ((raw & !3) as u64, false)
    } else {
        let mut addr = (raw & !0xF) as u64;
        if (raw >> 1) & 3 == 2 {
            addr |= (read32(d, 0x14 + n * 4) as u64) << 32;
        }
        (addr, true)
    }
}
