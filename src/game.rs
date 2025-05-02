use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use tokio::{
    sync::{mpsc::UnboundedSender, RwLock},
    time::{self, sleep},
};
use warp::filters::ws::Message;

use crate::{center_within_larger, check_radial_collision, math::clamp};
#[derive(Deserialize)]
pub struct InputMessage {
    pub vx: f64,
    pub vy: f64,
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
    pub virus: Vec<Virus>,
}
pub const WIDTH: f64 = 1000.0;
pub const HEIGHT: f64 = 1000.0;
pub const MAX_V: f64 = 5.0;
const MAX_DOTS: usize = 100;
impl Room {
    pub fn step(&mut self) {
        // Move players
        for player in &mut self.players {
            player.x = (player.x + player.vx).min(WIDTH).max(0.0);
            player.y = (player.y + player.vy).min(HEIGHT).max(0.0);
        }
        let time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        self.dots.retain_mut(|dot| {
            for player in &mut self.players {
                if let Some(username) = &dot.emitter {
                    if *username == player.username {
                        continue;
                    }
                }
                if check_radial_collision!(player, dot) {
                    player.eat(&dot);
                    return false;
                }
            }
            let mut status = false;
            dot.x = (dot.x + dot.vx).min(WIDTH).max(0.0);
            dot.y = (dot.y + dot.vy).min(HEIGHT).max(0.0);
            dot.vy = clamp(dot.vy, DOT_FRICTION);
            dot.vx = clamp(dot.vx, DOT_FRICTION);
            if let Some(_) = &dot.emitter {
                let emit_time: u64 = dot.emit_time.unwrap();
                if time > emit_time + DOT_EXPIRY_SECS {
                    status = true;
                }
            }
            if status {
                dot.emitter = None;
                dot.emit_time = None;
            }
            return true;
        });
        self.virus.retain_mut(|virus| {
            for player in &mut self.players {
                if player.radius > virus.radius && center_within_larger!(player, virus) {
                    let percentage = ((virus.radius - MIN_VIRUS_RADIUS) / (MAX_VIRUS_RADIUS - MIN_VIRUS_RADIUS))
                        .min(0.5)
                        .max(0.25);
                    player.breakup(percentage, &mut self.dots);
                    return false;
                }
            }
            return true;
        });
        let mut to_remove = vec![];
        for i in 0..self.players.len() {
            for j in 0..self.players.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (&self.players[i], &self.players[j]);
                if center_within_larger!(a, b) {
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
        let dots_to_add = MAX_DOTS - self.dots.len().min(MAX_DOTS);
        for _ in 0..dots_to_add {
            let mut rng = ChaCha8Rng::from_os_rng();
            let x_coord: f64 = rng.random_range(1.0..=WIDTH);
            let y_coord: f64 = rng.random_range(1.0..=HEIGHT);
            let radius: f64 = rng.random_range(MIN_DOT_RADIUS..=MAX_DOT_RADIUS);
            let dot = Dot {
                x: x_coord,
                y: y_coord,
                vx: 0.0,
                vy: 0.0,
                radius,
                emitter: None,
                emit_time: None,
            };
            self.dots.push(dot);
        }
        let virus_to_add = MAX_VIRUS - self.virus.len();
        for _ in 0..virus_to_add {
            let mut rng = ChaCha8Rng::from_os_rng();
            let x_coord: f64 = rng.random_range(1.0..=WIDTH);
            let y_coord: f64 = rng.random_range(1.0..=HEIGHT);
            let radius = rng.random_range(MIN_VIRUS_RADIUS..=MAX_VIRUS_RADIUS);
            let virus = Virus {
                x: x_coord,
                y: y_coord,
                radius,
            };
            self.virus.push(virus);
        }
    }
}

#[derive(Serialize)]
pub struct Player {
    pub username: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    #[serde(skip)]
    pub vx: f64,
    #[serde(skip)]
    pub vy: f64,
    #[serde(skip)]
    pub sender: UnboundedSender<Message>,
}

impl Player {
    pub fn breakup(&mut self, percentage: f64, dots: &mut Vec<Dot>) {
        let to_distribute = self.radius * percentage;
        self.radius *= 1.0 - percentage;
        let mut distributed: f64 = 0.0;
        let mut rng = ChaCha8Rng::from_os_rng();
        let time = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

        while distributed < to_distribute {
            let radius = rng.random_range(MIN_DOT_RADIUS..=MAX_DOT_RADIUS);
            let angle_deg: f64 = rng.random_range(0.0..360.0);
            let angle = angle_deg.to_radians();
            let vx = MAX_DOT_SPEED * angle.cos();
            let vy = MAX_DOT_SPEED * angle.sin();

            let dot = Dot {
                x: self.x,
                y: self.y,
                vx,
                vy,
                radius,
                emitter: Some(self.username.clone()),
                emit_time: Some(time),
            };
            dots.push(dot);
            distributed += radius
        }
    }
    pub fn eat(&mut self, dot: &Dot) {
        let self_sq = (self.radius).powf(2.0);
        let dot_sq = (dot.radius).powf(2.0);
        let new_r = (self_sq + dot_sq).sqrt().round();
        self.radius = new_r;
    }
}

const MAX_DOT_RADIUS: f64 = 8.0;
const MIN_DOT_RADIUS: f64 = 3.0;
const DOT_FRICTION: f64 = 0.1;
const MAX_DOT_SPEED: f64 = 5.0;
const DOT_EXPIRY_SECS: u64 = 10;
#[derive(Serialize)]
pub struct Dot {
    pub x: f64,
    pub y: f64,
    #[serde(skip)]
    pub vx: f64,
    #[serde(skip)]
    pub vy: f64,
    pub radius: f64,
    pub emitter: Option<String>,
    #[serde(skip)]
    pub emit_time: Option<u64>,
}

const MAX_VIRUS_RADIUS: f64 = 60.0;
const MIN_VIRUS_RADIUS: f64 = 30.0;
const MAX_VIRUS: usize = 10;
#[derive(Serialize)]
pub struct Virus {
    pub x: f64,
    pub y: f64,
    pub radius: f64,
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
