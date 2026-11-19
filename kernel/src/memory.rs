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
                let start = self.next_addr.max((region.start + 4095) & !4095);
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
