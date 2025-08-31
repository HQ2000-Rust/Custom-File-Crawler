pub mod prelude {
    pub use crate::builder::{Crawler, marker::{Async, NonAsync}};
    pub use tokio;
    #[cfg(feature = "legacy")]
    pub use anyhow;
    #[cfg(feature = "legacy")]
    pub use crate::legacy::single_threaded::for_every_file;
}
pub mod builder {
    use regex::Regex;
    use std::{
        error::Error,
        fmt::{Debug},
        marker::PhantomData,
        path::{Path, PathBuf},
        sync::Arc
    };
    use crate::{
        builder::{
            internal::{async_run, par_run},
            marker::{Async, NonAsync}
        }
    };
    use tokio::{
        sync::RwLock,
        task::JoinSet
    };

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
        pub fn search_depth(self, depth: u32) -> Self {
            Self {
                max_depth: Some(depth),
                ..self
            }
        }

    }

    impl Crawler<NonAsync>
    {
        pub fn new() -> Self {
            Self {
                //?
                ..Self::default()
            }
        }
        pub fn run<A, E>(self, action: A) -> Result<(), Box<dyn Error + Send + 'static>>
        where
        A: FnMut(PathBuf) -> Result<(),E> + Clone + Send + Sync,
        E: Error + Send + 'static,
        {

            let start_dir = match self.start_dir {
                StartDir::Custom(path) => path,
                StartDir::Current => match std::env::current_dir() {
                    Ok(path) => path,
                    Err(e) => panic!("Could not resolve current directory: {}", e),
                },
            };

            par_run::<A, E>(action, Self {
                start_dir: StartDir::Custom(start_dir),
                ..self
            })?;

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
        pub async fn run<Fun, Fut, E>(self, action: Fun) -> Result<(), Box<dyn Error + Send + 'static>>
        where
            E: Send + Error + 'static,
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

            //this works, but the version above not :/
            loop {
                let mut lock = action_tasks.write().await;
                let task = match lock.join_next().await {
                    Some(result) => match result {
                        Ok(result) => result,
                        Err(e) => return Err(Box::new(e) as Box<dyn Error + Send + 'static>),
                    },
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
        //should - like the async version - only called with a Custom(DIR) variant!

            pub(in super::super::builder) fn par_run<A, E>(action: A ,config: Crawler<NonAsync>) -> Result<(), Box<dyn Error + Send + 'static>>
            where
                A: FnMut(PathBuf) -> Result<(),E> + Clone + Send + Sync,
                E: Error + Send + 'static,
            {
                use rayon::prelude::*;
                //'?' doesn't work here (because of the trait bounds I guess)
            let entries = match std::fs::read_dir(&config.start_dir.get_custom_dir()) {
                Ok(entry) => entry,
                Err(e) => return Err(Box::new(e) as Box<dyn Error + Send>)
            };

                //could optimize that later with .filter()
            entries.into_iter().par_bridge().map(|result| {

                let path = match result {
                    Ok(entry) => entry.path(),
                    Err(e) => return Err(Box::new(e) as Box<dyn Error + Send>)
                };
                if path.is_dir() && !matches!(config.max_depth, Some(0)) {
                    if let Some(regex) = config.folder_regex.clone() && regex.is_match(&path.to_string_lossy()) {
                    let config = Crawler {
                        start_dir: super::StartDir::Custom(path),
                        max_depth: config.max_depth.and_then(|depth| Some(depth - 1)),
                        folder_regex: config.folder_regex.clone(),
                        file_regex: config.file_regex.clone(),
                        phantom: PhantomData,
                    };
                    par_run(action.clone(), config.clone())?;
                }
            }
                else {
                    if let Some(regex) = config.file_regex.clone() && regex.is_match(&path.to_string_lossy()) {
                        match action.clone()(path) {
                            Ok(_) => {},
                            Err(e) => return Err(Box::new(e) as Box<dyn Error + Send + 'static>)
                        };
                    }
                }
                Ok(())
            }).collect::<Result<(), Box<dyn Error + Send>>>()?;
            Ok(())


        }

            use std::error::Error;
            use crate::builder::Crawler;
        use crate::builder::marker::Async;
        use std::fmt::Display;
        use std::marker::PhantomData;
        use std::path::PathBuf;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tokio::task::JoinSet;
        use crate::builder::NonAsync;

        pub(in super::super::builder) fn async_run<Fun, Fut, E>(
            recursion_tasks: Arc<RwLock<JoinSet<Result<(), std::io::Error>>>>,
            action_tasks: Arc<RwLock<JoinSet<Result<(), E>>>>,
            action: Fun,
            config: Crawler<Async>,
        ) -> impl Future<Output = Result<(), std::io::Error>> + Send
        where
            E: Send + 'static + Error,
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
                            if let Some(regex) = config.folder_regex.clone() && regex.is_match(&path.to_string_lossy()) {
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
                                );
                            }
                        } else {
                            if let Some(regex) = config.file_regex.clone() && regex.is_match(&path.to_string_lossy()) {
                                //same as above
                                drop(action_tasks.write().await.spawn(action.clone()(path)));
                            }
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

#[cfg(feature = "legacy")]
pub mod legacy {
    pub mod single_threaded {
        use std::fs::ReadDir;
        use std::path::{Path, PathBuf};
        pub fn for_every_file<F>(start_dir: impl AsRef<Path>, mut action: F) -> anyhow::Result<()>
        where
            F: FnMut(PathBuf) -> anyhow::Result<()> + Clone,
        {
            let start_dir = start_dir.as_ref().to_path_buf();
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
}
