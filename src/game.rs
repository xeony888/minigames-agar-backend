use std::{sync::Arc, time::Duration};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc::UnboundedSender, RwLock},
    time,
};
use warp::filters::ws::Message;
#[derive(Deserialize)]
pub struct InputMessage {
    pub vx: f32,
    pub vy: f32,
}
#[derive(Deserialize)]
pub struct RoomQuery {
    pub index: Option<usize>,
    pub username: String,
}
#[derive(Serialize)]
pub struct Room {
    pub id: u8,
    pub entry_fee: i32,
    pub players: Vec<Player>,
    pub dots: Vec<Dot>,
}
pub const WIDTH: f32 = 1000.0;
pub const HEIGHT: f32 = 1000.0;
pub const MAX_V: f32 = 5.0;
const MAX_DOTS: usize = 100;
impl Room {
    pub fn step(&mut self) {
        // Move players
        for player in &mut self.players {
            player.x = (player.x + player.vx).min(WIDTH).max(0.0);
            player.y = (player.y + player.vy).min(HEIGHT).max(0.0);
        }

        // Check player vs dot collisions
        self.players.iter_mut().for_each(|player| {
            self.dots.retain(|dot| {
                let dx = player.x - dot.x;
                let dy = player.y - dot.y;
                let d_sqrd = dx * dx + dy * dy;
                let max_rad = player.radius.max(dot.radius);
                let max_rad_sqrd = (max_rad * max_rad) as f32;
                if d_sqrd < max_rad_sqrd {
                    player.radius += 1;
                    false // remove dot
                } else {
                    true // keep dot
                }
            });
        });

        // Check player vs player
        let mut to_remove = vec![];
        for i in 0..self.players.len() {
            for j in 0..self.players.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (&self.players[i], &self.players[j]);
                let dx = a.x - b.x;
                let dy = a.y - b.y;
                let d_sqrt = dx * dx + dy + dy;
                let max_rad = a.radius.max(b.radius);
                let max_rad_sqrd = (max_rad * max_rad) as f32;
                if d_sqrt < max_rad_sqrd {
                    if a.radius > b.radius {
                        to_remove.push(j);
                    }
                }
            }
        }
        to_remove.sort_unstable();
        to_remove.dedup();
        for &idx in to_remove.iter().rev() {
            self.players.remove(idx);
        }

        let state_json = serde_json::to_string(&self).unwrap();
        for player in &self.players {
            let _ = player.sender.send(Message::text(&state_json));
        }
        let dots_to_add = MAX_DOTS - self.dots.len();
        for i in 0..dots_to_add {
            let mut rng = ChaCha8Rng::from_os_rng();
            let x_coord: f32 = rng.random_range(1.0..=WIDTH);
            let y_coord: f32 = rng.random_range(1.0..=HEIGHT);
            let dot = Dot {
                x: x_coord,
                y: y_coord,
                radius: 2,
            };
            self.dots.push(dot);
        }
    }
}

#[derive(Serialize)]
pub struct Player {
    pub username: String,
    pub x: f32,
    pub y: f32,
    pub radius: u32,
    #[serde(skip)]
    pub vx: f32,
    #[serde(skip)]
    pub vy: f32,
    #[serde(skip)]
    pub sender: UnboundedSender<Message>,
}

#[derive(Serialize)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub radius: u32,
}

pub fn start_game_loop(room: Arc<RwLock<Room>>) {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_millis(33)); // ~30 FPS
        loop {
            interval.tick().await;
            let mut r = room.write().await;
            r.step();
        }
    });
}
pub type Rooms = Vec<Arc<RwLock<Room>>>;
