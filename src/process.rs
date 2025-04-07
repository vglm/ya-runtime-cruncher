use anyhow::Context;
use std::env::current_exe;
use std::path::{Path, PathBuf};

#[allow(unused)]
#[derive(Default, Clone)]
pub struct Usage {
    pub cnt: u64,
}

pub fn find_file(file_name: impl AsRef<Path>) -> anyhow::Result<PathBuf> {
    let exe = current_exe()?;
    let parent_dir = exe
        .parent()
        .context("Unable to get parent dir of {exe:?}")?;
    let file = parent_dir.join(&file_name);
    if file.exists() {
        return Ok(file);
    }
    anyhow::bail!("Unable to get dummy runtime base dir");
}
