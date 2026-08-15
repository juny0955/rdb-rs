use std::{
    fs::{File, OpenOptions, create_dir_all},
    io::Result,
    path::Path,
};

pub fn open_rw_create(path: &Path) -> Result<File> {
    let mut binding = OpenOptions::new();
    let options = binding.read(true).write(true).create(true);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        create_dir_all(parent)?;
    }

    let file = options.open(path)?;
    Ok(file)
}
