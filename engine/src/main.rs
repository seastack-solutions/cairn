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
#[serde(tag = "mode",rename_all = "snake_case")]
enum RouteRequest {
    PointToPoint { start: Coord, goal: Coord },
    OutAndBack { start: Coord, target_m: u32},
    Loop { start: Coord, target_m: u32 }
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
    path: Vec<Coord>, // the route as real lat/lon points, ready todraw on a map
    distance: u32,
}

fn to_response(graph: &Graph, indices: Vec<usize>, distance: u32) -> RouteResponse {
    let path: Vec<Coord> = indices.iter().map(|&i| graph.coords[i]).collect();
    RouteResponse { distance, path}
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

fn distances_from(graph: &Graph, start: usize) -> (Vec<u32>, Vec<Option<usize>>) {
    let n = graph.adjacency.len();
    let mut dist = vec![u32::MAX; n];
    let mut visited = vec![false; n];
    let mut prev = vec![None; n];
    dist[start] = 0;

    loop {
        // pick the closest unvisited node (no heuristic here - we want ALL distances)
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
        for edge in &graph.adjacency[current] {
            let new_dist = dist[current] + edge.weight;
            if new_dist < dist[edge.to] {
                dist[edge.to] = new_dist;
                prev[edge.to] = Some(current);
            }
        }
    }
    (dist, prev)
}

fn out_and_back(graph: &Graph, start: usize, target_m: u32) -> Option<(Vec<usize>, u32)> {
    let half = target_m / 2;

    let (dist, prev) = distances_from(graph, start);

    // Fine the reachable node whose distance from start is closest to half the target.
    let mut turnaround = None;
    let mut best_diff = u32::MAX;
    for node in 0..graph.adjacency.len() {
        if dist[node] == u32::MAX || node == start {
            continue; // unreachable, or the start iteself
        }
        let diff = dist[node].abs_diff(half);
        if diff < best_diff {
            best_diff = diff;
            turnaround = Some(node);
        }
    }

    let turnaround = turnaround?; // bail out (None) if there is no where to go

    // rebuid start -> turnaround by following the breadcrumb trail.
    let mut out = vec![turnaround];
    let mut node = turnaround;
    while node != start {
        node = prev[node]?;
        out.push(node);
    }
    out.reverse();

    // Mirror it for the return leg: [start,..., turnaround,...,start]
    let mut path = out.clone();
    for &node in out.iter().rev().skip(1) {
        path.push(node);
    }

    let total = dist[turnaround] * 2;
    Some((path, total))
}


// POST /route handler
async fn route(
    State(graph): State<Arc<Graph>>,
    Json(req): Json<RouteRequest>,
) -> Result<Json<RouteResponse>, StatusCode> {
    match req {
        RouteRequest::PointToPoint { start, goal } => {
            let s = nearest_node(&graph, start);
            let g = nearest_node(&graph, goal);
            match shortest_path(&graph, s, g) {
                Some((indices, distance)) => Ok(Json(to_response(&graph, indices, distance))),
                None => Err(StatusCode::NOT_FOUND),
            }
        }
       RouteRequest::OutAndBack { start, target_m } => {
            let s = nearest_node(&graph, start);
            match out_and_back(&graph, s, target_m) {
                Some((indices, distance)) => Ok(Json(to_response(&graph, indices, distance))),
                None => Err(StatusCode::NOT_FOUND),
            }
        }
        RouteRequest::Loop { .. } => Err(StatusCode::NOT_IMPLEMENTED),
    }
}

// A* heuristic: optimistic straight-line distance from a node to the goal, in metres.
// Never overestimates the real road distance, so A* stays correct.
fn heuristic(graph: &Graph, node: usize, goal: usize) -> u32 {
    haversine_m(graph.coords[node], graph.coords[goal]).round() as u32
}

// Find the graph node geographically closest to a given point.
fn nearest_node(graph: &Graph, point: Coord) -> usize {
    let mut best = 0;
    let mut best_dist = u32::MAX;
    for i in 0..graph.coords.len() {
        let d = haversine_m(point, graph.coords[i]).round() as u32;
        if d < best_dist {
            best_dist = d;
            best = i;
        }
    }
    best
}

fn shortest_path(graph: &Graph, start: usize, goal: usize) -> Option<(Vec<usize>, u32)> {
    let n = graph.adjacency.len();
    let mut dist: Vec<u32> = vec![u32::MAX; n];   // g-cost: real distance from start
    let mut visited: Vec<bool> = vec![false; n];
    let mut prev: Vec<Option<usize>> = vec![None; n];
    dist[start] = 0;

    let mut settled = 0; // just to SEE how much work A* does

    loop {
        // --- CHANGED: pick the node with the smallest f = g + h (was: smallest g) ---
        let mut current = None;
        let mut best = u32::MAX;
        for node in 0..n {
            if visited[node] || dist[node] == u32::MAX {
                continue; // skip finished nodes AND unreached ones (MAX)
            }
            let f = dist[node] + heuristic(graph, node, goal);
            if f < best {
                best = f;
                current = Some(node);
            }
        }
        // --------------------------------------------------------------------------

        let current = match current {
            Some(node) => node,
            None => break,
        };

        visited[current] = true;
        settled += 1;
        if current == goal {
            break;
        }

        // Relaxation is UNCHANGED: still uses real distance (g), never f.
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

    println!("A* settled {settled} of {n} nodes");

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