//! Configuration for profile-guided optimization of bootstrap components.

use serde::{Deserialize, Deserializer};

use crate::core::config::Merge;
use crate::core::config::toml::ReplaceOpt;
use crate::{HashSet, PathBuf, define_config, exit};

#[derive(Clone, Default, Debug, serde_derive::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PgoConfig {
    /// Use this profile when optimizing the component.
    #[serde(default, rename = "use")]
    pub use_profile: Option<PathBuf>,
    /// Instrument the component and write profiles to this path when it runs.
    #[serde(default, rename = "generate")]
    pub generate_profile: Option<PathBuf>,
}

define_config! {
    #[derive(Default)]
    struct Pgo {
        rustc: Option<PgoConfig> = "rustc",
        rustdoc: Option<PgoConfig> = "rustdoc",
        cargo: Option<PgoConfig> = "cargo",
        clippy: Option<PgoConfig> = "clippy",
        rustfmt: Option<PgoConfig> = "rustfmt",
        llvm: Option<PgoConfig> = "llvm",
    }
}
