//many parts are also found in the other two files!!


pub mod builder {
    use std::collections::HashSet;
    use std::convert::Infallible;
    use std::error::Error;
    use std::marker::PhantomData;
    use std::path::{Path, PathBuf};
    use regex::Regex;
    use tokio::task::JoinSet;
    use crate::async_recursive::for_every_file;
    use crate::builder::marker::{Async, NonAsync};

    pub mod marker {
        pub struct NonAsync;
        pub struct Async;
    }
    #[derive(Debug, Default)]
    pub struct Crawler<'a, M> {
        start_dir: StartDir,
        file_regex: Option<Regex>,
        folder_regex: Option<Regex>,
        max_depth: usize,
        phantom: PhantomData<M>,
    }

    #[derive(Debug, Default)]
    enum StartDir {
        #[default]
        Current,
        Custom(PathBuf)
    }

    impl<M> Crawler<M> {
        pub fn start_dir<P: AsRef<Path>>(self, path: P) -> Self {
            fn inner(crawler: Crawler<M>, path: &Path) -> Crawler<M> {
                Crawler {
                    start_dir: StartDir::Custom(path.to_path_buf()),
                    ..crawler
                }
            }
            inner(self, path.as_ref())
        }
        pub fn file_regex(self, regex: &str) -> Self {
            Self {
                file_regex: match Regex::new(regex) {
                    Ok(re) => Some(re),
                    Err(e) => panic!("Error compiling file regex: {}", e),
                },
                    ..self
            }
        }
        pub fn folder_regex(self, regex: &str) -> Self {
            Self {
                folder_regex: match Regex::new(regex) {
                    Ok(re) => Some(re),
                    Err(e) => panic!("Error compiling folder regex: {}", e),
                },
                ..self
            }
        }
    }

    impl Crawler<NonAsync> {
        pub fn new() -> Self {
            Self {
                //?
                ..Self::default()
            }
        }
        pub fn run(self, action: impl FnMut(std::path::PathBuf)) {

        }
    }
    impl Crawler<Async> {
        pub fn new_async() -> Self {
            Self {
                //?
                ..Self::default()
            }
        }
    pub async fn run<Fun, Fut>(self,action: Fun) -> Result<(), Box<dyn Error>>
    where
        Fun: Fn(PathBuf) -> Fut + Send + 'static + Clone,
        Fut: Future<Output=Result<(), Box<dyn Error>>>,
    {
        let start_dir = match self.start_dir {
            StartDir::Custom(path) => path,
            StartDir::Current => std::env::current_dir()?,
        };
        let entries = tokio::fs::read_dir(&start_dir).await?;

        let mut tasks=JoinSet::new();

        loop {
            match entries.next_entry().await {

                Ok(entry_opt) => {
                    if let Some(entry) = entry_opt {
                        let path = entry.path();
                        if path.is_dir() {
                            //is it possible to return the result??
                            let cloned_action = action.clone();
                            tasks.spawn(for_every_file(path, cloned_action));
                        } else {
                            //same
                            let cloned_action=action.clone();

                            tasks.spawn(cloned_action(path))
                        }
                        //saving the else branch
                        continue;
                    }
                    break;
                }
                Err(_e) => {
                    //#[cfg(feature = "dont_keep_going")]
                    //return Err(anyhow::Error::from(_e));
                }
            }
        }
        while let Some(task)= tasks.join_next() {

        }
    }
}

pub use anyhow;

pub mod single_threaded {
    use std::fs::ReadDir;
    use std::path::{Path, PathBuf};
    pub fn for_every_file<F>(start_dir: impl AsRef<Path>, mut action: F) -> anyhow::Result<()>
    where
        F: FnMut(PathBuf) -> anyhow::Result<()> + Clone,
    {
        let start_dir = start_dir.as_ref().to_path_buf();
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
            Err(error) => {
                return Err(anyhow::Error::from(error))
            },
        };

        for entry in entries.into_iter().filter_map(|entry| entry.ok()) {
            if entry.path().is_dir() {
                #[cfg(feature = "dont_keep_going")]
                for_every_file(entry.path(), action.clone())?;
                #[cfg(not(feature = "dont_keep_going"))]
                let _ = for_every_file(entry.path(), action.clone());
            } else {
                #[cfg(feature = "dont_keep_going")]
                action(entry.path())?;
                #[cfg(not(feature = "dont_keep_going"))]
                let _ = action(entry.path());
            }
        }

        Ok(())
    }
}

pub mod multi_threaded {
    use std::{
        fs::{File, ReadDir},
        hint::spin_loop,
        path::PathBuf,
        sync::{Arc, Mutex},
        thread::JoinHandle,
        time::Duration,
    };

    const FINISH_QUERY_INTERVAL: Duration = Duration::from_millis(500);

    pub fn for_every_file(
        start_dir: PathBuf,
        handles: Arc<Mutex<Option<Vec<JoinHandle<()>>>>>,
        //#[cfg(feature = "dont_keep_going")] error: Arc<RwLock<Option<anyhow::Error>>>,
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

        /*#[cfg(feature = "dont_keep_going")]
        for entry in entries.into_iter() {
            let entry: DirEntry = entry?;
            let handles_cloned = handles.clone();
            let failure = failure.clone();
            if entry.path().is_dir() {
                let handle = std::thread::spawn(move || {
                    let _ = for_every_file(entry.path(), handles_cloned, failure);
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
        }*/
        #[cfg(not(feature = "dont_keep_going"))]
        for entry in entries.into_iter().filter_map(|entry| entry.ok()) {
            if entry.path().is_dir() {
                let handles_clone = handles.clone();
                println!(
                    "Thread spawned for {} at {}",
                    entry.path().display(),
                    line!()
                );
                let handle = std::thread::spawn(move || {
                    let _ = for_every_file(entry.path(), handles_clone);
                });
                loop {
                    match handles.try_lock() {
                        Ok(ref mut handles) => {
                            handles
                                .as_mut()
                                .expect("Always Some(_) during acquisition")
                                .push(handle);
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
}
pub mod async_recursive {
    use std::path::{Path, PathBuf};

    #[async_recursion::async_recursion]
    pub async fn for_every_file<Fun, Fut>(
        start_dir: impl AsRef<Path> + Send + 'static,
        action: Fun
    )
    where
        Fun: Fn(PathBuf) -> Fut + Send + 'static + Clone,
        Fut: Future<Output: Send + 'static> + Send + 'static,
        //<F as Future>::Output: Send + 'static,
       // <F as AsyncFn(PathBuf) >::Output: Send,
    {
        let start_dir = start_dir.as_ref().to_path_buf();
        #[cfg(feature = "verify")]
        if !start_dir.is_dir() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{:?} is not a directory", start_dir),
            )));
        }
        let entries_result = tokio::fs::read_dir(&start_dir).await;
        let mut entries = match entries_result {
            Ok(entries) => entries,
            Err(_) => return (),
        };


        loop {
            match entries.next_entry().await {

                Ok(entry_opt) => {
                    if let Some(entry) = entry_opt {
                        let path = entry.path();
                        if path.is_dir() {
                            //is it possible to return the result??
                            let cloned_action = action.clone();
                            tokio::spawn(for_every_file(path, cloned_action)).await.unwrap();
                        } else {
                            //same
                            let cloned_action=action.clone();

                            tokio::spawn(async move {
                                cloned_action(path).await
                            }).await.unwrap();
                        }
                        //saving the else branch
                        continue;
                    }
                    break;
                }
                Err(_e) => {
                    //#[cfg(feature = "dont_keep_going")]
                    //return Err(anyhow::Error::from(_e));
                }
            }
        }
    }
}

pub mod async_non_recursive {

}
