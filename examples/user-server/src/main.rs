//! Run the trial's user server.
//!
//! The library half is what the tests drive in-process. This binary adds the two things a human
//! driving it by hand needs and a test does not: a line per request, and a shutdown that closes the
//! listener instead of dropping it.

use std::time::Instant;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

const DEFAULT_ADDR: &str = "127.0.0.1:3001";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = user_server::AppState::in_memory().await?;
    // D4d's delete cascade, on only when a todo server is named. Absent, a user delete removes the
    // user alone, which is correct for a deployment with no todos to orphan.
    if let Ok(todo_server) = std::env::var("TODO_SERVER_URL") {
        println!("user-server cascading user deletes to {todo_server}");
        state = state.with_todo_server(todo_server);
    }
    let addr = std::env::var("USER_SERVER_ADDR").unwrap_or_else(|_| DEFAULT_ADDR.to_owned());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("user-server listening on http://{}", listener.local_addr()?);
    println!("storage is in memory: everything here is gone when this process is");

    let app = user_server::router(state).layer(axum::middleware::from_fn(log_request));
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    println!("user-server stopped");
    Ok(())
}

/// One line per request. The trial's failure mode is a wire disagreement, and a wire disagreement
/// is invisible without knowing which path was asked for and what it answered.
async fn log_request(request: Request, next: Next) -> Response {
    let (method, path) = (request.method().clone(), request.uri().path().to_owned());
    let started = Instant::now();
    let response = next.run(request).await;
    println!(
        "{method} {path} -> {} in {}ms",
        response.status(),
        started.elapsed().as_millis()
    );
    response
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
