use std::fs::{ File, ReadDir};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let dir = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => std::panic::panic_any(error),
    };

    load_all(dir)
}

fn load_all(
    start_dir: PathBuf,
) -> anyhow::Result<()> {
    println!("Loading all data from {:?}", start_dir);
    #[cfg(feature = "verify")]
    if !start_dir.is_dir() {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{:?} is not a directory", start_dir),
        )));
    }

    let entries_result = std::fs::read_dir(&start_dir);
    let entries: ReadDir = match entries_result {
        Ok(entries) => entries,
        Err(error) => return Err(anyhow::Error::from(error)),
    };

    for entry in entries.into_iter().filter_map(|entry| entry.ok()) {
        if entry.path().is_dir() {
            #[cfg(feature = "dont_keep_going")]
                load_all(entry.path())?;
            #[cfg(not(feature = "dont_keep_going"))]
            let _ = load_all(entry.path());
        } else {
            println!("Loading {}", entry.path().display());
            #[cfg(feature = "dont_keep_going")]
            File::open(entry.path())?;
            #[cfg(not(feature = "dont_keep_going"))]
            let _ = File::open(entry.path());
        }
    }

    Ok(())
}
