use alloc::vec;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::addr_of;
use core::sync::atomic::{AtomicU8, Ordering};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::{PrivilegeLevel, VirtAddr};
use x86_64::structures::idt::{InterruptStackFrame, PageFaultErrorCode};
use lazy_static::lazy_static;
use spin::Once;
use x86::current::segmentation::swapgs;
use x86_64::registers::segmentation::SegmentSelector;
use crate::{log_error, log_info, log_last, ALLOC};
use crate::cpu::msr::read;
use crate::thread_local::read_gs;
use crate::util_types::SmartPtr;

pub(crate) const DOUBLE_FAULT_STACK_ADDR: u16 = 1;
pub(crate) const NMI_STACK_ADDR: u16 = 0;
pub const STACK_SIZE: usize = 20480;

pub static DATA: Once<Vec<NmiArgs>> = Once::new();

#[repr(u8)]
pub enum NmiCommand {
    None = 0,
    Panic = 1,
    Ping = 2,
}

pub struct NmiArgs(AtomicU8);

impl NmiArgs {
    pub fn store(&self, cmd: NmiCommand) {
        self.0.store(cmd as u8, Ordering::SeqCst);
    }

    pub fn load(&self) -> NmiCommand {
        match self.0.load(Ordering::SeqCst) {
            1 => NmiCommand::Panic,
            2 => NmiCommand::Ping,
            _ => NmiCommand::None,
        }
    }
}

#[repr(align(16))]
pub struct IdtRawStacks {
    pub(crate) data: Vec<SmartPtr<usize, crate::memory::physical_allocator::OsPhysicalAllocator>>
}

impl Default for IdtRawStacks {
    fn default() -> Self {
        let mut data = vec![];
        for _ in 0..2 {
            let layout = Layout::from_size_align(STACK_SIZE as usize, 16).unwrap();
            let allocated = unsafe{ALLOC.alloc(layout)};
            if allocated.is_null() {
                panic!("Failed to allocate memory");
            }
            data.push(SmartPtr::new(
                allocated.addr(),
                layout,
                &ALLOC
            ).unwrap())
        }

        Self {
            data,
        }
    }
}

extern "x86-interrupt" fn gp_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    if stack_frame.code_segment.rpl() == PrivilegeLevel::Ring0 {
        log_last!("kernel", "general protection fault", "gp raised.");

        let index = (error_code >> 3) & 0x1FFF;
        let ti = (error_code >> 2) & 1;
        let idt = (error_code >> 1) & 1;
        let ext = error_code & 1;

        log_last!("kernel", "gp detail", "Index: {}, TI: {}, IDT: {}, External: {}", index, ti, idt, ext);

        log_last!("kernel", "general protection fault", "{:?}", stack_frame);
        panic!("general protection fault");
    } else {
        log_error!("kernel", "general protection fault", "------ other segment error( {:?} ) ------", stack_frame.code_segment.rpl());
        log_error!("kernel", "general protection fault", "gp raised.");

        let index = (error_code >> 3) & 0x1FFF;
        let ti = (error_code >> 2) & 1;
        let idt = (error_code >> 1) & 1;
        let ext = error_code & 1;

        log_error!("kernel", "gp detail", "Index: {}, TI: {}, IDT: {}, External: {}", index, ti, idt, ext);

        log_error!("kernel", "general protection fault", "{:?}", stack_frame);
        log_error!("kernel", "general protection fault", "------ end ------");
    }
}

// ページフォルトの解析例
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    if error_code.contains(PageFaultErrorCode::USER_MODE) {
        log_error!("kernel", "page fault", "------ other page fault( {:?} ) ------", stack_frame.code_segment.rpl());
        log_error!("kernel", "page fault", "gf raised.");
        log_error!("kernel", "page fault", "Code: {:?}", error_code);

        log_error!("kernel", "page fault", "{:?}", stack_frame);
        let accessed_address = x86_64::registers::control::Cr2::read();
        log_error!("kernel", "page fault", "Accessed Address: {:?}", accessed_address);
        log_error!("kernel", "page fault", "------ end ------");


    } else {
        log_last!("kernel", "page fault", "gf raised.");
        log_last!("kernel", "page fault", "Code: {:?}", error_code);
        let accessed_address = x86_64::registers::control::Cr2::read();
        log_error!("kernel", "page fault", "Accessed Address: {:?}", accessed_address);


        log_last!("kernel", "page fault", "{:?}", stack_frame);
        panic!("page fault");
    }
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    log_last!("kernel", "double fault", "!!! DOUBLE FAULT !!!");
    log_last!("kernel", "double fault", "{:#?}", stack_frame);
    panic!("Double Fault (Probably Stack Overflow in Kernel)");
}

extern "x86-interrupt" fn invalid_opcode_handler(
    stack_frame: InterruptStackFrame,
) {
    if stack_frame.code_segment.rpl() == PrivilegeLevel::Ring0 {
        log_last!("kernel", "invalid opcode", "ud raised.");
        log_last!("kernel", "invalid opcode", "{:#?}", stack_frame);
        panic!("invalid opcode");
    } else {
        log_error!("kernel", "invalid opcode", "------ other invalid opcode( {:?} ) ------", stack_frame.code_segment.rpl());
        log_error!("kernel", "invalid opcode", "ud raised.");
        log_error!("kernel", "invalid opcode", "{:#?}", stack_frame);
        log_error!("kernel", "invalid opcode", "------ end ------");
    }
}

extern "x86-interrupt" fn divide_error_handler(
    stack_frame: InterruptStackFrame,
) {
    if stack_frame.code_segment.rpl() == PrivilegeLevel::Ring0 {
        log_last!("kernel", "divide error", "de raised.");
        log_last!("kernel", "divide error", "{:#?}", stack_frame);
        panic!("divide error");
    } else {
        log_error!("kernel", "divide error", "------ other divide error( {:?} ) ------", stack_frame.code_segment.rpl());
        log_error!("kernel", "divide error", "de raised.");
        log_error!("kernel", "divide error", "{:#?}", stack_frame);
        log_error!("kernel", "divide error", "------ end ------");
    }
}

extern "x86-interrupt" fn breakpoint_handler(
    stack_frame: InterruptStackFrame,
) {
    if stack_frame.code_segment.rpl() == PrivilegeLevel::Ring0 {
        log_last!("kernel", "breakpoint", "bp raised.");
        log_last!("kernel", "breakpoint", "{:#?}", stack_frame);
    } else {
        log_error!("kernel", "breakpoint", "------ other breakpoint( {:?} ) ------", stack_frame.code_segment.rpl());
        log_error!("kernel", "breakpoint", "bp raised.");
        log_error!("kernel", "breakpoint", "{:#?}", stack_frame);
        log_error!("kernel", "breakpoint", "------ end ------");
    }


}

extern "x86-interrupt" fn nmi_handler(
    stack_frame: InterruptStackFrame,
) {
    if stack_frame.code_segment.rpl() != PrivilegeLevel::Ring0 {
        unsafe{swapgs()};
    }

}

pub fn init() {
    let target = &mut read_gs().unwrap().idt_raw;

    unsafe {
        target.general_protection_fault.set_handler_fn(gp_handler).set_code_selector(SegmentSelector::new(
            1,
            PrivilegeLevel::Ring0
        ));
        target.page_fault.set_handler_fn(page_fault_handler).set_code_selector(SegmentSelector::new(
            1,
            PrivilegeLevel::Ring0
        ));
        target.double_fault.set_handler_fn(double_fault_handler).set_stack_index(DOUBLE_FAULT_STACK_ADDR).set_code_selector(SegmentSelector::new(
            1,
            PrivilegeLevel::Ring0
        ));
        target.divide_error.set_handler_fn(divide_error_handler).set_code_selector(SegmentSelector::new(
            1,
            PrivilegeLevel::Ring0
        ));
        target.invalid_opcode.set_handler_fn(invalid_opcode_handler).set_code_selector(SegmentSelector::new(
            1,
            PrivilegeLevel::Ring0
        ));
        target.breakpoint.set_handler_fn(breakpoint_handler).set_code_selector(SegmentSelector::new(
            1,
            PrivilegeLevel::Ring0
        ));
        target.non_maskable_interrupt.set_handler_fn(nmi_handler).set_stack_index(NMI_STACK_ADDR).set_code_selector(SegmentSelector::new(
            1,
            PrivilegeLevel::Ring0
        ));
    };

    target.load();
}