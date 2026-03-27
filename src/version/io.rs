use core::fmt::{Debug, Display, Formatter};
use crate::version::types::VersionInfo;

impl Debug for VersionInfo<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "--- version info ---")?;

        {
            writeln!(f, "data_version: {}", self.data_version)?;
            writeln!(f, "")?;

            writeln!(f, " -*- build info -*-")?;
            writeln!(f, "name: {}", self.build.name)?;
            writeln!(f, "version: {}", self.build.version)?;
            writeln!(f, "cycle: {}", self.build.cycle)?;

            let profile = self.build.profile.as_ref();
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
            writeln!(f, "dirty: {}", git.dirty)?;
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

        {
            writeln!(f, " -*- additional info -*-")?;
            write!(f, "additional: [")?;
            for (i, (k, v)) in self.additional.iter().enumerate() {
                if i > 0 { write!(f, ",")?; }
                write!(f, "{}: {}", k, v)?;
            }
            writeln!(f, "]")?;
        }

        writeln!(f, "--- end of version info ---")
    }
}

impl Display for VersionInfo<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{} ({} build v{})",
            self.build.name,
            self.build.cycle,
            self.build.version
        )
    }
}