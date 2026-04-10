use alloc::alloc::dealloc;
use alloc::vec::Vec;
use bitflags::bitflags;
use core::alloc::Layout;
use core::cmp::PartialEq;
use core::hint::unlikely;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use rhai::CustomType;
use x86::bits64::paging::{
    PAddr, PDEntry, PDFlags, PDPTEntry, PDPTFlags, PML4Entry, PML4Flags, PML5Entry, PML5Flags,
    PTEntry, PTFlags,
};
use x86::tlb::flush_all;
use x86_64::registers::control::{Cr3, Cr3Flags, Cr4, Cr4Flags};
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame};
use x86_64::{PhysAddr, VirtAddr};
use crate::{result};
use crate::result::{Error, ErrorType};
use crate::util_types::MemRangeData;

pub const PHY_OFFSET: usize = 0;

const RELAY_FLAGS: PageEntryFlags = PageEntryFlags::from_bits_retain(
    PageEntryFlags::PRESENT.bits()
        | PageEntryFlags::WRITABLE.bits()
        | PageEntryFlags::USER_PAGE.bits(),
);

bitflags! {
    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    pub struct PageEntryFlags: u16 {
        const PRESENT         = 1 << 0;
        const WRITABLE        = 1 << 1;
        const USER_PAGE       = 1 << 2;
        const PWT             = 1 << 3;
        const PCD             = 1 << 4;
        const ACCESSED        = 1 << 5;
        const DIRTY           = 1 << 6;
        const PAT             = 1 << 7;
        const GLOBAL          = 1 << 8;
        const EXECUTE_DISABLE = 1 << 9;
        const HUGE            = 1 << 10;
    }
}

impl PageEntryFlags {
    const fn apply_to_raw(&self, mut raw_bits: u64) -> u64 {
        if self.contains(Self::PRESENT) {
            raw_bits |= 1 << 0;
        }
        if self.contains(Self::WRITABLE) {
            raw_bits |= 1 << 1;
        }
        if self.contains(Self::USER_PAGE) {
            raw_bits |= 1 << 2;
        }
        if self.contains(Self::PWT) {
            raw_bits |= 1 << 3;
        }
        if self.contains(Self::PCD) {
            raw_bits |= 1 << 4;
        }
        if self.contains(Self::ACCESSED) {
            raw_bits |= 1 << 5;
        }
        if self.contains(Self::EXECUTE_DISABLE) {
            raw_bits |= 1u64 << 63;
        }
        raw_bits
    }
    pub const fn to_pml5f(&self) -> PML5Flags {
        PML5Flags::from_bits_truncate(self.apply_to_raw(0))
    }
    pub const fn to_pml4f(&self) -> PML4Flags {
        PML4Flags::from_bits_truncate(self.apply_to_raw(0))
    }
    pub const fn to_pdptf(&self) -> PDPTFlags {
        let mut bits = self.apply_to_raw(0);

        if self.contains(Self::PAT) {
            bits |= PDPTFlags::PAT.bits();
        }
        if self.contains(Self::DIRTY) {
            bits |= PDPTFlags::D.bits();
        }
        if self.contains(Self::GLOBAL) {
            bits |= PDPTFlags::G.bits();
        }
        if self.contains(Self::HUGE) {
            bits |= PDPTFlags::PS.bits();
        }

        PDPTFlags::from_bits_truncate(bits)
    }

    pub const fn to_pdf(&self) -> PDFlags {
        let mut f = self.apply_to_raw(0);
        if self.contains(Self::PAT) {
            f |= PDFlags::PAT.bits();
        }
        if self.contains(Self::DIRTY) {
            f |= PDFlags::D.bits();
        }
        if self.contains(Self::GLOBAL) {
            f |= PDFlags::G.bits();
        }
        if self.contains(Self::HUGE) {
            f |= PDFlags::PS.bits();
        }

        PDFlags::from_bits_truncate(f)
    }
    pub const fn to_ptf(&self) -> PTFlags {
        let mut f = self.apply_to_raw(0);
        if self.contains(Self::DIRTY) {
            f |= PTFlags::D.bits();
        }
        if self.contains(Self::GLOBAL) {
            f |= PTFlags::G.bits();
        }

        PTFlags::from_bits_truncate(f)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, FromPrimitive)]
#[repr(u8)]
pub enum PageLevel {
    Pt = 0,
    Pd = 1,
    Pdpt = 2,
    Pml4 = 3,
    Pml5 = 4,
}

impl PageLevel {
    pub const fn get_index(&self, vaddr: VirtAddr) -> usize {
        let shift = (*self as u8) * 9 + 12;
        ((vaddr.as_u64() >> shift) & 0x1ff) as usize
    }
    pub const fn down(&self) -> Option<PageLevel> {
        match self {
            PageLevel::Pt => None,
            _ => Some(unsafe { core::mem::transmute::<u8, PageLevel>(*self as u8 - 1) }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TopPageTable {
    pub phys: PhysAddr,
    pub virt: VirtAddr,
    pub level: PageLevel,
    pub page_fragmentation_level: usize,
    pub memory_mapping: (Vec<MemRangeData<usize>>, Vec<PageEntryFlags>),
}

impl Default for TopPageTable {
    fn default() -> Self {
        Self {
            phys: PhysAddr::zero(),
            virt: VirtAddr::zero(),
            level: PageLevel::Pt,
            memory_mapping: (Vec::new(), Vec::new()),
            page_fragmentation_level: 0
        }
    }
}

impl CustomType for TopPageTable {
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder.with_set("set_addr", |me: &mut Self, value: i64| {
            let virt_addr = VirtAddr::new(value as u64);
            me.virt = virt_addr;
        });
    }
}

impl TopPageTable {
    pub fn ptr(&self) -> &'static mut PageTable {
        unsafe { &mut *(self.virt.as_mut_ptr()) }
    }
}

pub fn create_page_table(
    map_list: &mut Vec<MemRangeData<usize>>,
    flags: &mut Vec<PageEntryFlags>,
    allow_huge_at: PageLevel,
    target: PageLevel,
) -> result::Result<TopPageTable> {
    #[cfg(feature = "enable_normal_safety_checks")]
    {
        if map_list.len() != flags.len() {
            return Error::new(
                ErrorType::InvalidData,
                Some("map_list length is not eq to flags"),
            )
                .raise();
        };

        for i in map_list.iter() {
            if !i.len().is_multiple_of(4096) || !i.start().is_multiple_of(4096) {
                return Error::new(
                    ErrorType::InvalidData,
                    Some("map data align/size align is need 4096, but not 4096"),
                )
                    .raise();
            }
        }
    }

    normalize_map_list(map_list, flags);

    let (_, phys) = match target {
        PageLevel::Pml5 | PageLevel::Pml4 => {
            let table_addr = create_recursive(0, target, map_list, flags, allow_huge_at)?;
            (
                table_addr as *mut PageTable,
                PhysAddr::new(table_addr as u64 - PHY_OFFSET as u64),
            )
        }
        _ => return Error::new(ErrorType::InvalidData, Some("Invalid target level")).raise(),
    };

    Ok(TopPageTable {
        phys,
        virt: VirtAddr::new(phys.as_u64() + PHY_OFFSET as u64),
        level: target,
        memory_mapping: (map_list.to_vec(), flags.to_vec()),
        page_fragmentation_level: 0
    })
}

#[inline]
pub fn set_current(table: &TopPageTable) {
    unsafe{Cr3::write(
        PhysFrame::from_start_address(
            table.phys
        ).unwrap(),
        Cr3Flags::empty()
    )};
    unsafe{flush_all()};
}

fn create_recursive(
    vaddr_base: usize,
    level: PageLevel,
    map_list: &Vec<MemRangeData<usize>>,
    flags: &Vec<PageEntryFlags>,
    allow_huge: PageLevel,
) -> result::Result<usize> {
    let layout = Layout::from_size_align(4096, 4096).unwrap();
    let table_ptr = unsafe {
        let p = alloc::alloc::alloc_zeroed(layout);
        if p.is_null() {
            return Error::new(ErrorType::AllocationFailed, Some("Allocation failed")).raise();
        }
        p as *mut u64
    };

    let shift = (level as u8) * 9 + 12;
    let entry_size = 1usize << shift;

    for idx in 0..512 {
        #[cfg(feature = "enable_overprotective_safety_checks")]
        {
            if map_list.len() != flags.len() {
                return Error::new(
                    ErrorType::InvalidData,
                    Some("map_list length is not eq to flags"),
                )
                    .raise();
            };

            for i in map_list {
                if !i.len().is_multiple_of(4096) || !i.start().is_multiple_of(4096) {
                    return Error::new(
                        ErrorType::InvalidData,
                        Some("map data align/size align is need 4096, but not 4096"),
                    )
                        .raise();
                }
            }
        }

        let range_vstart = vaddr_base + (idx * entry_size);
        let range_vend = range_vstart + entry_size;

        let mut sub_map = Vec::new();
        let mut sub_flags = Vec::new();
        for (m, f) in map_list.iter().zip(flags.iter()) {
            if m.start() < range_vend && m.end() > range_vstart {
                sub_map.push(m.clone());
                sub_flags.push(*f);
            }
        }

        if sub_map.is_empty() {
            continue;
        }

        let can_be_huge = level != PageLevel::Pt
            && level <= allow_huge
            && sub_map.len() == 1
            && sub_map[0].start() <= range_vstart
            && (sub_map[0].end()) >= range_vend;

        if can_be_huge {
            let addr = range_vstart as u64;
            let raw_entry = match level {
                PageLevel::Pdpt => {
                    let f_orig = sub_flags[0].to_pdptf();
                    let mut f_bits = f_orig.bits();
                    if (f_bits & (1 << 7)) != 0 {
                        f_bits = (f_bits & !(1 << 7)) | (1 << 12);
                    }
                    let f = PDPTFlags::from_bits_truncate(f_bits);
                    PDPTEntry::new(PAddr::from(addr), f | PDPTFlags::PS | PDPTFlags::P).0
                }
                PageLevel::Pd => {
                    let f_orig = sub_flags[0].to_pdf();
                    let mut f_bits = f_orig.bits();
                    if (f_bits & (1 << 7)) != 0 {
                        f_bits = (f_bits & !(1 << 7)) | (1 << 12);
                    }
                    let f = PDFlags::from_bits_truncate(f_bits);
                    PDEntry::new(PAddr::from(addr), f | PDFlags::PS | PDFlags::P).0
                }
                _ => 0,
            };
            unsafe {
                *table_ptr.add(idx) = raw_entry;
            }
        } else if let Some(next_level) = level.down() {
            let next_table =
                {
                    let a = create_recursive(range_vstart, next_level, &sub_map, &sub_flags, allow_huge);
                    if unlikely(a.is_err()) {
                        unsafe{alloc::alloc::dealloc(
                            table_ptr as *mut u8,
                            layout
                        )}

                        let err = a.err().unwrap();
                        return Error::try_raise(
                            Err(err),
                            Some("failed to create recursive")
                        );
                    }
                    unsafe{a.unwrap_unchecked()}
                };
            let phys_addr = next_table as u64 - PHY_OFFSET as u64;

            let raw_entry = match level {
                PageLevel::Pml5 => {
                    PML5Entry::new(
                        PAddr::from(phys_addr),
                        RELAY_FLAGS.to_pml5f() | PML5Flags::P,
                    )
                        .0
                }
                PageLevel::Pml4 => {
                    PML4Entry::new(
                        PAddr::from(phys_addr),
                        RELAY_FLAGS.to_pml4f() | PML4Flags::P,
                    )
                        .0
                }
                PageLevel::Pdpt => {
                    PDPTEntry::new(
                        PAddr::from(phys_addr),
                        RELAY_FLAGS.to_pdptf() | PDPTFlags::P,
                    )
                        .0
                }
                PageLevel::Pd => {
                    PDEntry::new(PAddr::from(phys_addr), RELAY_FLAGS.to_pdf() | PDFlags::P).0
                }
                _ => 0,
            };
            unsafe {
                *table_ptr.add(idx) = raw_entry;
            }
        } else if level == PageLevel::Pt {
            let addr = range_vstart as u64;
            let raw_entry = PTEntry::new(PAddr::from(addr), sub_flags[0].to_ptf() | PTFlags::P).0;
            unsafe {
                *table_ptr.add(idx) = raw_entry;
            }
        }
    }

    Ok(table_ptr as usize)
}

pub fn normalize_map_list(map_list: &mut Vec<MemRangeData<usize>>, flags: &mut Vec<PageEntryFlags>) {
    if map_list.is_empty() {
        return;
    }

    let original_maps: Vec<MemRangeData<usize>> = map_list.drain(..).collect();
    let original_flags: Vec<PageEntryFlags> = flags.drain(..).collect();

    let mut final_list: Vec<(MemRangeData<usize>, PageEntryFlags)> = Vec::new();

    for (new_range, new_flag) in original_maps.into_iter().zip(original_flags.into_iter()) {
        let mut j = 0;
        let new_start = new_range.start();
        let new_end = new_range.end();

        while j < final_list.len() {
            let (old_range, old_flag) = &final_list[j];
            let old_start = old_range.start();
            let old_end = old_range.end();

            if new_start < old_end && new_end > old_start {
                if old_start >= new_start && old_end <= new_end {
                    final_list.remove(j);
                    continue;
                } else if old_start < new_start && old_end > new_end {
                    let flag_copy = *old_flag;
                    final_list[j].0.set_len(new_start - old_start);
                    let split_range = MemRangeData::new(new_end, old_end - new_end);
                    final_list.insert(j + 1, (split_range, flag_copy));
                    j += 1;
                } else if old_start < new_start {
                    final_list[j].0.set_len(new_start - old_start);
                } else {
                    final_list[j].0.set_start(new_end);
                    final_list[j].0.set_len(old_end - new_end);
                }
            }
            j += 1;
        }
        final_list.push((new_range, new_flag));
    }

    final_list.sort_unstable_by_key(|(map, _)| map.start());

    let mut merged: Vec<(MemRangeData<usize>, PageEntryFlags)> = Vec::new();
    if let Some(first) = final_list.first().cloned() {
        let (mut curr_map, mut curr_flag) = first;
        for (next_map, next_flag) in final_list.into_iter().skip(1) {
            if curr_map.end() == next_map.start() && curr_flag == next_flag {
                curr_map.set_len(next_map.len() + curr_map.len());
            } else {
                merged.push((curr_map, curr_flag));
                curr_map = next_map;
                curr_flag = next_flag;
            }
        }
        merged.push((curr_map, curr_flag));
    }

    for (m, f) in merged {
        map_list.push(m);
        flags.push(f);
    }
}

pub fn get_addr(addr: VirtAddr) -> result::Result<PhysAddr> {
    let cr3 = Cr3::read().0;
    let l5 = Cr4::read().contains(Cr4Flags::L5_PAGING);
    let mut current_table_ptr =
        (cr3.start_address().as_u64() + PHY_OFFSET as u64) as *const PageTable;

    for lev_raw in (0u8..=if l5 { 4 } else { 3 }).rev() {
        let lev = PageLevel::from_u8(lev_raw).unwrap();
        let index = lev.get_index(addr);
        let entry = unsafe { &(&(*current_table_ptr))[index] };

        if !entry.flags().contains(PageTableFlags::PRESENT) {
            return Error::new(ErrorType::NotFound, Some("page table entry not present")).raise();
        }

        if lev == PageLevel::Pt || entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            let base_phy = entry.addr().as_u64();
            let shift = (lev_raw as u64) * 9 + 12;
            let mask = (1u64 << shift) - 1;
            return Ok(PhysAddr::new(base_phy + (addr.as_u64() & mask)));
        }
        current_table_ptr = (entry.addr().as_u64() + PHY_OFFSET as u64) as *const PageTable;
    }
    Error::new(ErrorType::NotFound, None).raise()
}

pub unsafe fn dealloc_all(map: TopPageTable) {
    let top = map.ptr();
    let level = map.level;

    unsafe{dealloc_recursive(top, level)};
}

unsafe fn dealloc_recursive(target: &mut PageTable, level: PageLevel) {
    if level != PageLevel::Pt {
        for i in target.iter_mut() {
            if !i.flags().contains(PageTableFlags::PRESENT) { continue; }

            if !i.flags().contains(PageTableFlags::HUGE_PAGE) {
                unsafe{dealloc_recursive(&mut *((i.addr().as_u64() + PHY_OFFSET as u64) as *mut PageTable), level.down().unwrap())};
            }
        }
    }
    unsafe{
        dealloc(
            target as *mut PageTable as *mut u8,
            Layout::from_size_align_unchecked(4096, 4096)
        )
    }
}

pub type UpdatePagingResults<'a> = Vec<(&'a mut PageTable, PageLevel)>;

pub fn update_paging(
    top: &mut TopPageTable,
    new_map_list: &mut Vec<MemRangeData<usize>>,
    new_flags: &mut Vec<PageEntryFlags>,
    allow_huge_at: PageLevel,
) -> result::Result<UpdatePagingResults<'static>> {
    normalize_map_list(new_map_list, new_flags);

    let mut buf: UpdatePagingResults  = Vec::new();

     update_recursive(
        top.virt.as_u64() as usize,
        0,
        top.level,
        new_map_list,
        new_flags,
        allow_huge_at,
        &mut buf,
    )?;

    top.memory_mapping = (new_map_list.to_vec(), new_flags.to_vec());

    Ok(buf)
}

pub unsafe fn free_not_used_paging(dealloc_target: UpdatePagingResults) {
    for i in dealloc_target {
        unsafe{dealloc_recursive(i.0, i.1)};
    }
}

fn update_recursive(
    table_ptr: usize,
    vaddr_base: usize,
    level: PageLevel,
    map_list: &Vec<MemRangeData<usize>>,
    flags: &Vec<PageEntryFlags>,
    allow_huge: PageLevel,
    tmp_vec: &mut UpdatePagingResults,
) -> result::Result<()> {
    fn generate_relay_entry(level: PageLevel, phys: u64) -> u64 {
        match level {
            PageLevel::Pml5 => PML5Entry::new(PAddr::from(phys), RELAY_FLAGS.to_pml5f() | PML5Flags::P).0,
            PageLevel::Pml4 => PML4Entry::new(PAddr::from(phys), RELAY_FLAGS.to_pml4f() | PML4Flags::P).0,
            PageLevel::Pdpt => PDPTEntry::new(PAddr::from(phys), RELAY_FLAGS.to_pdptf() | PDPTFlags::P).0,
            PageLevel::Pd   => PDEntry::new(PAddr::from(phys), RELAY_FLAGS.to_pdf() | PDFlags::P).0,
            _ => 0,
        }
    }


    fn generate_huge_entry(level: PageLevel, vaddr: u64, flags: PageEntryFlags) -> u64 {
        match level {
            PageLevel::Pdpt => {
                let f = flags.to_pdptf();
                PDPTEntry::new(PAddr::from(vaddr), f | PDPTFlags::PS | PDPTFlags::P).0
            }
            PageLevel::Pd => {
                let f = flags.to_pdf();
                PDEntry::new(PAddr::from(vaddr), f | PDFlags::PS | PDFlags::P).0
            }
            _ => 0,
        }
    }

    let table = unsafe { &mut *(table_ptr as *mut [u64; 512]) };
    let shift = (level as u8) * 9 + 12;
    let entry_size = 1usize << shift;

    for idx in 0..512 {
        let range_vstart = vaddr_base + (idx * entry_size);
        let range_vend = range_vstart + entry_size;

        let mut sub_map = Vec::new();
        let mut sub_flags = Vec::new();
        for (m, f) in map_list.iter().zip(flags.iter()) {
            if m.start() < range_vend && m.end() > range_vstart {
                sub_map.push(m.clone());
                sub_flags.push(*f);
            }
        }

        let old_entry = table[idx];
        let old_present = (old_entry & 1) != 0;
        let old_huge = (old_entry & (1 << 7)) != 0;

        if sub_map.is_empty() {
            if old_present {
                if !old_huge && level != PageLevel::Pt {
                    let next_ptr = (old_entry & 0x000F_FFFF_FFFF_F000) + PHY_OFFSET as u64;
                    tmp_vec.push((unsafe{&mut *(next_ptr as *mut PageTable)}, level.down().unwrap()));
                }
                table[idx] = 0;
            }
            continue;
        }

        let can_be_huge = level != PageLevel::Pt
            && level <= allow_huge
            && sub_map.len() == 1
            && sub_map[0].start() <= range_vstart
            && sub_map[0].end() >= range_vend;

        if can_be_huge {
            let new_entry = generate_huge_entry(level, range_vstart as u64, sub_flags[0]);
            if old_entry != new_entry {
                if old_present && !old_huge {
                    let next_ptr = (old_entry & 0x000F_FFFF_FFFF_F000) + PHY_OFFSET as u64;
                    tmp_vec.push((unsafe{&mut *(next_ptr as *mut PageTable)}, level.down().unwrap()));
                }
                table[idx] = new_entry;
            }
            continue;
        }

        if let Some(next_level) = level.down() {
            if old_present && !old_huge {
                let next_ptr = (old_entry & 0x000F_FFFF_FFFF_F000) + PHY_OFFSET as u64;
                update_recursive(next_ptr as usize, range_vstart, next_level, &sub_map, &sub_flags, allow_huge, tmp_vec)?;
                table[idx] = generate_relay_entry(level, (next_ptr as u64) - PHY_OFFSET as u64);
            } else {
                let next_ptr = create_recursive(range_vstart, next_level, &sub_map, &sub_flags, allow_huge)?;
                table[idx] = generate_relay_entry(level, (next_ptr as u64) - PHY_OFFSET as u64);
            }
        } else if level == PageLevel::Pt {
            let new_entry = PTEntry::new(PAddr::from(range_vstart as u64), sub_flags[0].to_ptf() | PTFlags::P).0;
            table[idx] = new_entry;
        }
    }
    Ok(())
}