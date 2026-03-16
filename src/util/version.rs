use alloc::borrow::{Cow, ToOwned};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::{Debug, Display, Formatter, Write};
use const_format::formatcp;
use const_str::split;
use rhai::TypeBuilder;
use rhai::CustomType;
use crate::{MICRO_VER, OS_NAME, VERSION_RAW};

const DIR_HASH: &[u8] = env!("DIR_HASH").as_bytes();
const GIT_HASH: &[u8] = env!("GIT_HASH").as_bytes();

const FEATURES: &[&str] = &split!(env!("BUILD_FEATURES"), ",");

#[derive(Debug, Clone)]
pub struct OptInfo<'a> {
    pub opt: Cow<'a, [Cow<'a, str>]>,
    pub debug: bool,
}

#[derive(Debug, Clone)]
pub struct GitInfo<'a> {
    pub url: Cow<'a, str>,
    pub branch: Cow<'a, str>,
}

#[derive(Debug, Clone)]
pub struct BuilderInfo<'a> {
    pub version: Cow<'a, str>,
    pub info: Cow<'a, str>,
    pub name: Cow<'a, str>,
}

#[derive(Debug, Clone)]
pub struct DeveloperInfo<'a> {
    pub developer: Cow<'a, str>,
    pub link: Cow<'a, str>,
}

#[derive(Debug, Clone)]
pub struct BuildInfo<'a> {
    pub name: Cow<'a, str>,
    pub features: Cow<'a,[Cow<'a, str>]>,
    pub profile: Option<Cow<'a, str>>,
    pub cycle: Cow<'a, str>,
    pub version: Cow<'a, str>,
}

#[derive(Debug, Clone)]
pub enum Sha2<'a> {
    Sha256(Cow<'a, [u8]>),
    Sha512(Cow<'a, [u8]>),
}

#[derive(Debug, Clone)]
pub enum Sha3<'a> {
    Sha256(Cow<'a, [u8]>),
    Sha512(Cow<'a, [u8]>),
}

#[derive(Debug, Clone)]
pub enum ShaVariant<'a> {
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

impl Display for ShaVariant<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {}", self.algo(), self.hex())
    }
}

impl ShaVariant<'_> {
    #[inline]
    pub fn hex(&self) -> String {
        let bytes = match self {
            ShaVariant::Sha1(b) | ShaVariant::Sha1Dc(b) |
            ShaVariant::Sha2_256(b) | ShaVariant::Sha2_512(b) |
            ShaVariant::Sha2_512_256(b) | ShaVariant::Sha3_256(b) |
            ShaVariant::Sha3_512(b) | ShaVariant::Blake2B(b) |
            ShaVariant::Blake2S(b) | ShaVariant::Blake3(b) => b,
        };

        hex::encode(bytes)
    }

    #[inline]
    pub fn algo(&self) -> String {
        match self {
            ShaVariant::Sha1(_) => "SHA-1",
            ShaVariant::Sha1Dc(_) => "SHA-1DC",

            ShaVariant::Sha2_256(_) => "SHA2-256",
            ShaVariant::Sha2_512(_) => "SHA2-512",
            ShaVariant::Sha2_512_256(_) => "SHA2-512/256",

            ShaVariant::Sha3_256(_) => "SHA3-256",
            ShaVariant::Sha3_512(_) => "SHA3-512",

            ShaVariant::Blake2B(_) => "Blake2_B",
            ShaVariant::Blake2S(_) => "Blake2_S",

            ShaVariant::Blake3(_) => "Blake3",
            _ => "Unknown"
        }.to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct HashInfo<'a> {
    pub dir_hash: Cow<'a,[ShaVariant<'a>]>,
    pub git_hash: Cow<'a,[ShaVariant<'a>]>,
}

impl CustomType for HashInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get("dir_hash", |sel: &mut Self| sel.dir_hash.clone());
        builder.with_get("git_hash", |sel: &mut Self| sel.git_hash.clone());
    }
}

impl CustomType for ShaVariant<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get("hex", |sel: &mut Self| {
            sel.hex()
        });

        builder.with_get("algo", |sel: &mut Self| {
            sel.algo()
        });
    }
}

impl CustomType for DeveloperInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get_set("developer", |sel: &mut Self| sel.developer.to_string(), |sel: &mut Self, value| sel.developer = Cow::Owned(value));
    }
}

impl CustomType for BuilderInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get_set("name", |sel: &mut Self| sel.name.to_string(), |sel: &mut Self, value| sel.name = Cow::Owned(value));
        builder.with_get_set("info", |sel: &mut Self| sel.info.to_string(), |sel: &mut Self, value| sel.info = Cow::Owned(value));
        builder.with_get_set("version", |sel: &mut Self| sel.version.to_string(), |sel: &mut Self, value| sel.version = Cow::Owned(value));
    }
}

impl CustomType for GitInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get_set("url", |sel: &mut Self| sel.url.to_string(), |sel: &mut Self, value| sel.url = Cow::Owned(value));
        builder.with_get_set("branch", |sel: &mut Self| sel.branch.to_string(), |sel: &mut Self, value| sel.branch = Cow::Owned(value));
    }
}


impl CustomType for OptInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get_set("opt",
                             |sel: &mut Self| {
                                 sel.opt.iter()
                                     .map(|s| rhai::Dynamic::from(s.to_string()))
                                     .collect::<Vec<_>>()
                             },

                             |sel: &mut Self, value: Vec<rhai::Dynamic>| {
                                 sel.opt = value.into_iter()
                                     .map(|d| Cow::Owned(d.to_string()))
                                     .collect::<Vec<_>>().into();
                             }
        );

        builder.with_get_set("debug",

                             |sel: &mut Self| sel.debug,
                             |sel: &mut Self, value: bool| sel.debug = value
        );
    }
}

impl CustomType for BuildInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get_set("features",
                             |sel: &mut Self| {
                                 sel.features.iter()
                                     .map(|p| p.to_string())
                                     .collect::<Vec<String>>()
                             },

                             |sel: &mut Self, value: Vec<String>| {
                                 sel.features = value.into_iter()
                                     .map(Cow::Owned)
                                     .collect::<Vec<Cow<'static, str>>>()
                                     .into();
                             }
        );

        builder.with_get_set("profile",
                             |sel: &mut Self| {
                                 sel.profile.as_ref().map(|p| p.to_string())
                             },

                             |sel: &mut Self, value: Option<String>| {
                                 sel.profile = value.map(Cow::Owned);
                             }
        );

        builder.with_get_set("cycle",

                             |sel: &mut Self| sel.cycle.to_string(),
                             |sel: &mut Self, value: String| sel.cycle = Cow::Owned(value)
        );
        builder.with_get_set("version",
                             |sel: &mut Self| sel.version.to_string(),
                             |sel: &mut Self, value: String| sel.version = Cow::Owned(value)
        );
        builder.with_get_set("name",
                             |sel: &mut Self| sel.name.to_string(),
                             |sel: &mut Self, value: String| sel.name = Cow::Owned(value)
        );
    }
}

#[derive(Clone)]
pub struct VersionInfo<'a> {
    pub git: Option<GitInfo<'a>>,
    pub builder: Option<BuilderInfo<'a>>,
    pub developer: DeveloperInfo<'a>,
    pub build: BuildInfo<'a>,
    pub hash: HashInfo<'a>
}

impl CustomType for VersionInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get("git", |sel: &mut Self| sel.git.clone());
        builder.with_get("builder", |sel: &mut Self| sel.builder.clone());

        builder.with_get("developer", |sel: &mut Self| sel.developer.clone());
        builder.with_get("build", |sel: &mut Self| sel.build.clone());
        builder.with_get("hash", |sel: &mut Self| sel.hash.clone());
    }
}

impl Debug for VersionInfo<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "--- version info ---")?;

        {
            writeln!(f, " -*- build info -*-")?;
            writeln!(f, "name: {}", self.build.name)?;
            writeln!(f, "version: {}", self.build.version)?;
            writeln!(f, "cycle: {}", self.build.cycle)?;

            let profile = self.build.profile.as_deref().unwrap_or("unknown");
            writeln!(f, "profile: {}", profile)?;

            write!(f, "features: [")?;
            for (i, feat) in self.build.features.iter().enumerate() {
                if i > 0 { write!(f, ",")?; }
                write!(f, "{}", feat)?;
            }
            writeln!(f, "]")?;
        }

        if let Some(git) = &self.git {
            writeln!(f, " -*- repo -*-")?;
            writeln!(f, "url: {}", git.url)?;
            writeln!(f, "branch: {}", git.branch)?;
        }

        {
            writeln!(f, " -*- developer -*-")?;
            writeln!(f, "name: {}", self.developer.developer)?;
            writeln!(f, "link: {}", self.developer.link)?;
        }

        if let Some(build) = &self.builder {
            writeln!(f, " -*- builder -*-")?;
            writeln!(f, "name: {}", build.name)?;
            writeln!(f, "version: {}", build.version)?;
            writeln!(f, "info: {}", build.info)?;
        }

        {
            writeln!(f, " -*- hash -*-")?;
            writeln!(f, "  -*- dir -*-")?;
            for i in self.hash.dir_hash.iter() {
                writeln!(f, "    {}", i)?;
            }
            writeln!(f, "  -*- repo -*-")?;
            for i in self.hash.git_hash.iter() {
                writeln!(f, "    {}", i)?;
            }
        }

        writeln!(f, "--- end of version info ---")
    }
}

impl VersionInfo<'_> {
    pub fn os() -> VersionInfo<'static> {
        let build = BuildInfo {
            name: Cow::Borrowed(OS_NAME),
            features: FEATURES.iter().map(|&s| Cow::Borrowed(s)).collect(),
            profile: Some(Cow::Borrowed(env!("OS_PROFILE"))),
            cycle: Cow::Borrowed(env!("OS_CYCLE")),
            version: Cow::Borrowed(formatcp!("{VERSION_RAW}_{MICRO_VER}")),
        };

        let (git, builder, developer, hash) = const {
            let git = GitInfo {
                url: Cow::Borrowed(env!("GIT_URL")),
                branch: Cow::Borrowed(env!("GIT_BRANCH")),
            };

            let builder = BuilderInfo {
                version: Cow::Borrowed(env!("RUST_VER")),
                info: Cow::Borrowed(env!("RUST_VERSION_INFO")),
                name: Cow::Borrowed("rust"),
            };

            let developer = DeveloperInfo {
                developer: Cow::Borrowed(env!("GIT_USER")),
                link: Cow::Borrowed(env!("GIT_URL")),
            };

            let hash = HashInfo {
                dir_hash: Cow::Borrowed(&[
                    ShaVariant::Sha3_512(Cow::Borrowed(DIR_HASH))
                ]),
                git_hash: Cow::Borrowed(&[
                    ShaVariant::Sha1Dc(Cow::Borrowed(GIT_HASH))
                ]),
            };
            (git, builder, developer, hash)
        };

        VersionInfo {
            git: Some(git),
            builder: Some(builder),
            developer,
            build,
            hash,
        }
    }
}