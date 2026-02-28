use crate::cpu::utils;
use alloc::string::String;
use core::arch::asm;
use core::arch::x86_64::{__cpuid_count, CpuidResult};

#[derive(Debug, Clone)]
pub enum CpuVendor {
    Intel,
    Amd,
    Other,
}

pub mod msr {
    pub mod common {
        pub const GS_BASE: u32 = 0xC0000101;
        pub const KERNEL_GS_BASE: u32 = 0xC0000102;
    }

    #[cfg(target_arch = "x86_64")]
    pub mod x64 {
        pub mod intel {
            pub const IA32_BIOS_SIGN_ID: u32 = 0x8B;
            pub const IA32_BIOS_UPDT_TRIG: u32 = 0x79;
            pub const IA32_PLATFORM_ID: u32 = 0x79;
        }

        pub mod amd {
            pub const UCODE_PATCH_LOADER: u32 = 0xC001_0020;
        }

        pub mod v1 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v1 instead of common directly")]
            pub use super::super::common::*;

            #[allow(unused_imports)]
            #[deprecated(note = "Use v1 instead of intel directly")]
            pub use super::intel::*;

            #[allow(unused_imports)]
            #[deprecated(note = "Use v1 instead of amd directly")]
            pub use super::amd::*;
        }

        pub mod v2 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v2 instead of v1 directly")]
            pub use super::v1::*;
        }

        pub mod v3 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v3 instead of v2 directly")]
            pub use super::v2::*;
        }

        pub mod v4 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v4 instead of v3 directly")]
            pub use super::v3::*;
        }
    }
}

pub mod cpuid {
    pub mod common {
        /// Processor Info and Feature Bits
        pub const PIAFB: u32 = 0x1;

        /// Vendor ID and Largest Standard Function Number
        pub const VIALSFN: u32 = 0x0;
    }

    #[cfg(target_arch = "x86_64")]
    pub mod x64 {
        pub mod intel {}

        pub mod amd {}

        pub mod v1 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v1 instead of common directly")]
            pub use super::super::common::*;

            #[allow(unused_imports)]
            #[deprecated(note = "Use v1 instead of intel directly")]
            pub use super::intel::*;

            #[allow(unused_imports)]
            #[deprecated(note = "Use v1 instead of amd directly")]
            pub use super::amd::*;
        }

        pub mod v2 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v2 instead of v1 directly")]
            pub use super::v1::*;

            pub const X2_APIC_ID: u32 = 0x0B;
        }

        pub mod v3 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v3 instead of v2 directly")]
            pub use super::v2::*;
        }

        pub mod v4 {
            #[allow(unused_imports)]
            #[deprecated(note = "Use v4 instead of v3 directly")]
            pub use super::v3::*;
        }
    }
}

#[inline]
pub unsafe fn write_msr(target: u32, value: u64) {
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") target,
            in("eax") value & 0xFFFF_FFFF,
            in("edx") value >> 32,
            options(nostack, preserves_flags, nomem)
        )
    };
}

#[inline]
pub unsafe fn read_msr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nostack, preserves_flags, nomem)
        )
    };

    ((high as u64) << 32) | (low as u64)
}

#[inline]
pub unsafe fn cpuid(leaf: u32, sub_leaf: Option<u32>) -> CpuidResult {
    __cpuid_count(leaf, sub_leaf.unwrap_or(0))
}

#[inline]
pub unsafe fn get_vendor_name() -> String {
    let res = unsafe { cpuid(cpuid::common::VIALSFN, None) };

    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&res.ebx.to_ne_bytes());
    vendor[4..8].copy_from_slice(&res.edx.to_ne_bytes());
    vendor[8..12].copy_from_slice(&res.ecx.to_ne_bytes());

    String::from_utf8(vendor.to_vec()).unwrap()
}

#[inline]
pub unsafe fn get_cpu_vendor() -> CpuVendor {
    let res = unsafe { cpuid(cpuid::common::VIALSFN, None) };

    match (res.ebx, res.edx, res.ecx) {
        (0x756e6547, 0x49656e69, 0x6c65746e) => CpuVendor::Intel,
        (0x68747541, 0x69746e65, 0x444d4163) => CpuVendor::Amd,
        _ => CpuVendor::Other,
    }
}

#[inline]
pub fn who_am_i() -> u32 {
    let gs = crate::util::mem::thread_safe::get_mut();
    if let Some(gs) = &gs
        && gs.cpu_id != 0
    {
        return gs.cpu_id;
    }

    let mut ret = 0;

    let res = unsafe { cpuid(cpuid::x64::v2::X2_APIC_ID, None) };
    if res.ebx != 0 {
        ret = res.edx;
    }

    if ret == 0 {
        let res = unsafe { cpuid(cpuid::common::PIAFB, None) };
        ret = (res.ebx >> 24) & 0xFF;
    }

    if gs.is_some() {
        unsafe { gs.unwrap_unchecked() }.cpu_id = ret;
    }

    ret
}

#[inline]
pub unsafe fn get_reversion(typ: CpuVendor) -> u32 {
    unsafe {
        match typ {
            CpuVendor::Intel => (utils::read_msr(0x8B) >> 32) as u32,
            CpuVendor::Amd => utils::read_msr(0x8B) as u32,
            _ => 0,
        }
    }
}
