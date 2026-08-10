//! Where mutable substrate state lives.
//!
//! Product code is immutable once installed, so nothing the substrate writes at
//! runtime may land inside it. The installed layout is
//! `<install-root>/the-athanor` for product code, `<install-root>/rooms` for
//! rooms, and `<install-root>/state/substrate` for the dotenv and the
//! PostgreSQL dumps this crate produces.
//!
//! An installed binary cannot derive `<install-root>` from anything it carries.
//! `CARGO_MANIFEST_DIR` is baked in at compile time and names the *build*
//! machine's checkout, so it is never a valid answer for an installed product —
//! not even when the build machine and the install machine are the same box.
//! Therefore:
//!
//! * `ATHANOR_STATE_DIR` names the state root explicitly and is mandatory for
//!   every installed run. It must be absolute.
//! * The compile-time checkout is used only when this process is demonstrably
//!   running *out of* that checkout's `target/` directory, i.e. under
//!   `cargo run` / `cargo test`. That is the development case and nothing else.
//! * Any other situation is an error, never a guess.

use std::{
    env,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;

/// Explicit Athanor state root, i.e. `<install-root>/state`.
pub const STATE_DIR: &str = "ATHANOR_STATE_DIR";

/// Why a state root could not be resolved. Every variant is actionable and
/// none of them exposes a build-machine path.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StateRootError {
    /// No explicit state root, and this process is not running from a
    /// development checkout.
    #[error(
        "{STATE_DIR} is not set. An installed Athanor must be told where its mutable state \
         lives; set {STATE_DIR} to the absolute path of <install-root>/state."
    )]
    Unset,
    /// `ATHANOR_STATE_DIR` was set to a relative path, which would resolve
    /// against whatever the current working directory happens to be.
    #[error("{STATE_DIR} must be an absolute path (got {0})")]
    Relative(String),
}

impl From<StateRootError> for io::Error {
    fn from(error: StateRootError) -> Self {
        io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
    }
}

/// How the state root was decided. Carried into diagnostics so an operator can
/// see *why* a path was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateRootSource {
    /// Read from `ATHANOR_STATE_DIR`.
    Environment,
    /// Derived from the checkout this binary was built in, because the binary
    /// is running out of that checkout's `target/` directory.
    DevelopmentCheckout,
}

impl StateRootSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::DevelopmentCheckout => "development_checkout",
        }
    }
}

/// The checkout this crate was compiled from. Only meaningful on the build
/// machine, and only trusted behind [`development_state_root_for`].
fn compiled_athanor_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate manifest must live at <athanor-root>/crates/house-substrate")
}

/// `<athanor-root>/state`, but only when `executable` actually lives inside
/// `<athanor-root>/target`. Cargo puts `cargo run` and `cargo test` binaries
/// there and an installed product never appears there, so this distinguishes a
/// development run from an installed one even when both happen on the machine
/// that produced the build.
fn development_state_root_for(executable: Option<&Path>) -> Option<PathBuf> {
    let root = compiled_athanor_root();
    // Canonicalize both sides so a symlinked or `..`-laden invocation still
    // compares correctly. If the target directory cannot be canonicalized the
    // build checkout is not present on this machine, which is itself the answer.
    let target = root.join("target").canonicalize().ok()?;
    let executable = executable?.canonicalize().ok()?;
    executable.starts_with(&target).then(|| root.join("state"))
}

/// The whole decision, with both inputs injected. Kept pure so it can be proven
/// without mutating process-wide environment state.
fn resolve_state_root_from(
    configured: Option<&OsStr>,
    executable: Option<&Path>,
) -> Result<(PathBuf, StateRootSource), StateRootError> {
    if let Some(configured) = configured.filter(|value| !value.is_empty()) {
        let configured = PathBuf::from(configured);
        if !configured.is_absolute() {
            return Err(StateRootError::Relative(configured.display().to_string()));
        }
        return Ok((configured, StateRootSource::Environment));
    }
    development_state_root_for(executable)
        .map(|root| (root, StateRootSource::DevelopmentCheckout))
        .ok_or(StateRootError::Unset)
}

/// The Athanor state root.
///
/// `ATHANOR_STATE_DIR` wins and must be absolute. Otherwise the only accepted
/// answer is a development checkout this process is demonstrably running from.
/// There is no third guess: an installed run without `ATHANOR_STATE_DIR` is an
/// error rather than a silent write into a build-machine path.
pub fn state_root() -> Result<PathBuf, StateRootError> {
    resolve_state_root().map(|(root, _)| root)
}

/// [`state_root`] plus the reason it was chosen.
pub fn resolve_state_root() -> Result<(PathBuf, StateRootSource), StateRootError> {
    let configured: Option<OsString> = env::var_os(STATE_DIR);
    let executable = env::current_exe().ok();
    resolve_state_root_from(configured.as_deref(), executable.as_deref())
}

/// Mutable state owned by the substrate: `<state-root>/substrate`.
pub fn substrate_state_dir() -> Result<PathBuf, StateRootError> {
    state_root().map(|root| root.join("substrate"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn absolute(tail: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(r"C:\athanor-install\{tail}"))
        } else {
            PathBuf::from(format!("/opt/athanor-install/{tail}"))
        }
    }

    /// The live process must resolve exactly the way the pure resolver says it
    /// does, so the injected-input proofs below are proofs about real behavior
    /// rather than about a parallel implementation.
    #[test]
    fn live_resolution_matches_the_pure_resolver() {
        let configured = env::var_os(STATE_DIR);
        let executable = env::current_exe().ok();
        assert_eq!(
            resolve_state_root(),
            resolve_state_root_from(configured.as_deref(), executable.as_deref()),
        );
    }

    /// A cargo test binary runs from `<athanor-root>/target`, so the
    /// development fallback resolves — and resolves to the checkout's own
    /// `state` directory, not to anything cwd-relative.
    #[test]
    fn development_root_is_the_compiled_checkout_state_dir() {
        let executable = env::current_exe().expect("a test binary has a path");
        let (root, source) = resolve_state_root_from(None, Some(&executable))
            .expect("a cargo test binary runs from <athanor-root>/target");
        assert_eq!(root, compiled_athanor_root().join("state"));
        assert_eq!(source, StateRootSource::DevelopmentCheckout);
        assert!(root.is_absolute());
    }

    /// The opposite case, and the one this module exists for: an executable
    /// that is NOT inside the build checkout's target directory gets no
    /// fallback at all. This is the installed product, including when it runs
    /// on the very machine that built it.
    #[test]
    fn installed_executable_outside_the_build_target_requires_the_variable() {
        // A real, canonicalizable path that is certainly not under
        // <athanor-root>/target: the system temp directory.
        let outside = env::temp_dir();
        assert_eq!(
            resolve_state_root_from(None, Some(&outside)).unwrap_err(),
            StateRootError::Unset,
        );
        // ...and with no executable path at all the answer is still an error,
        // never the compiled checkout.
        assert_eq!(
            resolve_state_root_from(None, None).unwrap_err(),
            StateRootError::Unset,
        );
    }

    /// An installed run supplies the variable and gets exactly that path back.
    #[test]
    fn absolute_override_wins_over_every_structural_answer() {
        let explicit = absolute("state");
        let inside_build_tree = env::current_exe().expect("a test binary has a path");
        let (root, source) =
            resolve_state_root_from(Some(explicit.as_os_str()), Some(&inside_build_tree))
                .expect("absolute override must resolve");
        assert_eq!(root, explicit);
        assert_eq!(source, StateRootSource::Environment);
        // The override must beat the development checkout, not merely coexist
        // with it: the same executable resolves differently without it.
        assert_ne!(root, compiled_athanor_root().join("state"));
    }

    /// A relative override is rejected instead of being resolved against the
    /// process working directory.
    #[test]
    fn relative_override_is_rejected() {
        let error = resolve_state_root_from(Some(OsStr::new("relative/state")), None).unwrap_err();
        assert_eq!(error, StateRootError::Relative("relative/state".into()));
        assert!(error.to_string().contains("absolute"));
    }

    /// An empty override is not a configured value. It must fall through to the
    /// structural decision rather than yielding an empty path.
    #[test]
    fn empty_override_is_not_a_configured_value() {
        let executable = env::current_exe().expect("a test binary has a path");
        let (root, source) = resolve_state_root_from(Some(OsStr::new("")), Some(&executable))
            .expect("development checkout resolves");
        assert_eq!(root, compiled_athanor_root().join("state"));
        assert_eq!(source, StateRootSource::DevelopmentCheckout);
        assert!(!root.as_os_str().is_empty());
    }

    /// `substrate_state_dir` is the resolved root plus exactly one component.
    #[test]
    fn substrate_dir_is_one_component_under_the_resolved_root() {
        let explicit = absolute("state");
        let (root, _) = resolve_state_root_from(Some(explicit.as_os_str()), None)
            .expect("absolute override must resolve");
        assert_eq!(root.join("substrate"), explicit.join("substrate"));
    }

    /// The errors an installed run receives name the variable to set and leak
    /// no build-machine path.
    #[test]
    fn errors_name_the_variable_without_leaking_the_build_tree() {
        let build_tree = env!("CARGO_MANIFEST_DIR");
        for error in [
            StateRootError::Unset,
            StateRootError::Relative("relative/state".into()),
        ] {
            let message = error.to_string();
            assert!(message.contains(STATE_DIR), "{message}");
            assert!(!message.contains(build_tree), "{message}");
        }
    }
}
