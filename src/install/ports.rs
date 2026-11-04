use super::domain::{Opts, Source};
use std::result::Result;

pub trait SourceResolver {
    fn classify(&self, target: &str) -> Source;
}

pub trait Installer {
    fn install_batch(&self, targets: &[String], opts: Opts) -> Result<(), String>;
    fn preview(&self, target: &str);
}
