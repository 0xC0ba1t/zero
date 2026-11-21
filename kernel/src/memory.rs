use x86_64::{
    structures::paging::PageTable,
    structures::paging::OffsetPageTable,
    VirtAddr,
    PhysAddr,
    structures::paging::{Page, PhysFrame, Mapper, Size4KiB, FrameAllocator}
};
use bootloader_api::info::{MemoryRegions, MemoryRegionKind};

/// Initialize a new OffsetPageTable.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must be only called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
 
pub unsafe fn init(physical_memory_offset: VirtAddr) ->OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

/// returns a mutable reference to the active lvl 4 table
/// 
/// this func is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this func must be only called once
/// to avoid aliasing `&mut` references (which is undefined behaviour).

// make private

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr)
    -> &'static mut PageTable
{
    use x86_64::registers::control::Cr3;
    
    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr // unsafe
}

/// Creates an example mapping for the given page to frame `0xb8000`.

pub fn create_example_mapping(
    page: Page,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let frame = PhysFrame::containing_address(PhysAddr::new(0xb8000));
    let flags = Flags::PRESENT | Flags::WRITABLE;

    let map_to_result = unsafe {
        // FIXME: this is not safe, we do it only for testing
        mapper.map_to(page, frame, flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();
}

/// A FrameAllocator that always returns `None`.

pub struct EmptyFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        None
    }
}

/// A FrameAllocator that returns usable frames from the bootloader's memory map.

pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryRegions,
    /// Index of the region currently being handed out.
    region: usize,
    /// Physical address of the next free frame in that region.
    next_addr: u64,
}

impl BootInfoFrameAllocator {
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryRegions) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            region: 0,
            next_addr: 0,
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    /// Hands out the next usable frame in constant time. (The old version
    /// re-walked the whole memory map for every frame, which made mapping a
    /// big heap quadratic.)
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        loop {
            let region = self.memory_map.get(self.region)?;
            if region.kind == MemoryRegionKind::Usable {
                // never hand out the first megabyte: the other processors start up there (see `smp`)
                let start = self.next_addr.max((region.start + 4095) & !4095).max(0x10_0000);
                if start + 4096 <= region.end {
                    self.next_addr = start + 4096;
                    return Some(PhysFrame::containing_address(PhysAddr::new(start)));
                }
            }
            self.region += 1;
            self.next_addr = 0;
        }
    }
}

/// Where all physical memory is mapped in virtual memory (set at boot); drivers
/// that hand physical addresses to hardware use it to look pages up.
pub static PHYS_OFFSET: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

// ---------------------------------------------------------------- device memory

use spin::Mutex;

/// The page tables and frame source, kept for mapping device memory later.
pub static MAPPER: Mutex<Option<OffsetPageTable<'static>>> = Mutex::new(None);
pub static FRAMES: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);

/// Next free virtual address for device mappings.
static MMIO_NEXT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0x_5555_0000_0000);

/// Keeps the page tables for later (call once at boot, after the heap exists).
/// Whether physical page `addr` is ordinary free memory according to the boot memory map.
pub fn page_is_free(addr: u64) -> bool {
    FRAMES.lock().as_ref().map(|f| {
        f.memory_map.iter().any(|r| r.kind == MemoryRegionKind::Usable && r.start <= addr && addr + 4096 <= r.end)
    }).unwrap_or(false)
}

pub fn install(mapper: OffsetPageTable<'static>, frames: BootInfoFrameAllocator) {
    *MAPPER.lock() = Some(mapper);
    *FRAMES.lock() = Some(frames);
}

/// Maps `bytes` of device registers at physical address `phys` (uncached)
/// and returns where they are.
pub fn map_mmio(phys: u64, bytes: usize) -> Option<*mut u8> {
    use x86_64::structures::paging::PageTableFlags as F;
    let start = phys & !4095;
    let end = (phys + bytes as u64 + 4095) & !4095;
    let pages = ((end - start) / 4096) as u64;
    let virt = MMIO_NEXT.fetch_add(pages * 4096, core::sync::atomic::Ordering::SeqCst);
    let mut mapper = MAPPER.lock();
    let mut frames = FRAMES.lock();
    let (mapper, frames) = (mapper.as_mut()?, frames.as_mut()?);
    for i in 0..pages {
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt + i * 4096));
        let frame = PhysFrame::containing_address(PhysAddr::new(start + i * 4096));
        let flags = F::PRESENT | F::WRITABLE | F::NO_CACHE | F::WRITE_THROUGH | F::NO_EXECUTE;
        unsafe { mapper.map_to(page, frame, flags, frames).ok()?.flush() };
    }
    Some((virt + (phys - start)) as *mut u8)
}

/// The physical address behind a virtual one (for handing buffers to hardware).
pub fn virt_to_phys(virt: u64) -> Option<u64> {
    use core::sync::atomic::Ordering;
    let offset = PHYS_OFFSET.load(Ordering::Relaxed);
    let (frame, _) = x86_64::registers::control::Cr3::read();
    let mut table = frame.start_address().as_u64();
    for shift in [39u32, 30, 21, 12] {
        let index = (virt >> shift) & 0x1FF;
        let entry = unsafe { ((offset + table + index * 8) as *const u64).read_volatile() };
        if entry & 1 == 0 {
            return None;
        }
        let address = entry & 0x000F_FFFF_FFFF_F000;
        if shift == 12 || entry & 0x80 != 0 {
            let low = (1u64 << shift) - 1;
            return Some((address & !low) | (virt & low));
        }
        table = address;
    }
    None
}
