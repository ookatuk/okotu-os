use alloc::boxed::Box;
use alloc::string::String;
use core::hint::{cold_path, likely, unlikely};
use spin::{Lazy, Once};
use x86_64::instructions::interrupts::without_interrupts;
use x86_64::instructions::segmentation::{Segment, CS};
use x86_64::instructions::tables::load_tss;
use x86_64::registers::segmentation::SegmentSelector;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable};
use x86_64::structures::idt::InterruptDescriptorTable;
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;
use crate::cpu::cpu_id;
use crate::thread_local::read_gs;

static CPU_VENDOR_CACHE: Once<[u8; 12]> = Once::new();

pub mod vendor_list {
    pub const INTEL: &[u8;12] = b"GenuineIntel";
    pub const AMD: &[u8;12] = b"AuthenticAMD";
    pub const VMWARE: &[u8; 12] = b"VMwareVMware";
    pub const KVM: &[u8; 12] = b"KVMKVMKVMKVM";
    pub const MICROSOFT_HYPER: &[u8; 12] = b"Microsoft Hv";
}

pub fn get_vendor_name_raw() -> Option<&'static [u8; 12]> {
    let vendor = CPU_VENDOR_CACHE.call_once(|| {
        cold_path();

        let res = unsafe { cpu_id::read(0, None) };
        let mut v = [0u8; 12];
        v[0..4].copy_from_slice(&res.ebx.to_ne_bytes());
        v[4..8].copy_from_slice(&res.edx.to_ne_bytes());
        v[8..12].copy_from_slice(&res.ecx.to_ne_bytes());

        v
    });

    if unlikely(core::str::from_utf8(vendor).is_err()) {
        return None;
    }

    Some(vendor)
}
#[inline]
pub fn get_vendor_name() -> Option<&'static str> {
    Some(unsafe{str::from_utf8_unchecked(
        get_vendor_name_raw()?
    )})
}

pub fn who_am_i() -> Option<u32> {
    let gs = crate::thread_local::read_gs()?;
    if likely(gs.cpu_id != 0) {
        return Some(gs.cpu_id);
    }

    let mut ret = 0;
    let vendor = get_vendor_name_raw()?;

    if vendor == vendor_list::AMD {
        let max_ext = unsafe { cpu_id::read(0x80000000, None).eax };
        if max_ext >= 0x8000001E {
            let res = unsafe { cpu_id::read(0x8000001E, None) };
            ret = res.eax;
        }
    } else if vendor == vendor_list::INTEL {
        let res = unsafe { cpu_id::read(0x0B, Some(0)) };
        if res.edx != 0 {
            ret = res.edx;
        }
    }

    if ret == 0 {
        let res = unsafe { cpu_id::read(1, None) };
        ret = (res.ebx >> 24) & 0xFF;
    }

    gs.cpu_id = ret;
    Some(ret)
}

fn make_gdt() -> GlobalDescriptorTable {
    let mut gdt = GlobalDescriptorTable::new();
    gdt.append(x86_64::structures::gdt::Descriptor::kernel_code_segment());
    gdt.append(x86_64::structures::gdt::Descriptor::kernel_data_segment());
    gdt.append(x86_64::structures::gdt::Descriptor::user_code_segment());
    gdt.append(x86_64::structures::gdt::Descriptor::user_data_segment());

    gdt
}

fn make_tss() -> TaskStateSegment {
    let mut tss = TaskStateSegment::new();
    let gs = read_gs().unwrap();

    let stack_start_ptr = gs.idt_stack.data[0].get_addr() as *const u8;
    let stack_start = VirtAddr::from_ptr(stack_start_ptr);

    let stack_end = stack_start + 20480u64;

    tss.interrupt_stack_table[crate::interrupt::raw::NMI_STACK_ADDR as usize] = stack_end;

    let stack_start_ptr = gs.idt_stack.data[1].get_addr() as *const u8;
    let stack_start = VirtAddr::from_ptr(stack_start_ptr);

    let stack_end = stack_start + 20480u64;

    tss.interrupt_stack_table[crate::interrupt::raw::DOUBLE_FAULT_STACK_ADDR as usize] = stack_end;

    tss
}

pub fn make() -> (&'static mut GlobalDescriptorTable, SegmentSelector) {
    let tss_ptr = Box::leak(Box::new(make_tss()));
    let gdt_ptr = Box::leak(Box::new(make_gdt()));

    let tss_desc = Descriptor::tss_segment(tss_ptr);

    let tss_selector = gdt_ptr.append(tss_desc);

    (gdt_ptr, tss_selector)
}

pub fn init_gdt() {
    let (gdt_ptr, tss_selector) = make();

    let code_selector = SegmentSelector::new(1, x86_64::PrivilegeLevel::Ring0);
    let data_selector = SegmentSelector::new(2, x86_64::PrivilegeLevel::Ring0);

    without_interrupts(|| {
        gdt_ptr.load();

        unsafe {
            CS::set_reg(code_selector);
            x86_64::instructions::segmentation::SS::set_reg(data_selector);
            x86_64::instructions::segmentation::DS::set_reg(data_selector);
            x86_64::instructions::segmentation::ES::set_reg(data_selector);
        }

        unsafe{load_tss(tss_selector)};
    });
}