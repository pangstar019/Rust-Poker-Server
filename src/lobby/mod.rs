//! This module contains the definitions for the Lobby and Player structs, as well as the implementation of the game state machine.
//! 
//! The Lobby struct represents a game lobby, which can contain multiple players. It manages the game state and player interactions.
//! 
//! The Player struct represents a player in the game. It contains the player's name, hand, wallet balance, and other attributes.
//! 
//! The game state machine is implemented as a series of async functions that handle the game logic, such as dealing cards, betting rounds, and showdowns.
//! 
//! The game state machine is driven by player input, which is received via WebSocket messages. The game state machine processes the input and sends messages back to the players. 
use super::*;
use crate::Deck;
use futures_util::future::{ready, Join};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, mpsc::UnboundedSender, Mutex};
use warp::{filters::ws::WebSocket, ws::Message};
use crate::games;

// use warp::filters::ws::SplitStream;

// Lobby attribute definitions
pub const MAX_PLAYER_COUNT: i32 = 5;
const EMPTY: i32 = -1;
pub const JOINABLE: i32 = 0;
pub const START_OF_ROUND: i32 = 1;
const ANTE: i32 = 2;
const DEAL_CARDS: i32 = 3;
const FIRST_BETTING_ROUND: i32 = 4;
const DRAW: i32 = 5;
const SECOND_BETTING_ROUND: i32 = 6;
const SHOWDOWN: i32 = 7;
const END_OF_ROUND: i32 = 8;
const UPDATE_DB: i32 = 9;

// Player state definitions
pub const READY: i32 = 0;
const FOLDED: i32 = 1;
const ALL_IN: i32 = 2;
const CHECKED: i32 = 3;
const CALLED: i32 = 4;
const RAISED: i32 = 8;
pub const IN_LOBBY: i32 = 5;
pub const IN_SERVER: i32 = 6;
const IN_GAME: i32 = 7;

// Method return defintions
pub const SUCCESS: i32 = 100;
pub const FAILED: i32 = 101;
pub const SERVER_FULL: i32 = 102;
pub const GAME_LOBBY_EMPTY: i32 = 103;
pub const GAME_LOBBY_NOT_EMPTY: i32 = 104;
pub const GAME_LOBBY_FULL: i32 = 105;

pub const FIVE_CARD_DRAW: i32 = 10;
pub const SEVEN_CARD_STUD: i32 = 11;
pub const TEXAS_HOLD_EM: i32 = 12;
pub const NOT_SET: i32 = 13;


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
    pub dealer: bool,
    pub ready: bool,
    pub games_played: i32,
    pub games_won: i32,
    pub lobby: Arc<Mutex<Lobby>>,
}

impl Player {
    pub async fn get_player_input(&mut self) -> String {
        let mut return_string: String = "".to_string();
        let mut rx = self.rx.lock().await;
        while let Some(result) = rx.next().await {
            match result {
                Ok(msg) => {
                    if msg.is_close() {
                        // handle all cases of disconnection here during game---------------------
                        println!("{} has disconnected.", self.name);
                        match self.state {
                            IN_GAME => {
                                self.state = IN_LOBBY;
                            }
                            ANTE => {
                                self.state = FOLDED;
                            }
                            DEAL_CARDS => {
                                self.state = FOLDED;
                            }
                            FIRST_BETTING_ROUND => {
                                self.state = FOLDED;
                            }
                            DRAW => {
                                self.state = FOLDED;
                            }
                            SECOND_BETTING_ROUND => {
                                self.state = FOLDED;
                            }
                            _ => {
                                self.state = IN_LOBBY;
                            }
                        }
                        return "Disconnect".to_string(); // pass flag back
                    } else {
                        // handles client response here----------------
                        if let Ok(str_input) = msg.to_str() {
                            return_string = str_input.to_string();
                        } else {
                            return_string = "Error".to_string();
                        }
                        return return_string;
                    }
                }
                Err(e) => {
                    eprintln!("Error input: {}", e);
                    return "Disconnect".to_string();
                }
            }
        }
        return return_string;
    }

    pub async fn player_join_lobby(
        &mut self,
        server_lobby: Arc<Mutex<Lobby>>,
        lobby_name: String,
    ) -> i32 {
        let lobbies = server_lobby.lock().await.lobbies.lock().await.clone();
        let lobby_names_and_status = server_lobby.lock().await.lobby_names_and_status.lock().await.clone();
        println!("Lobby name entered: {}", lobby_name);
        let mut found = false;
        for (name, status) in lobby_names_and_status {
            if name == lobby_name && status == JOINABLE {
                found = true;
                break;
            }
        }
        if found{
            for lobby in lobbies {
                let lobby_guard = lobby.try_lock();
                if let Ok(mut lobby_guard) = lobby_guard {
                    if lobby_guard.name == lobby_name {
                        println!("reached2");
                        lobby_guard.add_player(self.clone()).await;
                        self.lobby = lobby.clone();
                        return SUCCESS;
                    }
                } else {
                    continue;
                }
            }
        }
        FAILED
    }
}

#[derive(Clone)]
pub struct Lobby {
    pub name: String,
    // Use Arc<Mutex<...>> so the Lobby struct can #[derive(Clone)]
    pub players: Arc<Mutex<Vec<Player>>>,
    pub lobbies: Arc<Mutex<Vec<Arc<Mutex<Lobby>>>>>,
    pub lobby_names_and_status: Arc<Mutex<Vec<(String, i32)>>>, // store lobby names and their statuses
    pub game_db: SqlitePool,
    pub deck: Deck,
    pub pot: i32,
    pub current_player_count: i32,
    pub max_player_count: i32,
    pub game_state: i32,
    pub first_betting_player: i32,
    pub game_type: i32,
}

impl Lobby {
    pub async fn new(player_count: Option<i32>, lobby_name: String) -> Self {
        Self {
            name: lobby_name,
            players: Arc::new(Mutex::new(Vec::new())),
            lobbies: Arc::new(Mutex::new(Vec::new())),
            lobby_names_and_status: Arc::new(Mutex::new(Vec::new())),
            deck: Deck::new(),
            current_player_count: 0,
            max_player_count: player_count.unwrap_or(MAX_PLAYER_COUNT),
            pot: 0,
            game_state: JOINABLE,
            first_betting_player: 0,
            game_db: SqlitePool::connect("sqlite://poker.db").await.unwrap(),
            game_type: NOT_SET,
        }
    }

    pub async fn listen_for_messages(
        &self,
        mut game_rx: mpsc::UnboundedReceiver<String>,
    ) -> String {
        while let Some(message) = game_rx.recv().await {
            println!("Received message: {}", message);
            return message;
        }
        "".to_string()
    }

    pub async fn increment_player_count(&mut self) {
        self.current_player_count += 1;
    }

    pub async fn decrement_player_count(&mut self) {
        self.current_player_count -= 1;
    }

    pub async fn get_player_count(&self) -> i32 {
        self.current_player_count
    }

    pub async fn add_player(&mut self, mut player: Player) {
        let mut players = self.players.lock().await;
        player.state = IN_LOBBY;
        players.push(player);
        self.current_player_count += 1;
        if self.current_player_count == self.max_player_count {
            self.game_state = GAME_LOBBY_FULL;
        } else {
            self.game_state = JOINABLE;
        }
    }

    pub async fn remove_player(&mut self, username: String) -> i32 {
        let mut players = self.players.lock().await;
        players.retain(|p| p.name != username);
        let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        self.lobby_wide_send(players_tx, format!("{} has disconnected from {}.", username, self.name)).await;
        println!("Player removed from {}: {}", self.name, username);
        self.current_player_count -= 1;
        if self.current_player_count == 0 {
            return GAME_LOBBY_EMPTY;
        } else {
            self.game_state = JOINABLE;
        }
        GAME_LOBBY_NOT_EMPTY
    }

    pub async fn add_lobby(&self, lobby: Arc<Mutex<Lobby>>) {
        let mut lobbies = self.lobbies.lock().await;
        lobbies.push(lobby.clone());
        // push lobby name onto the tuple vec
        let lobby_name = lobby.lock().await.name.clone();
        let lobby_status = lobby.lock().await.game_state.clone();
        self.lobby_names_and_status.lock().await.push((lobby_name, lobby_status));
    }

    pub async fn remove_lobby(&self, lobby_name: String) {
        let mut lobbies = self.lobbies.lock().await;
        let mut i = 0;
        while i < lobbies.len() {
            let curr_lobby_name = lobbies[i].lock().await.name.clone();
            if lobby_name == curr_lobby_name {
                lobbies.remove(i);
                // push lobby name onto the tuple vec
                self.lobby_names_and_status.lock().await.remove(i);
            } else {
                i += 1;
            }
        }
    }

    pub async fn get_lobby_names_and_status(&self) -> Vec<(String, i32, i32)> {
        // self.lobby_names_and_status.lock().await.clone()
        let lobbies = self.lobbies.lock().await;
        let mut result = Vec::new();
        for lobby in lobbies.iter() {
            let lobby_guard = lobby.lock().await;
            result.push((lobby_guard.name.clone(), lobby_guard.game_state, lobby_guard.game_type));
        }
        result
    }

    pub async fn lobby_exists(&self, lobby_name: String) -> bool {
        let lobby_names_and_status = self.lobby_names_and_status.lock().await;
        for (name, _) in lobby_names_and_status.iter() {
            if name == &lobby_name {
                return true;
            }
        }
        false
    }

    pub async fn get_player_names(&self) -> String {
        let players = self.players.lock().await;
        let message = players
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<String>>()
            .join("\n");
        message
    }

    pub async fn broadcast(&self, message: String) {
        println!("Broadcasting: {}", message);
        let players = self.players.lock().await;
        let mut tasks = Vec::new();
        for player in players.iter() {
            let tx = player.tx.clone();
            let msg = Message::text(message.clone());
            tasks.push(tokio::spawn(async move {
                let _ = tx.send(msg);
            }));
        }
        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
    }

    pub async fn lobby_wide_send(
        &self,
        players_tx: Vec<UnboundedSender<Message>>,
        message: String,
    ) {
        let mut tasks = Vec::new();
        for tx in players_tx.iter().cloned() {
            let msg = Message::text(message.clone());
            tasks.push(tokio::spawn(async move {
                let _ = tx.send(msg);
            }));
        }
        // Wait for all tasks to complete
        for task in tasks {
            let _ = task.await;
        }
    }

    pub async fn ready_up(&self, username: String) -> (i32, i32) {
        let mut players = self.players.lock().await;
        // self.broadcast(format!("{} is ready!", username)).await;
        if let Some(player) = players.iter_mut().find(|p| p.name == username) {
            player.ready = true;
        }
        let mut ready_player_count = 0;
        for player in players.iter() {
            if player.ready {
                ready_player_count += 1;
            }
        }
        let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        self.lobby_wide_send(players_tx, format!("{} is ready!", username))
            .await;
        return (ready_player_count, self.current_player_count);
    }

    async fn change_player_state(&self, state: i32) {
        // loop through players and change their state
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            println!("Changing {} state to: {}", player.name, state);
            player.state = state;
            player.hand.clear();
        }
    }
    pub async fn start_game(&mut self) {
        // change lobby state first so nobody can try to join anymore
        println!("Game started!");
    
        self.game_state = START_OF_ROUND;
        self.change_player_state(IN_GAME).await;
    
        if self.game_type == FIVE_CARD_DRAW {
            games::game_state_machine(self).await;
        }
        // call different game state machine (not done yet)
        else if self.game_type == SEVEN_CARD_STUD {
            games::game_state_machine(self).await;
        } else {
            games::game_state_machine(self).await;
        }
    
        self.game_state = JOINABLE;
        self.change_player_state(IN_LOBBY).await;
        self.broadcast(format!("Welcome to lobby: {}\nChoose an option:\n1. Ready:           r\n2. Show Players:    p\n3. View stats:      s\n4. Quit:            q\n\n", self.name)).await;
    }


    pub async fn update_db(&self) {
        // update the database with the new player stats
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            println!("Updating player: {}", player.name);
            println!("games played: {}", player.games_played);
            println!("games won: {}", player.games_won);
            println!("wallet: {}", player.wallet);
            sqlx::query(
                "UPDATE players SET games_played = games_played + ?1, games_won = games_won + ?2, wallet = ?3 WHERE name = ?4",
            )
            .bind(player.games_played)
            .bind(player.games_won)
            .bind(player.wallet)
            .bind(&player.name)
            .execute(&self.game_db)
            .await
            .unwrap();
    
            player.games_played = 0;
            player.games_won = 0;
        }
    }
}


