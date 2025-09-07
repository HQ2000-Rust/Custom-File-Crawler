//! A customisable, multithreaded (optionally async) file crawler for local file systems
//! # Getting Started
//! It is recommended to just `cargo add file-crawler` it to your project and read the examples (or the [`Crawler`][crate::builder::Crawler] docs)!
//! While working with the library, refer to the `Crawler` documentation.
//!
//! # Examples
//! Below are some examples showing usage in different use cases. Reading these is is enough to understand everything for most use cases.
//! ### Example 1
//! Creation of a synchronous, multithreaded `Crawler` that prints the file name of every file in a folder:
//! ```rust,ignore
//! # fn main() -> Result<Box<dyn Error>> {
//! use file_crawler::prelude::*;
//!
//! use std::path::PathBuf;
//!
//! Crawler::new()
//!     .start_dir("C.\\user\\foo")
//!     .run(|_, path: PathBuf| {
//!         println!("{}", path.display());
//!         //placeholder error type for now
//!         Ok::<(), std::io::Error>(())
//!     })?;
//! # }
//! ```
//! ### Example 2
//! Actually, we left one argument out: the [`Context`][crate::builder::Crawler::context]!
//! We didn't need it, but if we want to know how many files we have in our folder we can do this:
//! ```rust,ignore
//! # fn main() -> Result<(), Box<dyn Error>> {
//! use file_crawler::prelude::*;
//!
//! use std::path::PathBuf;
//! use std::sync::atomic::AtomicU32;
//! use std::sync::{Arc, Mutex};
//!
//! //the context is later returned as the exact same type from the Crawler::run function
//! //so we can bind it to a variable if needed
//! let count=
//! Crawler::new()
//!     .start_dir("C:\\user\\foo")
//!     //you can of course use atomic types, this makes more sense for numbers
//!     .context(Mutex::new(0))
//!     .run(|ctx: Arc<Mutex<u32>>, path: PathBuf| {
//!         ctx.lock().unwrap()+=1;
//!         println!("{}", path.display())
//!         Ok::<(), std::io::Error>()
//!     })?;
//!  println!("Total number of files in \"C\\user\\foo\": {}", count)
//! # }
//! ```
//! ### Example 3
//! Until now the `Ok()` was more mandatory than useful. Let's look at a use case where it is a big benefit,
//! like counting the appearance of the letter '`a`' (assuming only text files are in the folder)
//! ```rust
//! # fn main() -> Result<(), Box<dyn Error>> {
//!  use file_crawler::prelude::*;
//!
//!  use std::fs::File;
//!  use std::path::PathBuf;
//!  use std::sync::Arc;
//!  use std::sync::atomic::AtomicU32;
//!
//!  let a_count=
//!  Crawler::new()
//!     .start_dir("C\\user\\foo")
//!     .context(AtomicU32::new(0))
//!     .run(|ctx: Arc<AtomicU32>, path: PathBuf| {
//!         let mut contents=String::new();
//!         let file=File::open(path)?;
//!         //NOTE: this can cause an error for files not readable as UTF-8
//!         //which returns an error and therefore terminates the crawler
//!         file.read_to_string(&mut contents)?;
//!         contents.chars().for_each(|char| if char=='a' { ctx.fetch_add(1); });
//!         Ok(())
//!     })?;
//!  println!("Appearance of the letter 'a' in \"C\\user\\foo\": {}", a_count)
//! # }
//! ```
//! ### Example 4
//! Say, you are looking all `.txt` files in a folder that's probably very big and deeply nested and
//! don't want to use all the computation power and time it would require you can do something like this:
//! ```rust
//! # fn main() -> Result<Box<dyn Error>> {
//!  use file_crawler::prelude::*;
//!
//!  use std::path::PathBuf;
//!
//!  Crawler::new()
//!     .start_dir("C\\user\\probably_very_deep_folder")
//!     //you can set a regex for every file / folder
//!     //the closure you specify is only executed for a file if its name matches the regex
//!     //this regex matches every single-line string ending in ".txt"
//!     .file_regex(r"^.*\.txt$")
//!     //sets a maximum depth (in terms of "folder layers" over each other)
//!     .search_depth(3)
//!     //you can also leave out the "PathBuf", before it was kept to make it easier to read
//!     .run(|_, path| {
//!         println!("{}", path.display());
//!         Ok(())
//!     })?;
//! # }
//! ```
//! You can also set a folder regex via [`Crawler::folder_regex`][crate::builder::Crawler::folder_regex], checking for the file regex
//! in the closure is possible, but in the future declaring it on the [`Crawler`][crate::builder::Crawler] may enable further optimisations.
//!
//! ### Example 5
//! A focus was also put on the laziness[^laziness_explanation] of the `Crawler`, so it is possible to create, store and *then* use one or more mostly without any heavy computations before running[^regex_compile_disclaimer]:
//! ```rust
//! # fn main() -> Result<(), Box<dyn Error>> {
//! use file_crawler::prelude::*;
//!
//! use std::path::PathBuf;
//!
//! const START_DIR="C:\\user\\foo";
//!
//! //the file types we are interested in
//! let regexes = [
//!                r"^.*\.txt$",
//!                r"^.*\.elf$",
//!                r"^.*\.png$"
//!               ];
//!
//! //constructing them
//! let crawlers = regexes.iter()
//!                 .map(|regex|
//!                     Crawler::new()
//!                     .file_regex(regex)
//!                     .start_dir(START_DIR)
//!                 );
//!
//! //using them
//! for crawler in crawlers.iter() {
//!     crawler.run(|_, path| {
//!         println!("{}", path.display());
//!         Ok::<(), std::io::Error>(())
//!     })?;
//! }
//! # }
//! ```
//!
//! ### Example 6
//! Like with iterators in [`rayon`](https://crates.io/crates/rayon), you can simply exchange the [`Crawler::new`][crate::builder::Crawler::new] method with the [`Crawler::new_async`][crate::builder::Crawler::new_async]
//! method to get an async crawler.
//! ```rust
//! # fn main() -> Result<(), Box<dyn Error>> {
//!  use file_crawler::prelude::*;
//!
//!  //we're using the tokio from the prelude here, no need to add it as an extra dependency
//!  use tokio::fs::File;
//!  use tokio::path::PathBuf;
//!  use std::sync::Arc;
//!  use std::sync::atomic::AtomicU32;
//!
//! //basically the same as example 3!
//!  let a_count=
//!  //only change required to make it async (except for the run(..) code)
//!  //don't forget to enable the 'async' feature!
//!  Crawler::new_async()
//!     .start_dir("C\\user\\foo")
//!     .context(AtomicU32::new(0))
//!     .run(|ctx: Arc<AtomicU32>, path: PathBuf| {
//!         let mut contents=String::new();
//!         let file=File::open(path).await?;
//!         //NOTE: this can cause an error for files not readable as UTF-8
//!         //which returns an error and therefore terminates the crawler
//!         file.read_to_string(&mut contents).await?;
//!         contents.chars().for_each(|char| if char=='a' { ctx.fetch_add(1); });
//!         Ok(())
//!     })?;
//!  println!("Appearance of the letter 'a' in \"C\\user\\foo\": {}", a_count)
//! # }
//! ```
//!
//! # Features
//! - **parallel**: enables non-async multithreaded Crawler execution via the [`rayon`](https://crates.io/crates/rayon) crate. *Enabled by default*.
//! - **async**: enables asynchronous, multithreaded[^async_disclaimer] Crawler execution via [`tokio`](https://crates.io/crates/tokio).
//! - **lazy_store**: enables creation of async and non-async `Crawler`s for later usage or interfacing with other crates, but not running them so tokio/rayon do not need to be compiled[^lazy_store_redundancy].
//!
//! # Planned Features
//! - **chili**: [`chili`](https://crates.io/crates/chili) as an optional backend (instead of [`rayon`](https://crates.io/crates/rayon), [GitHub issue](https://github.com/HQ2000-Rust/Custom-File-Crawler/issues/1))
//!
//! # Panics
//! In general - especially with the focus on the [`Crawler`][crate::builder::Crawler]'s laziness - it is desirable to have as many potential panics at creation, not at runtime (in terms of calling run on the `Crawler`).
//! Panics can (for example) occur when setting the regex to an invalid string, this may be changed in the future. So, if the creation of the `Crawler` succeeds, running will most likely *not* cause a panic.
//!
//!
//! [^async_disclaimer]: Currently, the async version demands a tokio runtime with at least 2 threads. Running it in a single threaded runtime is theoretically possible, but causes indefinite execution, so this **won't work**:
//! [^lazy_store_redundancy]: Not necessary if both the **parallel** and **async** feature are enabled.
//! [^laziness_explanation]: [lazy evaluation](https://en.wikipedia.org/wiki/Lazy_evaluation).
//! [^regex_compile_disclaimer]: one exception is setting a regex because it is compiled on setting it to emit an early panic.

pub mod prelude;
pub mod builder {
    use crate::builder::{
        context::NoContext,
        internal::{config::Config, utils::box_err},
    };
    #[cfg(any(feature = "parallel", doc))]
    use crate::builder::{internal::par_run, marker::NonAsync};
    #[cfg(any(feature = "async", doc))]
    use crate::builder::{
        internal::{async_run, utils::Execute},
        marker::Async,
    };

    #[cfg(any(feature = "async", doc))]
    use std::future::Future;
    use std::{error::Error, marker::Send};
    use regex::Regex;
    use std::{
        fmt::Debug,
        marker::PhantomData,
        path::{Path, PathBuf},
        sync::Arc,
    };
    //to not get a compile error for docs
    #[cfg(any(feature = "async", doc))]
    use tokio::sync::mpsc::error::TryRecvError;

    pub mod marker {
        #[cfg(any(feature = "parallel", feature = "lazy_store",doc))]
        #[derive(Default, Copy, Clone, Debug)]
        pub struct NonAsync;
        #[cfg(any(feature = "async", feature = "lazy_store", doc))]
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

    ///The core of this library.
    /// Create one with [`Crawler::new`][crate::builder::Crawler::new] or [`Crawler::new_async`][crate::builder::Crawler::new_async] to get started.
    #[derive(Debug, Clone, Default)]
    pub struct Crawler<A, C> {
        start_dir: StartDir,
        file_regex: Option<Regex>,
        folder_regex: Option<Regex>,
        max_depth: Option<u32>,
        context: C,
        async_marker: PhantomData<A>,
    }

    #[cfg(any(feature = "parallel", feature = "lazy_store", doc))]
    impl Crawler<NonAsync, NoContext> {
        pub fn new() -> Self {
            //if there are diverging attributes for the Sync/Async versions later
            Self { ..Self::default() }
        }
    }

    #[cfg(any(feature = "async", feature = "lazy_store", doc))]
    impl Crawler<Async, NoContext> {
        pub fn new_async() -> Self {
            //same as above
            Self { ..Self::default() }
        }
    }

    #[cfg(any(feature = "parallel", feature = "lazy_store", doc))]
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
        pub fn context<CNEW: Send + Sync>(self, context: CNEW) -> Crawler<NonAsync, CNEW> {
            self.context_(context)
        }
    }
    #[cfg(any(feature = "parallel", doc))]
    impl<C> Crawler<NonAsync, C>
    where
        C: Send + Sync,
    {
        pub fn run<A, E>(self, action: A) -> Result<C, Box<dyn Error + Send + 'static>>
        where
            A: FnMut(Arc<C>, PathBuf) -> Result<(), E> + Clone + Send + Sync,
            E: Error + Send + 'static,
        {
            let start_dir = match self.start_dir {
                StartDir::Custom(path) => path,
                StartDir::Current => std::env::current_dir().map_err(box_err)?,
            };

            let result = par_run::<A, E, C>(
                action,
                Config {
                    start_dir,
                    file_regex: self.file_regex,
                    folder_regex: self.folder_regex,
                    max_depth: self.max_depth,
                    context: Arc::new(self.context),
                },
            )?;
            Ok(Arc::into_inner(result).expect("Every other Arc should have been dropped by now"))
        }
    }
    #[cfg(any(feature = "async", feature = "lazy_store", doc))]
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
        pub fn context<CNEW: Send + Sync + 'static>(self, context: CNEW) -> Crawler<Async, CNEW> {
            self.context_(context)
        }
    }

    #[cfg(any(feature = "async", doc))]
    impl<C> Crawler<Async, C>
    where
        C: Send + Sync + 'static,
    {
        pub async fn run<Fun, Fut, E>(
            self,
            action: Fun,
        ) -> Result<C, Box<dyn Error + Send + 'static>>
        where
            E: Send + Error + 'static,
            Fun: Fn(Arc<C>, PathBuf) -> Fut + Send + 'static + Clone,
            Fut: Future<Output = Result<(), E>> + Send + 'static,
        {
            let (task_authority_tx, mut task_authority_rx) =
                tokio::sync::mpsc::unbounded_channel::<Execute<E>>();

            let task_authority = tokio::task::spawn_blocking(
                async move || -> Result<(), Box<dyn Error + Send + 'static>> {
                    let mut recursion_tasks = tokio::task::JoinSet::new();
                    let mut action_tasks = tokio::task::JoinSet::new();

                    loop {
                        match task_authority_rx.try_recv() {
                            Ok(signal) => match signal {
                                Execute::Recursion(task) => drop(recursion_tasks.spawn(task)),
                                Execute::Action(task) => drop(action_tasks.spawn(task)),
                            },
                            Err(e) => match e {
                                TryRecvError::Disconnected => {
                                    unreachable!("Senders shouldn't be dropped by now");
                                }
                                //fall-through
                                TryRecvError::Empty => {}
                            },
                        };
                        match (recursion_tasks.is_empty(), action_tasks.is_empty()) {
                            (true, true) => break,
                            (rec, act) => {
                                if !rec {
                                    if let Some(result) = recursion_tasks.try_join_next() {
                                        //unwrap to propagate panics
                                        result.unwrap().map_err(box_err)?;
                                    }
                                }
                                if !act {
                                    if let Some(result) = action_tasks.try_join_next() {
                                        //unwrap to propagate panics
                                        result.unwrap().map_err(box_err)?;
                                    }
                                }
                            }
                        }
                    }

                    Ok::<(), Box<dyn Error + Send + 'static>>(())
                },
            );

            let start_dir = match self.start_dir {
                StartDir::Custom(path) => path,
                StartDir::Current => std::env::current_dir().map_err(box_err)?,
            };

            let config = Config {
                //this is the start of the invariant get_custom_dir() relies on through the whole execution
                start_dir,
                context: Arc::new(self.context),
                max_depth: self.max_depth,
                folder_regex: self.folder_regex,
                file_regex: self.file_regex,
            };

            task_authority_tx
                .send(Execute::Recursion(async_run(
                    task_authority_tx.clone(),
                    action,
                    Config {
                        context: Arc::clone(&config.context),
                        ..config
                    },
                )))
                .expect("The Reveiver should not have been dropped by now");

            task_authority.await.unwrap().await?;

            Ok(Arc::into_inner(config.context)
                .expect("Every other clone should have been dropped by now"))
        }
    }

    pub(in crate::builder) mod internal {
        pub(crate) mod utils {

            use std::error::Error;

            pub(crate) fn box_err(
                error: impl Error + Send + 'static,
            ) -> Box<dyn Error + Send + 'static> {
                Box::new(error)
            }

            #[cfg(any(feature = "async", doc))]
            use std::pin::Pin;
            #[cfg(any(feature = "async", doc))]
            pub(in crate::builder) enum Execute<E> {
                Recursion(Pin<Box<dyn Future<Output = Result<(), std::io::Error>> + Send>>),
                Action(Pin<Box<dyn Future<Output = Result<(), E>> + Send>>),
            }
        }
        pub(in crate::builder) mod config {
            use crate::builder::{Crawler, StartDir};
            use std::path::PathBuf;
            use std::sync::Arc;

            //I could add C: ?Sized, but that would make no difference because in the Crawler C: Sized...
            #[derive(Debug, Clone)]
            pub(in crate::builder) struct Config<C> {
                pub(in crate::builder) start_dir: PathBuf,
                pub(in crate::builder) file_regex: Option<regex::Regex>,
                pub(in crate::builder) folder_regex: Option<regex::Regex>,
                pub(in crate::builder) max_depth: Option<u32>,

                pub(in crate::builder) context: Arc<C>,
            }
            impl<A, C: 'static> From<Crawler<A, C>> for Config<C> {
                fn from(value: Crawler<A, C>) -> Self {
                    Self {
                        start_dir: match value.start_dir {
                            StartDir::Custom(path) => path,
                            StartDir::Current => unreachable!("Ensure that this isn't the case"),
                        }
                        .to_path_buf(),
                        context: Arc::new(value.context),
                        file_regex: value.file_regex,
                        folder_regex: value.folder_regex,
                        max_depth: value.max_depth,
                    }
                }
            }
        }
        //this impl imposes minimal trait bounds for C, so I can have custom ones for async/non_async

        //this prevents error with trait bounds when trying to run stored crawlers

        //could make a feature gate here, but it makes no sense to disable both async and parallel
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
                pub(in crate::builder) fn validate_folder_regex(&self, str: &str) -> bool {
                    self.folder_regex
                        .as_ref()
                        .map_or(true, |regex| regex.is_match(str))
                }
                pub(in crate::builder) fn validate_file_regex(&self, str: &str) -> bool {
                    self.file_regex
                        .as_ref()
                        .map_or(true, |regex| regex.is_match(str))
                }
            }
        }
        #[cfg(any(feature = "parallel", doc))]
        pub(in crate::builder) fn par_run<A, E, C>(
            action: A,
            config: Config<C>, //1
        ) -> Result<Arc<C>, Box<dyn Error + Send + 'static>>
        where
            A: FnMut(Arc<C>, PathBuf) -> Result<(), E> + Clone + Send + Sync,
            E: Error + Send + 'static,
            C: Send + Sync,
        {
            use rayon::prelude::*;
            //'?' doesn't work here (because of the non-trivial trait bound conversions)
            let entries = std::fs::read_dir(&config.start_dir).map_err(box_err)?;
            //could optimize that later with .filter()
            entries.into_iter().par_bridge().try_for_each(|result| {
                let path = result.map_err(box_err)?.path();
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
                    }
                } else {
                    if config.validate_file_regex(&path.to_string_lossy()) {
                        action.clone()(config.context.clone(), path).map_err(box_err)?;
                    }
                }
                //just to be sure
                Ok::<(), Box<dyn Error + Send>>(())
            })?;
            Ok(config.context)
        }

        #[cfg(any(feature = "async", doc))]
        use crate::builder::internal::utils::Execute;
        use crate::builder::{
            Crawler, StartDir,
            internal::{config::Config, utils::box_err},
        };
        use ::regex::Regex;
        use std::{
            error::Error,
            path::{Path, PathBuf},
            sync::Arc,
        };

        #[cfg(any(feature = "async", doc))]
        use std::pin::Pin;
        #[cfg(any(feature = "async", doc))]
        use tokio::sync::mpsc::UnboundedSender;

        #[cfg(any(feature = "async", doc))]
        pub(in crate::builder) fn async_run<Fun, Fut, E, C>(
            authority_sender: UnboundedSender<Execute<E>>,
            action: Fun,
            config: Config<C>,
        ) -> Pin<Box<dyn Future<Output = Result<(), std::io::Error>> + Send>>
        where
            E: Send + 'static + Error,
            Fun: Fn(Arc<C>, PathBuf) -> Fut + Send + 'static + Clone,
            Fut: Future<Output = Result<(), E>> + Send + 'static,
            C: Send + Sync + 'static,
        {
            Box::pin(async move {
                //here, the Custom(_) invariant is important
                let mut entries = tokio::fs::read_dir(&config.start_dir).await?;

                loop {
                    if let Some(entry) = entries.next_entry().await? {
                        let path = entry.path();
                        if path.is_dir() && !matches!(config.max_depth, Some(0)) {
                            if config.validate_folder_regex(&path.to_string_lossy()) {
                                let config = Config {
                                    start_dir: path,
                                    max_depth: config.max_depth.and_then(|depth| Some(depth - 1)),
                                    context: Arc::clone(&config.context),
                                    file_regex: config.file_regex.clone(),
                                    folder_regex: config.folder_regex.clone(),
                                };
                                authority_sender
                                    .send(Execute::Recursion(Box::pin(
                                        async_run::<Fun, Fut, E, C>(
                                            authority_sender.clone(),
                                            action.clone(),
                                            config,
                                        ),
                                    )))
                                    .expect("The Receiver should not have been dropped by now");
                            }
                        } else {
                            if config.validate_file_regex(&path.to_string_lossy()) {
                                authority_sender
                                    .send(Execute::Action(Box::pin(action.clone()(
                                        Arc::clone(&config.context),
                                        path,
                                    ))))
                                    .expect("The Receiver should not be dropped by now");
                            }
                        }
                        //saving the else branch
                        continue;
                    }
                    break Ok(());
                }
            })
        }
    }
}
