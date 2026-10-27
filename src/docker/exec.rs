/// Output of a non-interactive command run inside a container.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecOutput {
    /// stdout and stderr, interleaved.
    pub output: String,
    pub exit_code: Option<i64>,
}

/// An interactive exec session with a TTY.
pub struct ExecSession {
    pub exec_id: String,
    pub output: futures::stream::BoxStream<'static, Result<Vec<u8>, String>>,
    pub input: std::pin::Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
}

/// Shells to try, in order of preference.
pub const SHELL_CANDIDATES: &[&str] = &[
    "/bin/bash",
    "/usr/bin/bash",
    "/bin/sh",
    "/busybox/sh",
    "/bin/ash",
];

/// Whether exec output means the binary itself could not be started.
pub fn is_missing_executable(out: &ExecOutput) -> bool {
    matches!(out.exit_code, Some(126) | Some(127))
        && (out.output.contains("executable file not found")
            || out.output.contains("no such file or directory"))
}
