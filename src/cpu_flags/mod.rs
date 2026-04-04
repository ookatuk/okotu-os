#![allow(non_upper_case_globals)]

use core::arch::x86_64::{CpuidResult, __cpuid, __cpuid_count};
use spin::Lazy;
use x86_64::registers::control::Cr4;
use x86_64::registers::control::Cr4Flags;
use local_macros::define_cpu_flags;

static CPUID_1: Lazy<CpuidResult> = Lazy::new(|| {
    __cpuid(1)
});

/// CPUIDの7_0
static CPUID_7_0: Lazy<CpuidResult> = Lazy::new(|| {
    if *MAX_BASE_LEAF_SUPPORTED < 7 {
        CpuidResult{
            eax: 0,
            ebx: 0,
            ecx: 0,
            edx: 0,
        }
    } else {
        __cpuid_count(7, 0)
    }
});

static MAX_BASE_LEAF_SUPPORTED: Lazy<u32> = Lazy::new(|| {
    unsafe { crate::cpu::cpu_id::read(0, None) }.eax
});

static MAX_EXT_LEAF_SUPPORTED: Lazy<u32> = Lazy::new(|| {
    unsafe { crate::cpu::cpu_id::read(0x8000_0000, None) }.eax
});


define_cpu_flags! {
    current {
        paging {
            Pml5,
        },
    },
    environment {
        capabilities,
        cpuid {
            SupportLeaf7
        },
        paging {
            Pml5,
            PdptHuge,
            NX,
        },
        tsc {
            TscHardWareAdjust,
            InvariantTsc,
            Aux
        },
        apic {
            X2Supported
        }
    },
}

#[macro_export]
macro_rules! cpu_info {
    ($($name:tt)*) => {
        $crate::thread_local::read_gs()
            .map(|gs| {
                gs.internal_cpu_flag_cache
                    .has($crate::cpu_flags::flags::$($name)*)
            })
            .unwrap_or(false)
    };
}


macro_rules! define_vulnerability_check {
    ($name:ident, $bit_pos:expr, $is_affected_when:tt) => {
        fn $name() -> bool {
            if !cpu_info!(environment::capabilities) {
                return true;
            }

            let capabilities = unsafe { read_msr(0x10A) };
            let bit = 1u64 << $bit_pos;

            (capabilities & bit) $is_affected_when 0
        }
    };
}

pub fn raw_detect_flag_impl(kind: InternalFlagKind) -> bool {
    match kind {
        InternalFlagKind::environment_capabilities => is_environment_capabilities(),
        InternalFlagKind::environment_cpuid_SupportLeaf7 => is_environment_cpuid_support_leaf7(),
        InternalFlagKind::environment_paging_Pml5 => is_environment_pml5(),
        InternalFlagKind::environment_paging_PdptHuge => is_environment_pdpt_huge(),
        InternalFlagKind::current_paging_Pml5 => cpu_info!(environment::paging::Pml5) && is_current_pml5(),
        InternalFlagKind::environment_paging_NX => is_environment_paging_nx(),
        InternalFlagKind::environment_tsc_TscHardWareAdjust => is_environment_tsc_hardwareadjust(),
        InternalFlagKind::environment_tsc_InvariantTsc => is_environment_tsc_invariant_tsc(),
        InternalFlagKind::environment_tsc_Aux => is_environment_tsc_aux(),
        InternalFlagKind::environment_apic_X2Supported => is_environment_apic_x2_supported(),
    }
}

fn is_environment_apic_x2_supported() -> bool {
    let info = unsafe { crate::cpu::cpu_id::read(0x01, None) };
    (info.ecx & (1 << 21)) != 0
}

fn is_environment_tsc_aux() -> bool {
    if *MAX_EXT_LEAF_SUPPORTED < 0x8000_0001 {
        return false;
    }

    let info = unsafe { crate::cpu::cpu_id::read(0x8000_0007, None) };

    (info.edx & (1 << 27)) != 0
}

fn is_environment_tsc_invariant_tsc() -> bool {
    if *MAX_EXT_LEAF_SUPPORTED < 0x8000_0007 {
        return false;
    }

    let info = unsafe { crate::cpu::cpu_id::read(0x8000_0007, None) };

    (info.edx & (1 << 8)) != 0
}

fn is_environment_tsc_hardwareadjust() -> bool {
    if *MAX_BASE_LEAF_SUPPORTED >= 0x07 {
        let res = unsafe { crate::cpu::cpu_id::read(0x07, Some(0)) };
        (res.ebx & (1 << 1)) != 0
    } else {
        false
    }
}

fn is_environment_paging_nx() -> bool {
    if *MAX_EXT_LEAF_SUPPORTED < 0x8000_0001 {
        return false;
    }

    let info = unsafe { crate::cpu::cpu_id::read(0x8000_0001, None) };

    (info.edx & (1 << 20)) != 0
}

fn is_environment_capabilities() -> bool {
    if !cpu_info!(environment::cpuid::SupportLeaf7) {return false;}
    (CPUID_7_0.edx & (1 << 29)) != 0
}

fn is_environment_pdpt_huge() -> bool {
    if *MAX_EXT_LEAF_SUPPORTED < 0x8000_0001 {
        return false;
    }

    let info = unsafe { crate::cpu::cpu_id::read(0x8000_0001, None) };

    (info.edx & (1 << 26)) != 0
}

fn is_environment_cpuid_support_leaf7() -> bool {
    *MAX_BASE_LEAF_SUPPORTED >= 7
}

fn is_environment_pml5() -> bool {
    if !cpu_info!(environment::cpuid::SupportLeaf7) { return false; }
    (CPUID_7_0.ecx & (1 << 16)) != 0
}

fn is_current_pml5() -> bool {
    let cr4 = Cr4::read();
    cr4.contains(Cr4Flags::L5_PAGING)
}