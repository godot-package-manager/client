use crate::package::Package;
use serde::Deserialize;
use serde_json::Result;
use std::collections::HashMap;

#[derive(Debug, Default)]
/// The config file: parsed from godot.package, usually.
/// Contains only a list of [Package]s, currently.
pub struct ConfigFile {
    pub packages: Vec<Package>,
    // hooks: there are no hooks now
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
/// A wrapper to [ConfigFile]. This _is_ necessary.
/// Any alternatives will end up being more ugly than this. (trust me i tried)
/// There is no way to automatically deserialize the map into a vec.
struct ConfigWrapper {
    // support NPM package.json files (also allows gpm -c package.json -u)
    #[serde(alias = "dependencies")]
    packages: HashMap<String, String>,
}

impl From<ConfigWrapper> for ConfigFile {
    fn from(from: ConfigWrapper) -> Self {
        Self {
            packages: from
                .packages
                .into_iter()
                .map(|(name, version)| Package::new(name, version))
                .collect::<Vec<Package>>(),
        }
    }
}

impl ConfigFile {
    /// Creates a new [ConfigFile] from the given path.
    /// Panics if the file doesn't exist, or the file cant be parsed as toml, hjson or yaml.
    pub fn new(contents: &String) -> Self {
        type W = ConfigWrapper;
        #[rustfmt::skip]
        let mut cfg: ConfigFile =
            if let Ok(w) = deser_hjson::from_str::<W>(contents) { w.into() }
            else if let Ok(w) = serde_yaml::from_str::<W>(contents) { w.into() }
            else if let Ok(w) = toml::from_str::<W>(contents) { w.into() }
            else { panic!("Failed to parse the config file") };
        cfg.packages.sort();
        cfg
    }

    pub fn from_json(json: &String) -> Result<Self> {
        Ok(serde_json::from_str::<ConfigWrapper>(json)?.into())
    }

    /// Creates a lockfile for this config file.
    /// note: Lockfiles are currently unused.
    pub fn lock(&mut self) -> String {
        let mut pkgs = vec![] as Vec<Package>;
        self.collect()
            .into_iter()
            .filter(|p| p.is_installed())
            .for_each(|mut p| {
                if p.integrity.is_empty() {
                    p.integrity = p
                        .get_integrity()
                        .expect("Should be able to get package integrity");
                }
                pkgs.push(p);
            });
        serde_json::to_string_pretty(&pkgs).unwrap()
    }

    /// Iterates over all the packages (and their deps) in this config file.
    fn _for_each(pkgs: &mut [Package], mut cb: impl FnMut(&mut Package)) {
        fn inner(pkgs: &mut [Package], cb: &mut impl FnMut(&mut Package)) {
            for p in pkgs {
                cb(p);
                if p.has_deps() {
                    inner(&mut p.dependencies, cb);
                }
            }
        }
        inner(pkgs, &mut cb);
    }

    /// Public wrapper for _for_each, but with the initial value filled out.
    pub fn for_each(&mut self, cb: impl FnMut(&mut Package)) {
        Self::_for_each(&mut self.packages, cb)
    }

    /// Collect all the packages, and their dependencys.
    /// Uses clones, because I wasn't able to get references to work
    pub fn collect(&mut self) -> Vec<Package> {
        let mut pkgs: Vec<Package> = vec![];
        self.for_each(|p| pkgs.push(p.clone()));
        pkgs
    }
}

#[cfg(test)]
mod tests {
    use crate::config_file::*;

    #[test]
    fn parse() {
        let _t = crate::test_utils::mktemp();
        let cfgs: [&mut ConfigFile; 3] = [
            &mut ConfigFile::new(&r#"dependencies: { "@bendn/test": 2.0.10 }"#.into()), // quoteless fails as a result of https://github.com/Canop/deser-hjson/issues/9
            &mut ConfigFile::new(&"dependencies:\n  \"@bendn/test\": 2.0.10".into()),
            &mut ConfigFile::new(&"[dependencies]\n\"@bendn/test\" = \"2.0.10\"".into()),
        ];
        #[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
        struct LockFileEntry {
            pub name: String,
            pub version: String,
            pub integrity: String,
        }
        let wanted_lockfile = serde_json::from_str::<Vec<LockFileEntry>>(
            r#"[{"name":"@bendn/test","version":"2.0.10","integrity":"sha512-hyPGxDG8poa2ekmWr1BeTCUa7YaZYfhsN7jcLJ3q2cQVlowcTnzqmz4iV3t21QFyabE5R+rV+y6d5dAItrJeDw=="},{"name":"@bendn/gdcli","version":"1.2.5","integrity":"sha512-/YOAd1+K4JlKvPTmpX8B7VWxGtFrxKq4R0A6u5qOaaVPK6uGsl4dGZaIHpxuqcurEcwPEOabkoShXKZaOXB0lw=="}]"#,
        ).unwrap();
        for cfg in cfgs {
            assert_eq!(cfg.packages.len(), 1);
            assert_eq!(cfg.packages[0].to_string(), "@bendn/test@2.0.10");
            assert_eq!(cfg.packages[0].dependencies.len(), 1);
            assert_eq!(
                cfg.packages[0].dependencies[0].to_string(),
                "@bendn/gdcli@1.2.5"
            );
            cfg.for_each(|p| p.download());
            assert_eq!(
                serde_json::from_str::<Vec<LockFileEntry>>(cfg.lock().as_str()).unwrap(),
                wanted_lockfile
            );
        }
    }
}
