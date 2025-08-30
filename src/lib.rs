pub use builder::Crawler;
pub use builder::marker::{Async, NonAsync};
pub mod builder {
    use crate::builder::internal::async_run;
    use crate::builder::marker::{Async, NonAsync};
    use regex::Regex;
    use std::fmt::Display;
    use std::fs::DirEntry;
    use std::marker::PhantomData;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tokio::task::JoinSet;

    pub mod marker {
        #[derive(Default, Copy, Clone, Debug)]

        pub struct NonAsync;
        #[derive(Default, Copy, Clone, Debug)]
        pub struct Async;
    }

    #[derive(Default, Clone, Debug)]
    enum StartDir {
        #[default]
        Current,
        Custom(PathBuf),
    }

    impl StartDir {
        fn get_custom_dir(&self) -> &Path {
            match self {
                StartDir::Custom(path) => path,
                StartDir::Current => unreachable!("Never this variant during the recursive calls"),
            }
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct Crawler<M> {
        start_dir: StartDir,
        file_regex: Option<Regex>,
        folder_regex: Option<Regex>,
        max_depth: Option<u32>,
        phantom: PhantomData<M>,
    }

    impl<M> Crawler<M> {
        pub fn start_dir<P: AsRef<Path>>(self, path: P) -> Self {
            fn inner<M>(crawler: Crawler<M>, path: &Path) -> Crawler<M> {
                Crawler {
                    start_dir: StartDir::Custom(path.to_path_buf()),
                    ..crawler
                }
            }
            inner::<M>(self, path.as_ref())
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
        pub fn run(self, action: impl FnMut(PathBuf)) -> Result<(), std::io::Error> {
            use rayon::prelude::*;
            let start_dir = match self.start_dir {
                StartDir::Custom(path) => path,
                StartDir::Current => match std::env::current_dir() {
                    Ok(path) => path,
                    Err(e) => panic!("Could not resolve current directory: {}", e),
                },
            };
            let entries = std::fs::read_dir(&start_dir)?;

            for entry in entries.collect::<Vec<_>>().iter() {
                
            }

            Ok(())
        }
    }
    impl Crawler<Async> {
        pub fn new_async() -> Self {
            Self {
                //?
                ..Self::default()
            }
        }
        pub async fn run<Fun, Fut, E>(self, action: Fun) -> Result<(), std::io::Error>
        where
            E: Send + 'static + Display,
            Fun: Fn(PathBuf) -> Fut + Send + 'static + Clone,
            Fut: Future<Output = Result<(), E>> + Send + 'static,
        {
            let start_dir = match self.start_dir {
                StartDir::Custom(path) => path,
                StartDir::Current => match std::env::current_dir() {
                    Ok(path) => path,
                    Err(e) => panic!("Could not resolve current directory: {}", e),
                },
            };
            let entries = tokio::fs::read_dir(&start_dir).await?;

            let action_tasks: Arc<RwLock<JoinSet<Result<(), E>>>> =
                Arc::new(RwLock::new(JoinSet::new()));
            let recursion_tasks: Arc<RwLock<JoinSet<Result<(), std::io::Error>>>> =
                Arc::new(RwLock::new(JoinSet::new()));

            recursion_tasks.clone().write().await.spawn(async_run(
                recursion_tasks.clone(),
                action_tasks.clone(),
                action,
                Self {
                    //this is the start of the invariant get_custom_dir() relies on through the whole execution
                    start_dir: StartDir::Custom(start_dir),
                    ..self
                },
            ));
            /*while let Some(task) = recursion_tasks.write().await.join_next().await {
                match task {
                    Err(e) => panic!("A panic occurred during task execution: {}", e),
                    Ok(result) => result?,
                };
            }*/

            loop {
                let mut lock = action_tasks.write().await;
                let task = match lock.join_next().await {
                    Some(result) => result?,
                    None => break,
                };
                match task {
                    Err(e) => panic!("A panic occurred during task execution: {}", e),
                    Ok(_) => continue,
                }
            }

            while let Some(task) = action_tasks.write().await.join_next().await {
                dbg!("Action task iteration");
                match task {
                    Err(e) => panic!("A panic occurred during task execution: {}", e),
                    Ok(result) => match result {
                        Ok(_) => {}
                        Err(e) => panic!("An error occurred during execution: {}", e),
                    },
                };
            }
            Ok(())
        }
    }
    pub(in super::builder) mod internal {
        use crate::builder::Crawler;
        use crate::builder::marker::Async;
        use std::fmt::Display;
        use std::marker::PhantomData;
        use std::path::PathBuf;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tokio::task::JoinSet;

        pub(in super::super::builder) fn async_run<Fun, Fut, E>(
            recursion_tasks: Arc<RwLock<JoinSet<Result<(), std::io::Error>>>>,
            action_tasks: Arc<RwLock<JoinSet<Result<(), E>>>>,
            action: Fun,
            config: Crawler<Async>,
        ) -> impl Future<Output = Result<(), std::io::Error>> + Send
        where
            E: Send + 'static + Display,
            Fun: Fn(PathBuf) -> Fut + Send + 'static + Clone,
            Fut: Future<Output = Result<(), E>> + Send + 'static,
        {
            async move {
                //here, the Custom(_) invariant is important
                let mut entries = tokio::fs::read_dir(&config.start_dir.get_custom_dir()).await?;

                loop {
                    if let Some(entry) = entries.next_entry().await? {
                        let path = entry.path();
                        if path.is_dir() && !matches!(config.max_depth, Some(0)) {
                            let config = Crawler {
                                start_dir: super::StartDir::Custom(path),
                                max_depth: config.max_depth.and_then(|depth| Some(depth - 1)),
                                folder_regex: config.folder_regex.clone(),
                                file_regex: config.file_regex.clone(),
                                phantom: PhantomData,
                            };
                            //explicit drop to show that the only task error can be a panic (as of tokio 1.47.1)
                            drop(
                                recursion_tasks
                                    .write()
                                    .await
                                    .spawn(async_run::<Fun, Fut, E>(
                                        recursion_tasks.clone(),
                                        action_tasks.clone(),
                                        action.clone(),
                                        config,
                                    )),
                            )
                        } else {
                            //same as above
                            drop(action_tasks.write().await.spawn(action.clone()(path)))
                        }
                        //saving the else branch
                        continue;
                    }
                    break Ok(());
                }
            }
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
            Err(error) => return Err(anyhow::Error::from(error)),
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

    /*#[async_recursion::async_recursion]
    pub async fn for_every_file<Fun, Fut>(start_dir: impl AsRef<Path> + Send + 'static, action: Fun)
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
                            tokio::spawn(for_every_file(path, cloned_action))
                                .await
                                .unwrap();
                        } else {
                            //same
                            let cloned_action = action.clone();

                            tokio::spawn(async move { cloned_action(path).await })
                                .await
                                .unwrap();
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
    }*/
}

pub mod async_non_recursive {}
