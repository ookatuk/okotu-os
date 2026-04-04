use alloc::borrow::{Cow};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use core::fmt::{Debug, Display, Formatter};
use base64::Engine;
use serde::{Deserialize, Serialize};
use crate::result;
use crate::result::{Error, ErrorType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptInfo<'a> {
    pub opt: Cow<'a, [Cow<'a, str>]>,
    pub debug: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo<'a> {
    pub url: Cow<'a, str>,
    pub branch: Cow<'a, str>,
    pub dirty: Cow<'a, str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuilderInfo<'a> {
    pub version: Cow<'a, str>,
    pub info: Cow<'a, str>,
    pub name: Cow<'a, str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeveloperInfo<'a> {
    pub developer: Cow<'a, str>,
    pub link: Cow<'a, str>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildInfo<'a> {
    pub name: Cow<'a, str>,
    pub features: Cow<'a,[Cow<'a, str>]>,
    pub profile: Cow<'a, str>,
    pub cycle: Cow<'a, str>,
    pub version: Cow<'a, str>,
}

#[repr(C)]
pub struct HashFnData {
    pub ptr: *const u8,
    pub len: u64,
}

#[repr(C)]
pub struct HashFnArgs {
    pub args: *const *const u8,
    pub str_len: *const u32,
    pub item_len: u16,
}

#[repr(C)]
pub struct LenResult {
    pub success: bool,
    pub len: u64,
}

#[derive(Debug, Clone)]
pub enum HashVariant<'a> {
    Sha1(Cow<'a, [u8]>),
    Sha1Dc(Cow<'a, [u8]>),

    Sha2_256(Cow<'a, [u8]>),
    Sha2_512(Cow<'a, [u8]>),
    Sha2_512_256(Cow<'a, [u8]>),

    Sha3_256(Cow<'a, [u8]>),
    Sha3_512(Cow<'a, [u8]>),

    Blake2B(Cow<'a, [u8]>),
    Blake2S(Cow<'a, [u8]>),

    Blake3(Cow<'a, [u8]>),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HashInfo<'a> {
    #[serde(bound(deserialize = "HashVariant<'a>: Deserialize<'de>, 'de: 'a"))]
    pub dir_hash: Cow<'a,[HashVariant<'a>]>,
    #[serde(bound(deserialize = "HashVariant<'a>: Deserialize<'de>, 'de: 'a"))]
    pub git_hash: Cow<'a,[HashVariant<'a>]>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VersionInfo<'a> {
    pub data_version: u32,
    pub git: Option<GitInfo<'a>>,
    pub builder: Option<BuilderInfo<'a>>,
    pub developer: DeveloperInfo<'a>,
    pub build: BuildInfo<'a>,
    #[serde(bound(deserialize = "HashVariant<'a>: Deserialize<'de>, 'de: 'a"))]
    pub hash: HashInfo<'a>,
    #[serde(borrow)]
    pub additional: BTreeMap<Cow<'a, str>, Cow<'a, str>>,
}

impl Display for HashVariant<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {}", self.algo(), self.hash())
    }
}

impl HashVariant<'_> {
    #[inline]
    pub fn hash(&self) -> String {
        let bytes = match self {
            HashVariant::Sha1(b) | HashVariant::Sha1Dc(b) |
            HashVariant::Sha2_256(b) | HashVariant::Sha2_512(b) |
            HashVariant::Sha2_512_256(b) | HashVariant::Sha3_256(b) |
            HashVariant::Sha3_512(b) | HashVariant::Blake2B(b) |
            HashVariant::Blake2S(b) | HashVariant::Blake3(b) => b,
        };

        base64::prelude::BASE64_URL_SAFE.encode(bytes)
    }

    #[inline]
    pub fn algo(&self) -> String {
        match self {
            HashVariant::Sha1(_) => "SHA-1",
            HashVariant::Sha1Dc(_) => "SHA-1DC",

            HashVariant::Sha2_256(_) => "SHA2-256",
            HashVariant::Sha2_512(_) => "SHA2-512",
            HashVariant::Sha2_512_256(_) => "SHA2-512/256",

            HashVariant::Sha3_256(_) => "SHA3-256",
            HashVariant::Sha3_512(_) => "SHA3-512",

            HashVariant::Blake2B(_) => "Blake2_B",
            HashVariant::Blake2S(_) => "Blake2_S",

            HashVariant::Blake3(_) => "Blake3",
            #[allow(unreachable_patterns)]
            _ => "Unknown"
        }.to_string()
    }

    pub fn from_parts<'a>(algo: &str, hash: Cow<'a, [u8]>) -> result::Result<HashVariant<'a>> {
        match algo {
            "SHA-1" => Ok(HashVariant::Sha1(hash)),
            "SHA-1DC" => Ok(HashVariant::Sha1Dc(hash)),
            "SHA2-256" => Ok(HashVariant::Sha2_256(hash)),
            "SHA2-512" => Ok(HashVariant::Sha2_512(hash)),
            "SHA2-512/256" => Ok(HashVariant::Sha2_512_256(hash)),
            "SHA3-256" => Ok(HashVariant::Sha3_256(hash)),
            "SHA3-512" => Ok(HashVariant::Sha3_512(hash)),
            "Blake2_B" => Ok(HashVariant::Blake2B(hash)),
            "Blake2_S" => Ok(HashVariant::Blake2S(hash)),
            "Blake3" => Ok(HashVariant::Blake3(hash)),
            _ => Error::new_string(
                ErrorType::NotSupported,
                Some(format!("not supported hash type ({})", algo)),
            ).raise(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EcdsaType {
    P256,
    P384,
    P521,
    BrainPool(u16),
}

#[derive(Debug, Clone)]
pub enum SignVariant<'a> {
    RsaPss2048(HashVariant<'a>, Cow<'a, [u8]>),
    RsaPss4096(HashVariant<'a>, Cow<'a, [u8]>),
    ECDSA(EcdsaType, HashVariant<'a>, Cow<'a, [u8]>),
    ED25519(Cow<'a, [u8]>),
    ED448(Cow<'a, [u8]>),
    DILITHIUM(u8, Cow<'a, [u8]>)
}