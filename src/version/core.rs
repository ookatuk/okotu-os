use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use const_format::formatcp;
use const_str::split;

pub const VERSION_DATA_VERSION: u32 = 1;

use super::types::{
    BuildInfo,
    BuilderInfo,
    DeveloperInfo,
    GitInfo,
    HashInfo,
    HashVariant,
    VersionInfo
};

use crate::{MICRO_VER, OS_NAME, VERSION_RAW};

const fn hex_to_byte(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("Invalid hex char"),
    }
}

macro_rules! decode_hex {
    ($s:expr) => {{
        const S: &[u8] = $s.as_bytes();
        const LEN: usize = S.len() / 2;
        const OUT: [u8; LEN] = {
            let mut res = [0u8; LEN];
            let mut i = 0;
            while i < LEN {
                // 2文字(ASCII)を1バイト(数値)に結合
                res[i] = (hex_to_byte(S[i * 2]) << 4) | hex_to_byte(S[i * 2 + 1]);
                i += 1;
            }
            res
        };
        &OUT
    }};
}

const DIR_HASH: &[u8] = decode_hex!(env!("DIR_HASH"));
const GIT_HASH: &[u8] = decode_hex!(env!("GIT_HASH"));

const FEATURES: &[&str] = &split!(env!("BUILD_FEATURES"), ",");


impl VersionInfo<'_> {
    pub fn new_os() -> VersionInfo<'static> {
        let mut info = const {
            let build = BuildInfo {
                name: Cow::Borrowed(OS_NAME),
                features: Cow::Borrowed(&[]),
                profile: Cow::Borrowed(env!("OS_PROFILE")),
                cycle: Cow::Borrowed(env!("OS_CYCLE")),
                version: Cow::Borrowed(formatcp!("{VERSION_RAW}_{MICRO_VER}")),
            };

            let git = GitInfo {
                url: Cow::Borrowed(env!("GIT_URL")),
                branch: Cow::Borrowed(env!("GIT_BRANCH")),
                dirty: Cow::Borrowed(env!("GIT_DIRTY"))
            };

            let builder = BuilderInfo {
                version: Cow::Borrowed(env!("RUST_VER")),
                info: Cow::Borrowed(env!("RUST_VERSION_INFO")),
                name: Cow::Borrowed("rust"),
            };

            let developer = DeveloperInfo {
                developer: Cow::Borrowed(env!("GIT_USER")),
                link: Cow::Borrowed(concat!("https://github.com/", env!("GIT_USER"))),
            };

            let hash = HashInfo {

                dir_hash: Cow::Borrowed(&[
                    HashVariant::Sha3_512(Cow::Borrowed(DIR_HASH))
                ]),
                git_hash: Cow::Borrowed(&[
                    HashVariant::Sha1Dc(Cow::Borrowed(GIT_HASH))
                ]),
            };

            VersionInfo {
                data_version: VERSION_DATA_VERSION,
                git: Some(git),
                builder: Some(builder),
                developer,
                build,
                hash,
                additional: BTreeMap::new(),
            }
        };

        let entries = const { [
                (Cow::Borrowed("OsBuildHost"), Cow::Borrowed(env!("BUILD_HOST"))),
                (Cow::Borrowed("OsBuildTarget"), Cow::Borrowed(env!("BUILD_TARGET"))),
        ]};

        info.additional = entries.into_iter().collect::<BTreeMap<Cow<str>, Cow<str>>>();
        info.build.features = FEATURES.iter().map(|&s| Cow::Borrowed(s)).collect();

        info
    }
}