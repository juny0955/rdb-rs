use std::{io::Result, path::Path};

use crate::file::open_rw_create;

mod catalog;
mod file;
mod page;
mod schema;
mod table;

fn main() -> Result<()> {
    let _file = open_rw_create(Path::new("data/table.data"))?;

    Ok(())
}

#[cfg(test)]
mod test_supports;
