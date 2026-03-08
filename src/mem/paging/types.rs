use crate::mem::types::MemData;
use crate::util::result;
use crate::util::result::{Error, ErrorType};
use alloc::vec::Vec;
use bitflags::bitflags;
use core::alloc::Layout;
use core::cmp::PartialEq;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use x86::bits64::paging::{
    PAddr, PDEntry, PDFlags, PDPTEntry, PDPTFlags, PML4Entry, PML4Flags, PML5Entry, PML5Flags,
    PTEntry, PTFlags,
};
use x86_64::registers::control::{Cr3, Cr4, Cr4Flags};
use x86_64::structures::paging::{PageTable, PageTableFlags};
use x86_64::{PhysAddr, VirtAddr};

const PT_SIZE: usize = 4096;
const PHY_OFFSET: usize = 0;

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
    pub fn to_pdptf(&self) -> PDPTFlags {
        let mut f = PDPTFlags::from_bits_truncate(self.apply_to_raw(0));
        if self.contains(Self::PAT) {
            f |= PDPTFlags::PAT;
        }
        if self.contains(Self::DIRTY) {
            f |= PDPTFlags::D;
        }
        if self.contains(Self::GLOBAL) {
            f |= PDPTFlags::G;
        }
        if self.contains(Self::HUGE) {
            f |= PDPTFlags::PS;
        }
        f
    }
    pub fn to_pdf(&self) -> PDFlags {
        let mut f = PDFlags::from_bits_truncate(self.apply_to_raw(0));
        if self.contains(Self::PAT) {
            f |= PDFlags::PAT;
        }
        if self.contains(Self::DIRTY) {
            f |= PDFlags::D;
        }
        if self.contains(Self::GLOBAL) {
            f |= PDFlags::G;
        }
        if self.contains(Self::HUGE) {
            f |= PDFlags::PS;
        }
        f
    }
    pub fn to_ptf(&self) -> PTFlags {
        let mut f = PTFlags::from_bits_truncate(self.apply_to_raw(0));
        if self.contains(Self::DIRTY) {
            f |= PTFlags::D;
        }
        if self.contains(Self::GLOBAL) {
            f |= PTFlags::G;
        }
        f
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
    pub fn get_index(&self, vaddr: VirtAddr) -> usize {
        let shift = (*self as u8) * 9 + 12;
        ((vaddr.as_u64() >> shift) & 0x1ff) as usize
    }
    pub fn down(&self) -> Option<PageLevel> {
        if *self == PageLevel::Pt {
            None
        } else {
            Self::from_u8(*self as u8 - 1)
        }
    }
}

pub struct TopPageTable {
    pub phys: PhysAddr,
    pub virt: VirtAddr,
    pub level: PageLevel,
    pub ptr: &'static mut PageTable,
}

pub fn create_page_table(
    map_list: &mut Vec<MemData<usize>>,
    flags: &mut Vec<PageEntryFlags>,
    allow_huge_at: PageLevel,
    target: PageLevel,
    _old: &mut PageTable,
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
            if !i.len.is_multiple_of(4096) || !i.start.is_multiple_of(4096) {
                return Error::new(
                    ErrorType::InvalidData,
                    Some("map data align/size align is need 4096, but not 4096"),
                )
                .raise();
            }
        }
    }

    normalize_map_list(map_list, flags);

    let (ptr, phys) = match target {
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
        ptr: unsafe { &mut *(ptr) },
    })
}

fn create_recursive(
    vaddr_base: usize,
    level: PageLevel,
    map_list: &Vec<MemData<usize>>,
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
                if !i.len.is_multiple_of(4096) || !i.start.is_multiple_of(4096) {
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
            let m_end = m.start + m.len;
            if m.start < range_vend && m_end > range_vstart {
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
            && sub_map[0].start <= range_vstart
            && (sub_map[0].start + sub_map[0].len) >= range_vend;

        if can_be_huge {
            let addr = range_vstart as u64;
            let raw_entry = match level {
                PageLevel::Pdpt => {
                    let f_orig = sub_flags[0].to_pdptf();
                    // PATビット(bit 7)をHugePage用のbit 12に移動させる処理
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
                create_recursive(range_vstart, next_level, &sub_map, &sub_flags, allow_huge)?;
            let phys_addr = next_table as u64 - PHY_OFFSET as u64;

            // 中間テーブルには RELAY_FLAGS を使用
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

pub fn normalize_map_list(map_list: &mut Vec<MemData<usize>>, flags: &mut Vec<PageEntryFlags>) {
    if map_list.is_empty() {
        return;
    }

    let original_maps: Vec<MemData<usize>> = map_list.drain(..).collect();
    let original_flags: Vec<PageEntryFlags> = flags.drain(..).collect();

    let mut final_list: Vec<(MemData<usize>, PageEntryFlags)> = Vec::new();

    for (new_range, new_flag) in original_maps.into_iter().zip(original_flags.into_iter()) {
        let mut j = 0;
        let new_start = new_range.start;
        let new_end = new_range.start + new_range.len;

        while j < final_list.len() {
            let (old_range, old_flag) = &final_list[j];
            let old_start = old_range.start;
            let old_end = old_range.start + old_range.len;

            if new_start < old_end && new_end > old_start {
                if old_start >= new_start && old_end <= new_end {
                    final_list.remove(j);
                    continue;
                } else if old_start < new_start && old_end > new_end {
                    let flag_copy = *old_flag;
                    final_list[j].0.len = new_start - old_start;
                    let split_range = MemData {
                        start: new_end,
                        len: old_end - new_end,
                    };
                    final_list.insert(j + 1, (split_range, flag_copy));
                    j += 1;
                } else if old_start < new_start {
                    final_list[j].0.len = new_start - old_start;
                } else {
                    final_list[j].0.start = new_end;
                    final_list[j].0.len = old_end - new_end;
                }
            }
            j += 1;
        }
        final_list.push((new_range, new_flag));
    }

    final_list.sort_by_key(|(map, _)| map.start);

    let mut merged: Vec<(MemData<usize>, PageEntryFlags)> = Vec::new();
    if let Some(first) = final_list.first().cloned() {
        let (mut curr_map, mut curr_flag) = first;
        for (next_map, next_flag) in final_list.into_iter().skip(1) {
            if (curr_map.start + curr_map.len) == next_map.start && curr_flag == next_flag {
                curr_map.len += next_map.len;
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
