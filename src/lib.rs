pub mod prelude {
    pub use crate::builder::{
        Crawler,
        marker::{Async, NonAsync},
    };
    #[cfg(feature = "legacy")]
    pub use crate::legacy::single_threaded::for_every_file;
    #[cfg(feature = "legacy")]
    pub use anyhow;
    pub use tokio;
}
pub mod builder {
    use crate::builder::context::NoContext;
    use crate::builder::internal::config::Config;
    use crate::builder::{
        internal::{async_run, par_run},
        marker::{Async, NonAsync},
    };
    use regex::Regex;
    use std::{
        error::Error,
        fmt::Debug,
        marker::PhantomData,
        path::{Path, PathBuf},
        sync::Arc,
    };
    use tokio::{sync::RwLock, task::JoinSet};

    pub mod marker {
        #[derive(Default, Copy, Clone, Debug)]

        pub struct NonAsync;
        #[derive(Default, Copy, Clone, Debug)]
        pub struct Async;
    }
    pub mod context {
        #[derive(Debug, Copy, Clone, Default)]
        pub struct NoContext;
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
    pub struct Crawler<A, C>
    {
        start_dir: StartDir,
        file_regex: Option<Regex>,
        folder_regex: Option<Regex>,
        max_depth: Option<u32>,
        context: C,
        async_marker: PhantomData<A>,
    }

    impl Crawler<NonAsync, NoContext> {
        pub fn new() -> Self {
            Self { ..Self::default() }
        }
    }

    impl Crawler<Async, NoContext> {
        pub fn new_async() -> Self {
            Self { ..Self::default() }
        }
    }

    impl<C> Crawler<NonAsync, C>
    where
        C: Send + Sync,
    {
        pub fn start_dir<P: AsRef<Path>>(self, path: P) -> Self {
            self.start_dir_(path.as_ref())
        }
        pub fn file_regex<STR: AsRef<str>>(self, regex: STR) -> Self {
            self.file_regex_(regex.as_ref())
        }
        pub fn folder_regex<STR: AsRef<str>>(self, regex: STR) -> Self {
            self.folder_regex_(regex.as_ref())
        }
        pub fn search_depth(self, depth: u32) -> Self {
            self.search_depth_(depth)
        }
        pub fn context<CNEW>(self, context: CNEW) -> Crawler<NonAsync, CNEW> {
            self.context_(context)
        }
        pub fn run<A, E>(self, action: A) -> Result<C, Box<dyn Error + Send + 'static>>
        where
            A: FnMut(Arc<C>, PathBuf) -> Result<(), E> + Clone + Send + Sync,
            E: Error + Send + 'static,
        {
            let start_dir = match self.start_dir {
                StartDir::Custom(path) => path,
                StartDir::Current => match std::env::current_dir() {
                    Ok(path) => path,
                    Err(e) => panic!("Could not resolve current directory: {}", e),
                },
            };

            let result=par_run::<A, E, C>(
                action,
                Config {
                    start_dir,
                    file_regex: self.file_regex,
                    folder_regex: self.folder_regex,
                    max_depth: self.max_depth,
                    context: Arc::new(self.context),
                },
            )?;
            Ok(
            Arc::into_inner(result).expect("Unique now?")
            )
        }
    }
    impl<C> Crawler<Async, C>
    where
        C: Send + Sync + 'static,
    {
        pub fn start_dir<P: AsRef<Path>>(self, path: P) -> Self {
            self.start_dir_(path.as_ref())
        }
        pub fn file_regex<STR: AsRef<str>>(self, regex: STR) -> Self {
            self.file_regex_(regex.as_ref())
        }
        pub fn folder_regex<STR: AsRef<str>>(self, regex: STR) -> Self {
            self.folder_regex_(regex.as_ref())
        }
        pub fn search_depth(self, depth: u32) -> Self {
            self.search_depth_(depth)
        }
        pub fn context<CNEW>(self, context: CNEW) -> Crawler<Async, CNEW> {
            self.context_(context)
        }
        pub async fn run<Fun, Fut, E>(
            self,
            action: Fun,
        ) -> Result<C, Box<dyn Error + Send + 'static>>
        where
            E: Send + Error + 'static,
            Fun: Fn(Arc<C>, PathBuf) -> Fut + Send + 'static + Clone,
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

            let config = Config {
                //this is the start of the invariant get_custom_dir() relies on through the whole execution
                start_dir,
                context: Arc::new(self.context),
                max_depth: self.max_depth,
                folder_regex: self.folder_regex,
                file_regex: self.file_regex,
            };

            recursion_tasks.clone().write().await.spawn(async_run(
                recursion_tasks.clone(),
                action_tasks.clone(),
                action,
                Config {
                    context: Arc::clone(&config.context),
                    ..config
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
            Ok(Arc::into_inner(config.context).unwrap_or_else(|| {
                unreachable!("Every other clone of this Arc should have been dropped by now")
            }))
        }
    }
    pub(in crate::builder) mod internal {
        pub(in crate::builder) mod config {
            use crate::builder::Crawler;
            use std::path::PathBuf;
            use std::sync::Arc;

            #[derive(Debug, Clone)]
            pub(in crate::builder) struct Config<C: ?Sized> {
                pub(in crate::builder) start_dir: PathBuf,
                pub(in crate::builder) file_regex: Option<regex::Regex>,
                pub(in crate::builder) folder_regex: Option<regex::Regex>,
                pub(in crate::builder) max_depth: Option<u32>,

                pub(in crate::builder) context: Arc<C>,
            }
            impl<A, C: 'static> From<Crawler<A, C>> for Config<C> {
                fn from(value: Crawler<A, C>) -> Self {
                    Self {
                        start_dir: value.start_dir.get_custom_dir().to_path_buf(),
                        context: Arc::new(value.context),
                        file_regex: value.file_regex,
                        folder_regex: value.folder_regex,
                        max_depth: value.max_depth,
                    }
                }
            }
        }
        //this impl imposes minimal trait bounds for C, so I can have custom ones for async/non_async
        impl<A, C> Crawler<A, C> {
            pub(in crate::builder) fn start_dir_(self, path: &Path) -> Self {
                Crawler {
                    start_dir: StartDir::Custom(path.to_path_buf()),
                    ..self
                }
            }
            pub(in crate::builder) fn file_regex_(self, regex: &str) -> Self {
                Self {
                    file_regex: match Regex::new(regex) {
                        Ok(re) => Some(re),
                        Err(e) => panic!("Error compiling file regex: {}", e),
                    },
                    ..self
                }
            }
            pub(in crate::builder) fn folder_regex_(self, regex: &str) -> Self {
                Self {
                    folder_regex: match Regex::new(regex) {
                        Ok(re) => Some(re),
                        Err(e) => panic!("Error compiling folder regex: {}", e),
                    },
                    ..self
                }
            }
            pub(in crate::builder) fn search_depth_(self, depth: u32) -> Self {
                Self {
                    max_depth: Some(depth),
                    ..self
                }
            }
            pub(in crate::builder) fn context_<CNEW>(self, context: CNEW) -> Crawler<A, CNEW> {
                //sadly this is necessary because of the different types...
                Crawler::<A, CNEW> {
                    context,
                    start_dir: self.start_dir,
                    file_regex: self.file_regex,
                    folder_regex: self.folder_regex,
                    max_depth: self.max_depth,
                    async_marker: self.async_marker,
                }
            }
        }
        pub(super) mod regex {
            use crate::builder::internal::config::Config;

            impl<C> Config<C> {
                pub(in super::super) fn validate_folder_regex(&self, str: &str) -> bool {
                    self.folder_regex
                        .as_ref()
                        .map_or(true, |regex| regex.is_match(str))
                }
                pub(in super::super) fn validate_file_regex(&self, str: &str) -> bool {
                    self.file_regex
                        .as_ref()
                        .map_or(true, |regex| regex.is_match(str))
                }
            }
        }
        pub(in crate::builder) fn par_run<A, E, C>(
            action: A,
            config: Config<C>,//1
        ) -> Result<Arc<C>, Box<dyn Error + Send + 'static>>
        where
            A: FnMut(Arc<C>, PathBuf) -> Result<(), E> + Clone + Send + Sync,
            E: Error + Send + 'static,
            C: Send + Sync,
        {
            use rayon::prelude::*;
            //'?' doesn't work here (because of the non-trivial trait bound conversions)
            let entries = match std::fs::read_dir(&config.start_dir) {
                Ok(entry) => entry,
                Err(e) => return Err(Box::new(e) as Box<dyn Error + Send>),
            };
            //could optimize that later with .filter()
            entries
                .into_iter()
                .par_bridge()
                .drive_unindexed()
                .map(|result| {
                    let path = match result {
                        Ok(entry) => entry.path(),
                        Err(e) => return Err(Box::new(e) as Box<dyn Error + Send>),
                    };
                    if path.is_dir() && !matches!(config.max_depth, Some(0)) {
                        if config.validate_folder_regex(&path.to_string_lossy()) {
                            let config = Config {
                                start_dir: path,
                                max_depth: config.max_depth.and_then(|depth| Some(depth - 1)),
                                folder_regex: config.folder_regex.clone(),
                                file_regex: config.file_regex.clone(),
                                context: Arc::clone(&config.context),
                            };
                            par_run(action.clone(), config)?;
                            //N drop?
                        }
                    } else {
                        if config.validate_file_regex(&path.to_string_lossy()) {
                            match action.clone()(config.context.clone(), path) {
                                Ok(_) => {}
                                Err(e) => {
                                    //N2 can't be returned here
                                    return Err(Box::new(e) as Box<dyn Error + Send + 'static>);
                                }
                            };
                        }
                    }
                    Ok(())
                })
                .collect::<Result<(), Box<dyn Error + Send>>>()?;
        Ok(config.context)
        }

        use crate::builder::internal::config::Config;
        use crate::builder::{Crawler, StartDir};
        use ::regex::Regex;
        use std::error::Error;
        use std::path::{Path, PathBuf};
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tokio::task::JoinSet;

        pub(in crate::builder) fn async_run<Fun, Fut, E, C>(
            recursion_tasks: Arc<RwLock<JoinSet<Result<(), std::io::Error>>>>,
            action_tasks: Arc<RwLock<JoinSet<Result<(), E>>>>,
            action: Fun,
            config: Config<C>,
        ) -> impl Future<Output = Result<(), std::io::Error>> + Send
        where
            E: Send + 'static + Error,
            Fun: Fn(Arc<C>, PathBuf) -> Fut + Send + 'static + Clone,
            Fut: Future<Output = Result<(), E>> + Send + 'static,
            C:  Send + Sync + 'static,
        {
            async move {
                //here, the Custom(_) invariant is important
                let mut entries = tokio::fs::read_dir(&config.start_dir).await?;

                loop {
                    if let Some(entry) = entries.next_entry().await? {
                        let path = entry.path();
                        if path.is_dir() && !matches!(config.max_depth, Some(0)) {
                            if dbg!(config.validate_folder_regex(&path.to_string_lossy())) {
                                let config = Config {
                                    start_dir: path,
                                    max_depth: config.max_depth.and_then(|depth| Some(depth - 1)),
                                    context: Arc::clone(&config.context),
                                    file_regex: config.file_regex.clone(),
                                    folder_regex: config.folder_regex.clone(),
                                };
                                //explicit drop to show that the only task error can be a panic (as of tokio 1.47.1)
                                drop(recursion_tasks.write().await.spawn(async_run::<
                                    Fun,
                                    Fut,
                                    E,
                                    C,
                                >(
                                    recursion_tasks.clone(),
                                    action_tasks.clone(),
                                    action.clone(),
                                    config,
                                )));
                            }
                        } else {
                            if config.validate_file_regex(&path.to_string_lossy()) {
                                //same as above
                                drop(
                                    action_tasks
                                        .write()
                                        .await
                                        .spawn(action.clone()(config.context.clone(), path)),
                                );
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
                    for_every_file(entry.path(), action.clone())?;
                } else {
                    action(entry.path())?;
                }
            }

            Ok(())
        }
    }
}
