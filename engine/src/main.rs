use axum::{routing::{get, post}, http::StatusCode, extract::State, Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// An attribute macro. "Transform the thing below at compile time"
#[tokio::main]
async fn main() {
    // Read the map we downloaded, build the graph ONCE at startup.
    // (cargo runs from the `engine/` dir, so this relative path works.)
    let json = std::fs::read_to_string("data/map.json").unwrap();
    let graph = Arc::new(build_graph(&json));
    println!("Loaded {} nodes from the map", graph.adjacency.len());

    let app = Router::new()
        .route("/", get(hello))
        .route("/route", post(route))
        .with_state(graph);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:9000")
        .await
        .unwrap();

    println!("Cairn engine listening on http://127.0.0.1:9000");
    axum::serve(listener, app).await.unwrap();
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

// One node's coordinate. Deserialize: read it from Overpass JSON.
// Serialize: send it back out later. Copy: cheap to pass by value.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
struct Coord {
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct RouteRequest {
    start: usize,
    goal: usize,
}

// Our graph: neighbours per node, plus each node's coordinate.
// adjacency[i] and coords[i] describe the same node i.
struct Graph {
    adjacency: Vec<Vec<Edge>>,
    coords: Vec<Coord>,
}

// ---- Parsing the Overpass JSON ----
// We only declare the fields we need; serde ignores the rest.

#[derive(Deserialize)]
struct OverpassResponse {
    elements: Vec<Way>,
}

#[derive(Deserialize)]
struct Way {
    nodes: Vec<u64>,
    geometry: Vec<Coord>,
}

#[derive(Serialize)]
struct RouteResponse {
    path: Vec<usize>,
    distance: u32,
}

fn haversine_m(a: Coord, b: Coord) -> f64 {
    let earth_radius = 6_371_000.0; // metres
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2)
        + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * earth_radius * h.sqrt().asin()
}

fn build_graph(json: &str) -> Graph {
      let response: OverpassResponse = serde_json::from_str(json).unwrap();

      let mut id_to_index: HashMap<u64, usize> = HashMap::new();
      let mut coords: Vec<Coord> = Vec::new();
      let mut adjacency: Vec<Vec<Edge>> = Vec::new();

      for way in &response.elements {
          // Assign a compact index to each OSM node id the first time we see it.
          for i in 0..way.nodes.len() {
              let osm_id = way.nodes[i];
              if !id_to_index.contains_key(&osm_id) {
                  let index = coords.len();
                  id_to_index.insert(osm_id, index);
                  coords.push(way.geometry[i]);
                  adjacency.push(Vec::new());
              }
          }

          // Connect each consecutive pair of nodes in this way (both directions).
          for pair in way.nodes.windows(2) {
              let a = id_to_index[&pair[0]];
              let b = id_to_index[&pair[1]];
              let weight = haversine_m(coords[a], coords[b]).round() as u32;
              adjacency[a].push(Edge { to: b, weight });
              adjacency[b].push(Edge { to: a, weight });
          }
      }

      Graph { adjacency, coords }
  }



// POST /route handler
async fn route(
    State(graph): State<Arc<Graph>>,
    Json(req): Json<RouteRequest>,
) -> Result<Json<RouteResponse>, StatusCode> {
    let n = graph.adjacency.len();
    if req.start >= n || req.goal >= n {
        return Err(StatusCode::BAD_REQUEST);
    }
    match shortest_path(&graph, req.start, req.goal) {
        Some((path, distance)) => Ok(Json(RouteResponse { path, distance })),
        None => Err(StatusCode::NOT_FOUND),
    }
}

// Find the shortest total distance from `start` to `goal`.
// Returns None if `goal` can't be reached. 
fn shortest_path(graph: &Graph, start: usize, goal: usize) -> Option<(Vec<usize>, u32)> {
    let n = graph.adjacency.len();
    let mut dist: Vec<u32> = vec![u32::MAX; n];
    let mut visited: Vec<bool> = vec![false; n];
    let mut prev: Vec<Option<usize>> = vec![None; n];
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
            None => break,
        };
        visited[current] = true;
        if current == goal {
            break;
        }
        for edge in &graph.adjacency[current] {
            let new_dist = dist[current] + edge.weight;
            if new_dist < dist[edge.to] {
                dist[edge.to] = new_dist;
                prev[edge.to] = Some(current);
            }
        }
    }

    if dist[goal] == u32::MAX {
        return None;
    }
    let mut path = vec![goal];
    let mut node = goal;
    while node != start {
        node = prev[node].unwrap();
        path.push(node);
    }
    path.reverse();
    Some((path, dist[goal]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_graph() -> Graph {
        let json = std::fs::read_to_string("data/map.json").unwrap();
        build_graph(&json)
    }

    #[test]
    fn graph_loads_many_nodes() {
        let graph = test_graph();
        assert!(graph.adjacency.len() > 10);
        // every node has both a neighbour-list slot and a coordinate
        assert_eq!(graph.adjacency.len(), graph.coords.len());
    }

    #[test]
    fn edges_are_bidirectional() {
        let graph = test_graph();
        let edge = &graph.adjacency[0][0];      // first neighbour of node 0
        // does that neighbour link back to node 0?
        let links_back = graph.adjacency[edge.to].iter().any(|e| e.to == 0);
        assert!(links_back);
    }
}