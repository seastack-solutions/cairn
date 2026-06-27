use axum::{routing::{get, post}, http::StatusCode, extract::State, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// An attribute macro. "Transform the thing below at compile time"
#[tokio::main]
async fn main() {

    let graph = Arc::new(build_graph());
    // async marks a function that can be paused while waiting e.g. for the network
    // Build our application: a router that maps URL paths to handler functions.
    let app = Router::new()
        .route("/", get(hello))
        .route("/route",post(route))
        .with_state(graph);

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

// ---- Phase 2: the map as a graph ----

// One connection: a path to no `to`, costing `weight` metres.
#[derive(Debug, Clone)]
struct Edge {
    to: usize,
    weight: u32,
}

#[derive(Deserialize)]
struct RouteRequest {
    start: usize,
    goal: usize,
}

#[derive(Serialize)]
struct RouteResponse {
    path: Vec<usize>,
    distance: u32,
}

// POST /route handler
async fn route(
    State(graph): State<Arc<Vec<Vec<Edge>>>>,
    Json(req): Json<RouteRequest>,
) -> Result<Json<RouteResponse>, StatusCode> {
    match shortest_path(&graph, req.start, req.goal) {
        Some((path, distance)) => Ok(Json(RouteResponse { path, distance })),
        None => Err(StatusCode::NOT_FOUND)
    }
}

//Build our 4-node example map. Nodes by index A=0, B=1, C=2, D=3.
fn build_graph() -> Vec<Vec<Edge>> {
    let mut adjacency: Vec<Vec<Edge>> = vec![Vec::new(); 4];

    //Paths are two way (you can walk back), so add each in both directions
    adjacency[0].push(Edge { to: 1, weight: 100 }); // A -> B
    adjacency[1].push(Edge { to: 0, weight: 100 }); // B -> A
    adjacency[0].push(Edge { to: 3, weight: 50 });  // A -> D
    adjacency[3].push(Edge { to: 0, weight: 50 });  // D -> A
    adjacency[1].push(Edge { to: 2, weight: 200 }); // B -> C
    adjacency[2].push(Edge { to: 1, weight: 200 }); // C -> B
    adjacency[3].push(Edge { to: 2, weight: 150 }); // D -> C
    adjacency[2].push(Edge { to: 3, weight: 150 }); // C -> D

    adjacency
}

// Find the shortest total distance from `start` to `goal`.
// Returns None if `goal` can't be reached. 
fn shortest_path(graph: &Vec<Vec<Edge>>, start: usize, goal: usize) -> Option<(Vec<usize>, u32)> {
    let n = graph.len();
    let mut dist: Vec<u32> = vec![u32::MAX; n];
    let mut visited: Vec<bool> = vec![false; n];
    let mut prev: Vec<Option<usize>> = vec![None; n]; // NEW: who we came from

    dist[start] = 0;

    loop {
        let mut current = None;
        let mut best = u32::MAX;
        for node in 0..n {
            if !visited[node] && dist[node] < best {
                best = dist[node];
                current = Some(node);
            }
        }

        let current = match current {
            Some(node) => node,
            None => break
        };

        visited[current] = true;
        if current == goal {
            break;
        }

        for edge in &graph[current] {
            let new_dist = dist[current] + edge.weight;
            if new_dist < dist[edge.to] {
                dist[edge.to] = new_dist;
                prev[edge.to] = Some(current); // NEW: breadcrumb
            }
        }
    }

    if dist[goal] == u32::MAX {
        return None; //unreachable
    }

    // NEW: walk the breadcrumbs backwards from goal to start.
    let mut path = vec![goal];
    let mut node = goal;
    while node != start {
        node = prev[node].unwrap(); // reachable => has a predecessor
        path.push(node);
    }
    path.reverse(); // we buuilt it goal start; flip it start->goal

    Some((path, dist[goal]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_has_expected_shape() {
        let graph = build_graph();
        assert_eq!(graph.len(), 4); //four nodes
        assert_eq!(graph[0].len(), 2 ); //A connects to B and D
    }

    #[test]
    fn finds_shortest_path_a_to_c() {
          let graph = build_graph();
          // A -> D -> C, total 200m. Nodes by index: A=0, D=3, C=2.
          assert_eq!(shortest_path(&graph, 0, 2), Some((vec![0, 3, 2], 200)));
      }
    
}