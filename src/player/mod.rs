use super::*;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use warp::ws::Message;


// Player state definitions
pub const READY: i32 = 0;
pub const FOLDED: i32 = 1;
pub const ALL_IN: i32 = 2;
pub const CHECKED: i32 = 3;
pub const CALLED: i32 = 4;
pub const RAISED: i32 = 8;
pub const IN_LOBBY: i32 = 5;
pub const IN_SERVER: i32 = 6;
pub const IN_GAME: i32 = 7;
pub const LOGGING_IN: i32 = 8;
pub const SPECTATOR: i32 = 9;



// Define Player struct
#[derive(Clone)]
pub struct Player {
    pub name: String,
    pub id: String,
    pub hand: Vec<i32>,
    pub wallet: i32,
    pub tx: mpsc::UnboundedSender<Message>,
    pub rx: Arc<Mutex<SplitStream<warp::ws::WebSocket>>>,
    pub state: i32,
    pub current_bet: i32,
    pub ready: bool,
    pub games_played: i32,
    pub games_won: i32,
    pub lobby: Arc<Mutex<Lobby>>,
    pub disconnected: bool,
    pub played_game: bool,
    pub won_game: bool,
}

impl Player {
    pub async fn player_join_lobby(
        &mut self,
        server_lobby: Arc<Mutex<Lobby>>,
        lobby_name: String,
        spectate: bool
    ) -> i32 {
        let lobbies = server_lobby.lock().await.lobbies.lock().await.clone();
        
        for lobby in lobbies {
            // First try with a non-blocking lock
            if let Ok(mut lobby_guard) = lobby.try_lock() {
                if lobby_guard.name == lobby_name {
                    if spectate {
                        // Join as spectator
                        self.state = SPECTATOR;
                        lobby_guard.add_spectator(self.clone()).await;
                        self.lobby = lobby.clone();
                        return SUCCESS;
                    } else {
                        // Check if game is in progress
                        if lobby_guard.game_state >= START_OF_ROUND && 
                           lobby_guard.game_state <= END_OF_ROUND {
                            // Can't join as player during game
                            return FAILED;
                        }
                        
                        // Join as regular player
                        lobby_guard.add_player(self.clone()).await;
                        self.lobby = lobby.clone();
                        return SUCCESS;
                    }
                }
            }
        }
        FAILED
    }
}