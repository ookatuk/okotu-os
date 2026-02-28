#[repr(C)]
pub struct IntelUcodeHeader {
    pub header_version: u32,
    pub update_revision: u32,
    pub date: u32,
    pub processor_sig: u32,
    pub checksum: u32,
    pub loader_revision: u32,
    pub processor_flags: u32,
    pub data_size: u32,
    pub total_size: u32,
    pub reserved: [u32; 3],
}

#[repr(C)]
pub struct AmdPatchHeader {
    pub data_code: u32,
    pub patch_id: u32,
    pub patch_level: u32,
    pub save_state_frame: u16,
    pub reg_init_count: u16,
    pub mpatch_format: u32,
    pub processor_id: u16,
    pub supported_cores: u8,
    pub reserved: [u8; 9],
    pub data_size: u32,
}

#[repr(C)]
pub struct AmdEquivTableEntry {
    pub installed_hierarchy: u32,
    pub update_channel: u32,
    pub microcode_level: u16,
    pub processor_rev_id: u16,
    pub equivalent_cpu_id: u16,
}
