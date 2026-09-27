//! The async runtime every omni-me binary builds for itself.
//!
//! Why the embedded database needs a bigger worker stack than tokio's default,
//! what was measured, and what this does not cover: `docs/src/runtime.md`.

/// Worker stack size, in bytes. Rationale: `docs/src/runtime.md`.
pub const WORKER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Overrides [`WORKER_STACK_BYTES`], in MiB.
pub const WORKER_STACK_MB_ENV: &str = "OMNI_WORKER_STACK_MB";

/// Build the runtime this process runs on.
///
/// Do not swap this back for `#[tokio::main]`: its 2 MiB worker stack overflows in
/// the database engine, and a stack overflow is a SIGABRT — no error to log, no
/// retry, process gone.
pub fn build() -> std::io::Result<tokio::runtime::Runtime> {
    build_with(worker_stack_bytes())
}

/// Build the runtime and run `future` to completion on it.
///
/// For entry points where failing to build one is fatal anyway. The deployed
/// binaries call [`build`] instead, so they can report it in their own voice.
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    build()
        .expect("the async runtime must build")
        .block_on(future)
}

/// The part that takes its size as an argument, so a test can pin one.
fn build_with(stack_bytes: usize) -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(stack_bytes)
        .build()
}

/// Resolve the stack size, honouring [`WORKER_STACK_MB_ENV`].
///
/// Reported with `eprintln!` because this runs before any binary has installed a
/// tracing subscriber.
fn worker_stack_bytes() -> usize {
    let Ok(raw) = std::env::var(WORKER_STACK_MB_ENV) else {
        return WORKER_STACK_BYTES;
    };
    match raw.trim().parse::<usize>() {
        Ok(mb) if mb > 0 => mb * 1024 * 1024,
        _ => {
            eprintln!(
                "{WORKER_STACK_MB_ENV}={raw} is not a positive number of MiB; using {} MiB",
                WORKER_STACK_BYTES / (1024 * 1024)
            );
            WORKER_STACK_BYTES
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One frame of the probe below. 64 KiB of stack that cannot be elided.
    #[inline(never)]
    fn descend(depth: usize) {
        let mut frame = [0u8; 64 * 1024];
        std::hint::black_box(&mut frame);
        if depth > 0 {
            descend(depth - 1);
        }
    }

    /// Asserts the property the default exists for: a worker thread takes more
    /// stack than tokio's 2 MiB default gives it.
    ///
    /// A regression here does not fail, it aborts the test binary — that is what
    /// a stack overflow does, and it is why this is a runtime check rather than a
    /// comparison against the constant.
    #[test]
    fn a_worker_thread_survives_deeper_than_tokios_default() {
        let rt = build_with(WORKER_STACK_BYTES).expect("the runtime must build");
        rt.block_on(async {
            tokio::spawn(async { descend(96) })
                .await
                .expect("the spawned task must finish");
        });
    }

    #[test]
    fn an_unusable_override_falls_back_to_the_default() {
        // Scoped to this process, and the var is read nowhere else in the suite.
        unsafe { std::env::set_var(WORKER_STACK_MB_ENV, "not-a-number") };
        assert_eq!(worker_stack_bytes(), WORKER_STACK_BYTES);
        unsafe { std::env::set_var(WORKER_STACK_MB_ENV, "0") };
        assert_eq!(worker_stack_bytes(), WORKER_STACK_BYTES);
        unsafe { std::env::set_var(WORKER_STACK_MB_ENV, "10") };
        assert_eq!(worker_stack_bytes(), 10 * 1024 * 1024);
        unsafe { std::env::remove_var(WORKER_STACK_MB_ENV) };
    }
}
