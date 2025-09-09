# Custom-File-Crawler - a fast, concurrent, customisable and fully documented file crawler!
Features:
- fully multithreaded by default (2x - x speedup)
- Two options:
  - async
  - non-async
- set maximum traversal depth
- set file / folder regex
- lazyness (see lazy evaluation)
- execution a closure for every file (in the defined scope)
- having persistent data across closure invocations
- (set start directory)

## Example usage
 use file_crawler::prelude::*;

 use std::fs::File;
 use std::path::PathBuf;
 use std::sync::Arc;
 use std::sync::atomic::AtomicU32;

 let a_count=
 Crawler::new()
    .start_dir("C\\user\\foo")
    .context(AtomicU32::new(0))
    .run(|ctx: Arc<AtomicU32>, path: PathBuf| {
        let mut contents=String::new();
        let file=File::open(path)?;
        //NOTE: this can cause an error for files not readable as UTF-8
        //which returns an error and therefore terminates the crawler
        file.read_to_string(&mut contents)?;
        contents.chars().for_each(|char| if char=='a' { ctx.fetch_add(1); });
        Ok(())
    })?;
 println!("Appearance of the letter 'a' in \"C\\user\\foo\": {}", a_count)
