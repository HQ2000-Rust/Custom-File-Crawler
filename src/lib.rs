use custom_file_crawler::prelude::*;
use std::os::windows::fs::MetadataExt;
use std::sync::{Arc, Mutex};

fn main() {
    let result=
    Crawler::new()
        .start_dir("C:\\Users\\jonat\\RustroverProjects\\crawler_test\\scratch")
        .context(Mutex::new(0))
        .search_depth(1)
        .file_regex(".*")
        .folder_regex(".*")
        .run(|context: Arc<Mutex<u32>>, p| {
            let created = std::fs::metadata(p.clone())?.created()?;
            let size = std::fs::metadata(p.clone())?.file_size();
            println!(
                "{}: {:04} ({} KB)",
                p.display(),
                created.elapsed().unwrap().as_secs() / 86400,
                size / 1024
            );
            *context.lock().unwrap()+=1;
            Ok::<(), std::io::Error>(())
        }).unwrap();
    println!("Finished: {}", result.lock().unwrap());
}
