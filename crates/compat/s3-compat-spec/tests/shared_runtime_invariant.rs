//! Regression guard for the mock-backend-reuse bug.
//!
//! Bug: `#[tokio::test]` creates a fresh runtime per test. Tasks spawned on
//! one test's runtime (e.g. the mock server's accept loop) are aborted when
//! that runtime drops, leaving any static state pointing at a dead socket.
//!
//! Fix: share a single runtime across all tests via `OnceLock<Runtime>` and
//! `block_on` from plain `#[test]` functions.
//!
//! This file proves the fix works. If these tests fail, the shared-runtime
//! invariant is broken.

use std::net::SocketAddr;
use std::sync::OnceLock;
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Runtime;

fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
    })
}

static ADDR: tokio::sync::OnceCell<SocketAddr> = tokio::sync::OnceCell::const_new();

async fn addr() -> SocketAddr {
    *ADDR
        .get_or_init(|| async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let a = listener.local_addr().unwrap();
            tokio::spawn(async move {
                loop {
                    let _ = listener.accept().await;
                }
            });
            a
        })
        .await
}

#[test]
fn first_establishes_listener() {
    rt().block_on(async {
        let a = addr().await;
        TcpStream::connect(a).await.expect("first connect");
    });
}

#[test]
fn second_reuses_listener() {
    rt().block_on(async {
        let a = addr().await;
        TcpStream::connect(a)
            .await
            .expect("shared runtime keeps the accept loop alive across tests");
    });
}
