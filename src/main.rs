use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use game::{start_game_loop, InputMessage, Player, Room, RoomQuery, Rooms, HEIGHT, MAX_V, WIDTH};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::Serialize;
use tokio::sync::{mpsc, RwLock};
use warp::http::StatusCode;
use warp::reject::Rejection;
use warp::reply::Reply;
use warp::ws::{Message, WebSocket};
use warp::Filter;
pub mod game;
pub mod math;

async fn handle_connection(ws: WebSocket, rooms: Rooms, query: RoomQuery) {
    let (mut ws_tx, mut ws_rx) = ws.split();

    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    let room: Arc<RwLock<Room>> = if let Some(index) = query.index {
        rooms[index].clone()
    } else {
        rooms[0].clone()
    };
    // Spawn task to forward messages to socket
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_tx.send(msg).await.is_err() {
                break;
            }
        }
    });
    println!("Client {} connected", query.username);
    let mut rng = ChaCha8Rng::from_os_rng();
    let x_coord: f64 = rng.random_range(1.0..=WIDTH);
    let y_coord: f64 = rng.random_range(1.0..=HEIGHT);
    let player = Player {
        username: query.username.clone(),
        x: x_coord,
        y: y_coord,
        radius: 10.0,
        vx: 0.0,
        vy: 0.0,
        sender: tx.clone(),
    };
    let mut room_write = room.write().await;
    room_write.players.push(player);
    drop(room_write);
    while let Some(Ok(msg)) = ws_rx.next().await {
        if msg.is_text() {
            if let Ok(input) = serde_json::from_str::<InputMessage>(msg.to_str().unwrap()) {
                let mut room = room.write().await;
                if let Some(player) = room.players.iter_mut().find(|p| p.sender.same_channel(&tx)) {
                    player.vx = input.vx.min(MAX_V).max(-MAX_V);
                    player.vy = input.vy.min(MAX_V).max(-MAX_V);
                }
            }
        }
    }
    room_write = room.write().await;
    room_write.players.retain(|player| player.username != query.username);
    println!("Client {} disconnected", query.username);
}
async fn handle_rejection(err: Rejection) -> Result<impl Reply, std::convert::Infallible> {
    println!("Other error: {:?}", err);
    Ok(warp::reply::with_status("Internal Server Error", StatusCode::INTERNAL_SERVER_ERROR))
}
#[derive(Serialize)]
struct Data {
    pub id: u8,
    pub players: usize,
}
async fn handle_rooms_data(rooms: Rooms) -> Result<impl Reply, Rejection> {
    let mut data: Vec<Data> = Vec::new();
    for room in rooms {
        let read = room.read().await;
        data.push(Data {
            id: read.id,
            players: read.players.len(),
        });
    }
    return Ok(warp::reply::json(&serde_json::to_string(&data).unwrap()));
}
#[tokio::main]
async fn main() {
    let mut rooms: Rooms = Vec::new();
    for i in 0..4 {
        let room: Room = Room {
            id: i,
            entry_fee: 5, // cents,
            players: Vec::new(),
            dots: Vec::new(),
            virus: Vec::new(),
        };
        let arc = Arc::new(RwLock::new(room));
        start_game_loop(arc.clone());
        rooms.push(arc);
    }
    fn with_rooms(rooms: Rooms) -> impl Filter<Extract = (Rooms,), Error = std::convert::Infallible> + Clone {
        return warp::any().map(move || rooms.clone()); // .clone() just cloned the ref because it is an Arc
    }
    let cors = warp::cors()
        .allow_any_origin()
        .allow_credentials(true)
        .allow_headers(vec!["User-Agent", "Content-Type", "Authorization", "X-Auth-Token", "X-Requested-With"])
        .allow_methods(vec!["GET", "POST", "PUT", "DELETE", "OPTIONS"])
        .max_age(3600)
        .build();
    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_rooms(rooms.clone())) // Pass rooms
        .and(warp::query::<RoomQuery>())
        .map(|ws: warp::ws::Ws, rooms: Rooms, query: RoomQuery| {
            let rooms = rooms.clone();
            ws.on_upgrade(move |socket| handle_connection(socket, rooms, query))
        });
    let rooms_data_route = warp::path("rooms")
        .and(warp::get())
        .and(with_rooms(rooms.clone()))
        .and_then(handle_rooms_data);

    let routes = ws_route.or(rooms_data_route).recover(handle_rejection).with(cors);

    // Run the server
    println!("Server running at 0.0.0.0:8080");
    warp::serve(routes).run(([0, 0, 0, 0], 8080)).await;
}
