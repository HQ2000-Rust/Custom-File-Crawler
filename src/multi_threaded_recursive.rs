//not working
use std::error::Error;
use std::fs::{DirEntry, File, ReadDir};
use std::hint::spin_loop;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, RwLock, RwLockReadGuard};
use std::thread::JoinHandle;
use std::time::Duration;

const FINISH_QUERY_INTERVAL: Duration = Duration::from_millis(500);

fn main() -> anyhow::Result<()> {
    let dir = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => std::panic::panic_any(error),
    };

    let handles: Arc<Mutex<Option<Vec<JoinHandle<()>>>>> = Arc::new(Mutex::new(Some(Vec::new())));

    #[cfg(feature = "dont_keep_going")]
    let failure: Arc<RwLock<Option<anyhow::Error>>> = Arc::new(RwLock::new(None));

    #[cfg(feature = "dont_keep_going")]
    {
        load_all(dir, handles.clone(), failure.clone())?;
    }

    #[cfg(not(feature = "dont_keep_going"))]
    let _ = load_all(dir, handles.clone());

    loop {
        match handles.try_lock() {
            Ok(mut handles) => {
                dbg!(&handles);
                handles.take().expect("Always Some(_) during acquisition").into_iter().for_each(|handle| handle.join().unwrap());
                /*if *handles {
                    #[cfg(feature = "dont_keep_going")]
                    {
                        break match (failure.read()? as RwLockReadGuard<'_, Option<Box<dyn Error>>>)
                            .as_ref()
                            .unwrap()
                        {
                            None => Ok(()),
                            Some(failure) => Err(failure.into()),
                        };
                    }
                    #[cfg(not(feature = "dont_keep_going"))]
                    break Ok(());
                }*/
                break Ok(());
            }
            Err(_) => {}
        }
        std::thread::sleep(FINISH_QUERY_INTERVAL);
    }
}

fn load_all(
    start_dir: PathBuf,
    handles: Arc<Mutex<Option<Vec<JoinHandle<()>>>>>,
    #[cfg(feature = "dont_keep_going")] failure: Arc<RwLock<Option<anyhow::Error>>>,
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

    #[cfg(feature = "dont_keep_going")]
    for entry in entries.into_iter() {
        let entry: DirEntry = entry?;
        let handles_cloned = handles.clone();
        let failure = failure.clone();
        if entry.path().is_dir() {
            let handle = std::thread::spawn(move || {
                let _ = load_all(entry.path(), handles_cloned, failure);
            });
            loop {
                match handles.try_lock() {
                    Ok(ref mut handles) => {
                        handles.as_mut().expect("Always Some(_) during lock acquisition").push(handle);
                        break;
                    }
                    Err(_) => {}
                }
                spin_loop();
            }
        } else {
            //ensuring this is not optimised away
            let _ = File::open(entry.path())?;
        }
    }
    #[cfg(not(feature = "dont_keep_going"))]
    for entry in entries.into_iter().filter_map(|entry| entry.ok()) {
        if entry.path().is_dir() {
            let handles_clone = handles.clone();
            println!("Thread spawned for {} at {}", entry.path().display(), line!());
            let handle = std::thread::spawn(move || {
                let _ = load_all(entry.path(), handles_clone);
            });
            loop {
                match handles.try_lock() {
                    Ok(ref mut handles) => {
                        handles.as_mut().expect("Always Some(_) during acquisition").push(handle);
                        break;
                    }
                    Err(_) => {}
                }
                spin_loop();
            }
        } else {
            let _file = File::open(entry.path())?;
            println!("File opened at {}", entry.path().display());
        }
    }

    Ok(())
}
