use axum::{routing::get, Router};

// An attribute macro. "Transform the thing below at compile time"
#[tokio::main]
async fn main() {
    // async marks a function that can be paused while waiting e.g. for the network
    // Build our application: a router that maps URL paths to handler functions.
    let app = Router::new().route("/", get(hello));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:9000")
        .await
        .unwrap();
        // Binding can fail (port in use, no permission). Rust doesn't use exceptions; risky operations return a Result — either
        // Ok(value) or Err(problem). .unwrap() means "give me the value, but if it's an error, crash immediately."

    println!("Cairn engine listening on http://127.0.0.1:9000");

    // Hand the socker + router to axumand serve forever
    axum::serve(listener,app).await.unwrap();
}

// A handler: takes no input, returns some text
// &'static str is the return type
async fn hello() -> &'static str {
    "Hello from the Cairn route engine."
}