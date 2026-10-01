//! perch as a library: build a [`Runtime`] from a [`Policy`], load a program,
//! run a command, get a [`RunResult`].
//!
//! ```
//! use perch::{Policy, Runtime};
//!
//! let rt = Runtime::new(Policy::default().no_shell(true).no_network(true));
//! let prog = rt
//!     .load_str("command hello\n    do\n        print \"hi\"\n    end\nend\n")
//!     .unwrap();
//! let res = rt.run(&prog, "hello", &[]);
//! assert!(res.ok);
//! assert_eq!(res.stdout, "hi\n");
//! ```
//!
//! Policies are plain data and each `Runtime` is independent: two runtimes
//! with different policies can run side by side on different threads. The
//! crate also ships the `perch` command-line binary, whose entry point is
//! [`run_cli`].
//!
//! Composition rule (VHCO): this crate is the orchestrator, the only place
//! that wires concrete infra and use-case crates together.
mod adapters;
mod flags;
mod orchestrator;
pub mod policy;
pub mod runtime;

pub use orchestrator::{run as run_cli, DEFAULT_COMMANDS_FILE, VERSION};
pub use policy::{HttpPolicy, Input, Output, Policy};
pub use runtime::{ArgInfo, CommandInfo, Issue, JsonReport, Loaded, LoadError, RunError, RunResult, Runtime};
