pub mod types;

use crate::cpu::mitigation::ucode::types::{AmdEquivTableEntry, AmdPatchHeader, IntelUcodeHeader};
use crate::cpu::utils;
use crate::cpu::utils::{CpuVendor, get_reversion};
use crate::mem::smart_ptr::RangePtr;
use crate::util::result;
use crate::util::result::{Error, ErrorType};
use crate::{fs, log_debug, log_info, log_trace, log_warn};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::{format, vec};
use core::alloc::Layout;
use core::ptr::NonNull;
use miniz_oxide::inflate::decompress_slice_iter_to_slice;
use uefi::proto::media::file::{File, FileInfo, FileMode};
use uefi::{CStr16, cstr16};
use uefi_raw::protocol::file_system::FileAttribute;

pub fn load() -> result::Result<()> {
    let typ = unsafe { utils::get_cpu_vendor() };

    log_info!("kernel", "micro code", "attaching micro code");
    log_trace!("kernel", "micro code", "cpu vendor: {:?}", typ);

    let (patch_ptr, ptr) = get_micro_code(typ.clone())?;

    unsafe {
        load_from_ptr(patch_ptr, typ)?;
    }

    log_trace!("kernel", "micro code", "dropping micro code...");

    drop(ptr);

    log_info!("kernel", "micro code", "successfully loaded ucode");

    Ok(())
}

pub fn get_micro_code(typ: CpuVendor) -> result::Result<(*const u8, RangePtr)> {
    let sig = unsafe { utils::cpuid(1, None) }.eax;

    let data = unsafe { find_good_file(typ.clone())? };

    let patch_ptr: *const u8 = match typ {
        CpuVendor::Intel => {
            let pid = ((unsafe { utils::read_msr(0x17) } >> 50) & 0x07) as u32;
            let pfs = 1 << pid;

            find_intel_patch(data.as_slice(), sig, pfs)
                .map(|p| p as *const u8)
                .ok_or(Error::new(
                    ErrorType::NotFound,
                    Some("Intel ucode not found"),
                ))
        }
        CpuVendor::Amd => find_amd_patch(data.as_slice(), sig)
            .map(|p| p as *const u8)
            .ok_or(Error::new(ErrorType::NotFound, Some("AMD ucode not found"))),
        _ => Error::new(ErrorType::NotSupported, Some("Unsupported Vendor")).raise(),
    }?;
    Ok((patch_ptr, data))
}

pub const fn find_intel_patch(slice: &[u8], sig: u32, pfs: u32) -> Option<*const IntelUcodeHeader> {
    let mut offset = 0;
    while offset + size_of::<IntelUcodeHeader>() <= slice.len() {
        let header_ptr = unsafe { slice.as_ptr().add(offset) as *const IntelUcodeHeader };
        let header = unsafe { &*header_ptr };

        let total_size = if header.total_size == 0 {
            2000
        } else {
            header.total_size
        } as usize;

        if header.processor_sig == sig && (header.processor_flags & pfs) != 0 {
            return Some(header_ptr);
        }
        offset += total_size;
        if total_size == 0 {
            break;
        }
    }
    None
}

pub const fn find_amd_patch(slice: &[u8], sig: u32) -> Option<*const AmdPatchHeader> {
    let equiv_id = match find_equiv_id(slice, sig) {
        Some(equiv_id) => equiv_id,
        None => return None,
    };

    let mut offset = find_first_patch_offset(slice);

    while offset + size_of::<AmdPatchHeader>() <= slice.len() {
        let patch_ptr = unsafe { slice.as_ptr().add(offset) as *const AmdPatchHeader };
        let patch = unsafe { &*patch_ptr };

        if patch.data_code != 0x00000001 {
            break;
        }

        if patch.processor_id == equiv_id {
            return Some(patch_ptr);
        }

        offset += size_of::<AmdPatchHeader>() + patch.data_size as usize;
    }
    None
}

pub unsafe fn load_from_ptr(data: *const u8, typ: CpuVendor) -> result::Result<()> {
    let addr = data as u64;

    // 16バイトアライメントチェック
    if addr % 16 != 0 {
        return Error::new(ErrorType::InvalidData, Some("Ucode address not aligned")).raise();
    }

    let current_rev = unsafe { get_reversion(typ.clone()) };

    let expected_rev = unsafe {
        match typ {
            CpuVendor::Intel => (*(data as *const IntelUcodeHeader)).update_revision,
            CpuVendor::Amd => (*(data as *const AmdPatchHeader)).patch_level,
            _ => 0,
        }
    };

    if current_rev >= expected_rev {
        if current_rev > expected_rev {
            log_warn!(
                "kernel",
                "micro code",
                "CPU already has a newer revision. (0x{:x} > 0x{:x})",
                current_rev,
                expected_rev
            );
        } else {
            log_info!(
                "kernel",
                "micro code",
                "No need to patch (already up to date)."
            );
        }
        return Ok(());
    }

    unsafe {
        match typ {
            CpuVendor::Intel => {
                utils::write_msr(0x8B, 0);
                utils::write_msr(0x79, addr);
                utils::cpuid(1, None);
            }
            CpuVendor::Amd => {
                utils::write_msr(0xC001_0020, addr);
            }
            _ => return Ok(()),
        }
    }

    let current_rev = unsafe { get_reversion(typ) };

    if current_rev != expected_rev {
        return Error::new(ErrorType::OtherError, Some("Revision mismatch after load")).raise();
    }

    Ok(())
}

const fn find_equiv_id(slice: &[u8], sig: u32) -> Option<u16> {
    let table_size = unsafe { *(slice.as_ptr().add(8) as *const u32) } as usize;

    let entry_size = size_of::<AmdEquivTableEntry>();
    let num_entries = table_size / entry_size;

    let table_ptr = unsafe { slice.as_ptr().add(12) as *const AmdEquivTableEntry };

    let mut i = 0;
    while i < num_entries {
        let entry = unsafe { &*table_ptr.add(i) };
        if entry.processor_rev_id == (sig as u16) {
            return Some(entry.equivalent_cpu_id);
        }

        i += 1;
    }
    None
}

const fn find_first_patch_offset(slice: &[u8]) -> usize {
    let table_size = unsafe { *(slice.as_ptr().add(8) as *const u32) } as usize;

    12 + table_size
}

unsafe fn find_good_file(vendor_enum: CpuVendor) -> result::Result<RangePtr> {
    let (mut ucode_dir, filename) = {
        let mut root = fs::get_root()?;
        let vendor_name = unsafe { utils::get_vendor_name() };

        let contents_dir = Error::try_raise(
            root.open(
                cstr16!("\\EFI\\BOOT\\contents\\ucode"),
                FileMode::Read,
                FileAttribute::DIRECTORY,
            ),
            Some("Cloud not found Dir."),
        )?
        .into_directory();

        let mut contents_dir =
            Error::from_option(contents_dir, ErrorType::InvalidFileType, Some("Not a dir"))?;

        let mut buf = vec![0u16; vendor_name.len() + 1];

        let ret = CStr16::from_str_with_buf(&vendor_name, buf.as_mut_slice());
        if let Some(err) = ret.err() {
            return Error::new_string(
                ErrorType::InvalidData,
                Some(format!("Cpu Vendor Name is Bad (sub-A) ({})", err)),
            )
            .raise();
        }
        let ret = unsafe { ret.unwrap_unchecked() };

        let ucode_dir = Error::try_raise(
            { contents_dir.open(ret, FileMode::Read, FileAttribute::DIRECTORY) },
            Some("Cloud not found Dir."),
        )?
        .into_directory();

        let mut ucode_dir =
            Error::from_option(ucode_dir, ErrorType::InvalidFileType, Some("Not a dir"))?;

        let filename = match vendor_enum {
            CpuVendor::Intel => {
                let sig = unsafe { utils::cpuid(utils::cpuid::common::PIAFB, None) }.eax;

                let stepping = sig & 0xF;
                let model = (sig >> 4) & 0xF;
                let family = (sig >> 8) & 0xF;
                let extended_model = (sig >> 16) & 0xF;
                let extended_family = (sig >> 20) & 0xFF;

                let actual_family = if family == 0xF {
                    family + extended_family
                } else {
                    family
                };
                let actual_model = if family == 0x6 || family == 0xF {
                    (extended_model << 4) + model
                } else {
                    model
                };

                vec![format!(
                    "{:02x}-{:02x}-{:02x}",
                    actual_family, actual_model, stepping
                )]
            }
            CpuVendor::Amd => {
                let sig = unsafe { utils::cpuid(0x0000_0001, None) }.eax;
                let base_family = (sig >> 8) & 0xF;
                let ext_family = (sig >> 20) & 0xFF;

                let actual_family = if base_family == 0xF {
                    base_family + ext_family
                } else {
                    base_family
                };

                vec![
                    format!("microcode_amd_fam{:02x}h", actual_family),
                    "microcode_amd".to_string(),
                ]
            }
            CpuVendor::Other => {
                return Error::new_string(
                    ErrorType::NotSupported,
                    Some(format!("Not support Cpu vendor {:?}.", vendor_enum)),
                )
                .raise();
            }
        };

        (ucode_dir, filename)
    };

    let mut target_path = {
        log_debug!("kernel", "micro code", "think targets: {:?}", filename);

        let mut target_path: Option<_> = None;
        let mut tmp_buf = vec![];

        for i in filename.iter() {
            let target = format!("{}.z", i);

            let required_len = target.len() + 1;

            tmp_buf.resize(required_len, 0u16);

            let a = CStr16::from_str_with_buf(&target, tmp_buf.as_mut_slice());

            if let Some(err) = a.err() {
                return Error::new_string(
                    ErrorType::InvalidData,
                    Some(format!("Cpu Vendor Name is Bad ({})", err)),
                )
                .raise();
            }
            let a = unsafe { a.unwrap_unchecked() };

            let res = ucode_dir.open(a, FileMode::Read, FileAttribute::READ_ONLY)?;

            if let Some(data) = res.into_regular_file() {
                target_path = Some(data);
                break;
            }
        }

        target_path
    };

    let mut target_path = Error::from_option(
        target_path,
        ErrorType::NotSupported,
        Some("No matching files were found."),
    )?;

    let size: Box<FileInfo> = target_path.get_boxed_info()?;
    let size = size.file_size();

    let mut buffer: Vec<u8> = vec![0u8; size as usize];
    target_path.read(&mut buffer)?;

    let result = unsafe {
        decompress(
            NonNull::new(buffer.as_mut_ptr()).unwrap_unchecked(),
            size as usize,
        )
    };

    drop(buffer);
    result
}

/// default圧縮を解凍する
/// # Args
/// * `raw_data` - 解凍前のデータ
/// * `file_size` - データのサイズ
/// # Returns
/// 1. `&'a mut [u8]` - 解凍後のやつ
/// # Errors
/// * [`ErrorType::AllocationFailed`]
/// 1. アロケーションに失敗した場合
/// * [`ErrorType::OtherError`]
/// 1. 解凍に失敗した場合
unsafe fn decompress<'a>(raw_data: NonNull<u8>, file_size: usize) -> result::Result<RangePtr> {
    if file_size < 4 {
        return Error::new(ErrorType::InvalidData, Some("Data too small")).raise();
    }
    let decomp_size = unsafe { (raw_data.as_ptr() as *const u32).read_unaligned() } as usize;
    let compressed_slice =
        unsafe { core::slice::from_raw_parts(raw_data.as_ptr().add(4), file_size - 4) };

    let layout = Layout::from_size_align(decomp_size, 16)
        .map_err(|_| Error::new(ErrorType::AllocationFailed, Some("Invalid layout")))?;

    let ptr = unsafe { alloc::alloc::alloc(layout) };
    if ptr.is_null() {
        return Error::new(
            ErrorType::AllocationFailed,
            Some("failed to allocate ucode"),
        )
        .raise();
    }

    let mut out_slice = unsafe { RangePtr::new(ptr, layout) };

    let status = decompress_slice_iter_to_slice(
        out_slice.as_mut_slice(),
        core::iter::once(compressed_slice),
        false,
        true,
    );

    if let Err(e) = status {
        return Error::new_string(ErrorType::OtherError, Some(format!("{:?}", e))).raise();
    }

    Ok(out_slice)
}
