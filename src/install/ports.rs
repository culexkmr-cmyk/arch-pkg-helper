use super::domain::{Opts, Target};

pub trait SourceResolver {
    fn resolve(&self, target: &str) -> Target;
}

pub trait Installer {
    fn preview(&self, targets: &[Target]);
    fn install(&self, targets: &[Target], opts: Opts) -> Result<(), String>;
}
