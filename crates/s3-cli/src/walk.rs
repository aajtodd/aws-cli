//! Walker construction for recursive operations.
//!
//! These builders are only called from recursive code paths (i.e., after
//! `--recursive` has been validated). They unconditionally recurse.
//!
//! Defaults match the Python CLI: follow symlinks, sorted, no depth limit,
//! page size 1000.

use aws_sdk_s3_transfer_manager::io::walk::{
    FsWalkContext, FsWalker, S3WalkContext, S3Walker,
};

use crate::cli::TransferArgs;

/// Build an `FsWalker` with Python-matching defaults.
///
/// `follow_symlinks` is resolved from `--follow-symlinks` / `--no-follow-symlinks`
/// (default: true). Always sorted, unlimited depth.
pub(crate) fn build_fs_walker(args: &TransferArgs) -> FsWalker {
    let follow = resolve_follow_symlinks(args);
    FsWalker::builder()
        .recursive(true)
        .follow_symlinks(follow)
        .sort(true)
        .build()
}

/// Build an `FsWalkContext` rooted at the given path.
pub(crate) fn build_fs_walk_context(root: impl Into<std::path::PathBuf>) -> FsWalkContext {
    FsWalkContext::builder().root(root).build()
}

/// Python CLI defaults for S3 prefix walking.
///
/// - No delimiter (recursive listing — all keys under prefix)
/// - Page size from `--page-size` or S3 default (1000)
/// - Prefix scopes the listing to the source key
pub(crate) fn build_s3_walker(prefix: Option<&str>, page_size: Option<i32>) -> S3Walker {
    let mut builder = S3Walker::builder();
    if let Some(p) = prefix {
        builder = builder.prefix(p);
    }
    if let Some(ps) = page_size {
        builder = builder.page_size(ps);
    }
    builder.build()
}

/// Build an `S3WalkContext` for the given bucket and client.
pub(crate) fn build_s3_walk_context(
    client: aws_sdk_s3::Client,
    bucket: impl Into<String>,
) -> S3WalkContext {
    S3WalkContext::builder().client(client).bucket(bucket).build()
}

/// Resolve follow_symlinks from CLI args.
///
/// Python default is `true`. `--no-follow-symlinks` sets it to `false`.
/// `--follow-symlinks` explicitly sets it to `true`.
fn resolve_follow_symlinks(args: &TransferArgs) -> bool {
    if args.no_follow_symlinks {
        return false;
    }
    args.follow_symlinks.unwrap_or(true)
}
