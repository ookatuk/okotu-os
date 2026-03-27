use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::hint::unlikely;
use rhai::{CustomType, TypeBuilder};
use crate::version::types::{BuildInfo, BuilderInfo, DeveloperInfo, GitInfo, HashInfo, OptInfo, HashVariant, VersionInfo};

impl CustomType for HashInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get("dir_hash", |sel: &mut Self| sel.dir_hash.clone());
        builder.with_get("git_hash", |sel: &mut Self| sel.git_hash.clone());
    }
}

impl CustomType for HashVariant<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get("hash", |sel: &mut Self| {
            sel.hash()
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
                                 let data = sel.profile.as_ref();
                                 if unlikely(data == "") {
                                     None
                                 } else {
                                     Some(data.to_string())
                                 }
                             },

                             |sel: &mut Self, value: Option<String>| {
                                 sel.profile = value.map(Cow::Owned).unwrap_or(Cow::Borrowed(""));
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

impl CustomType for VersionInfo<'static> {
    fn build(mut builder: TypeBuilder<Self>) {
        builder.with_get("git", |sel: &mut Self| sel.git.clone());
        builder.with_get("builder", |sel: &mut Self| sel.builder.clone());

        builder.with_get("developer", |sel: &mut Self| sel.developer.clone());
        builder.with_get("build", |sel: &mut Self| sel.build.clone());
        builder.with_get("hash", |sel: &mut Self| sel.hash.clone());
    }
}