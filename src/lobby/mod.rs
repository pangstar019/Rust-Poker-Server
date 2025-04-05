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
pub const ANTE: i32 = 2;
pub const DEAL_CARDS: i32 = 3;
pub const FIRST_BETTING_ROUND: i32 = 4;
pub const DRAW: i32 = 5;
pub const SECOND_BETTING_ROUND: i32 = 6;
pub const SHOWDOWN: i32 = 7;
pub const END_OF_ROUND: i32 = 8;
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
pub const IN_GAME: i32 = 7;
pub const LOGGING_IN: i32 = 8;
pub const SPECTATOR: i32 = 9;

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

#[derive(Clone)]
pub struct Lobby {
    pub name: String,
    // Use Arc<Mutex<...>> so the Lobby struct can #[derive(Clone)]
    pub players: Arc<Mutex<Vec<Player>>>,
    pub lobbies: Arc<Mutex<Vec<Arc<Mutex<Lobby>>>>>,
    pub lobby_names_and_status: Arc<Mutex<Vec<(String, i32, i32, i32, i32)>>>, // store lobby names and their statuses
    pub game_db: SqlitePool,
    pub deck: Deck,
    pub pot: i32,
    pub current_player_count: i32,
    pub max_player_count: i32,
    pub game_state: i32,
    pub first_betting_player: i32,
    pub game_type: i32,
    pub current_max_bet: i32,
    pub community_cards: Arc<Mutex<Vec<i32>>>,
    pub current_player_turn: String,
    pub current_player_index: i32,
    pub turns_remaining: i32,
    pub spectators: Arc<Mutex<Vec<Player>>>,
}

impl Lobby {
    pub async fn new(lobby_type: i32, lobby_name: String) -> Self {
        let mut player_count = 0;
        match lobby_type {
            FIVE_CARD_DRAW => {
                player_count = 5;
            }
            SEVEN_CARD_STUD => {
                player_count = 7
            }
            TEXAS_HOLD_EM => {
                player_count = 10
            }
            _ => {
                player_count = MAX_PLAYER_COUNT;
            }

        }
        Self {
            name: lobby_name,
            players: Arc::new(Mutex::new(Vec::new())),
            lobbies: Arc::new(Mutex::new(Vec::new())),
            lobby_names_and_status: Arc::new(Mutex::new(Vec::new())),
            deck: Deck::new(),
            current_player_count: 0,
            max_player_count: player_count,
            pot: 0,
            game_state: JOINABLE,
            first_betting_player: 0,
            game_db: SqlitePool::connect("sqlite://poker.db").await.unwrap(),
            game_type: lobby_type,
            current_max_bet: 0,
            community_cards: Arc::new(Mutex::new(Vec::new())),
            current_player_turn: "".to_string(),
            current_player_index: 0,
            turns_remaining: 0,
            spectators: Arc::new(Mutex::new(Vec::new())),
        }
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

    pub async fn add_spectator(&mut self, player: Player) {
        let name = player.name.clone();
        {
            let mut spectators = self.spectators.lock().await;
            spectators.push(player);
        }

        // Broadcast that a spectator joined
        self.broadcast(format!("{} has joined as a spectator", name)).await;
    }

    pub async fn remove_player(&mut self, username: String) -> i32 {
        let mut players = self.players.lock().await;
        players.retain(|p| p.name != username);
        let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        self.lobby_wide_send(players_tx, format!("{} has disconnected from {}.", username, self.name)).await;
        println!("Player removed from {}: {}", self.name, username);
        self.current_player_count -= 1;
        
        let result = if self.current_player_count == 0 {
            GAME_LOBBY_EMPTY
        } else {
            self.game_state = JOINABLE;
            GAME_LOBBY_NOT_EMPTY
        };
        
        result
    }

    pub async fn remove_spectator(&mut self, username: String) -> bool {
        let mut spectators = self.spectators.lock().await;
        let initial_count = spectators.len();
        spectators.retain(|p| p.name != username);

        initial_count > spectators.len()
    }

    pub async fn update_lobby_names_status(&self, lobby_name: String) {
        // This method should only be called by the server lobby
        {
            let mut lobby_names_and_status = self.lobby_names_and_status.lock().await;
            for (name, status, _, player_count, _) in lobby_names_and_status.iter_mut() {
                if *name == lobby_name {
                    // Find the target lobby to get its current state
                    let lobbies = self.lobbies.lock().await;
                    for lobby in lobbies.iter() {
                        if let Ok(lobby_guard) = lobby.try_lock() {
                            if lobby_guard.name == lobby_name {
                                // Update with current values from the actual lobby
                                *status = lobby_guard.game_state;
                                *player_count = lobby_guard.current_player_count;
                                println!("count: {}", player_count);
                                break;
                            }
                        } else {
                            println!("didn't lock");
                        }
                    }
                    break;
                }
            }
        } // Mutex is automatically dropped here when the block ends
        let lobbies = self.get_lobby_names_and_status().await;
        println!("{:?}", lobbies);
        
        // After updating, broadcast the changes to all players
        self.broadcast_lobbies(None).await;
    }

    pub async fn broadcast_player_count(&self) {
        let count = self.current_player_count;
        let message = format!(r#"{{"playerCount": {}}}"#, count);
        self.broadcast(message).await;
    }

    pub async fn add_lobby(&self, lobby: Arc<Mutex<Lobby>>) {
        let mut lobbies = self.lobbies.lock().await;
        lobbies.push(lobby.clone());

        {
            let lobby_guard = lobby.lock().await;
            // push lobby name onto the tuple vec
            let lobby_name = lobby_guard.name.clone();
            let lobby_status = lobby_guard.game_state.clone();
            let lobby_type = lobby_guard.game_type.clone();
            let curr_player_count = lobby_guard.current_player_count.clone();
            let max_player_count = lobby_guard.max_player_count.clone();
            self.lobby_names_and_status.lock().await.push((lobby_name, lobby_status, lobby_type, curr_player_count, max_player_count));
        }
        // Broadcast the updated lobby list
        self.broadcast_lobbies(None).await;
    }

    // Update the remove_lobby function:
    pub async fn remove_lobby(&self, lobby_name: String) {
        let mut lobbies = self.lobbies.lock().await;
        let mut i = 0;
        while i < lobbies.len() {
            let curr_lobby_name = lobbies[i].lock().await.name.clone();
            if lobby_name == curr_lobby_name {
                lobbies.remove(i);
                // Remove from the tuple vec
                self.lobby_names_and_status.lock().await.remove(i);
            } else {
                i += 1;
            }
        }
        
        // Broadcast the updated lobby list
        self.broadcast_lobbies(None).await;
    }

    // Add a new function to broadcast the lobby list:
    pub async fn broadcast_lobbies(&self, tx: Option<mpsc::UnboundedSender<Message>>) {
        // Get the lobby information
        let lobbies = self.get_lobby_names_and_status().await;
        let mut lobby_list = Vec::new();
        
        for (lobby_name, lobby_status, lobby_type, player_count, max_player_count) in lobbies {
            // Convert status code to string
            let status = if lobby_status == JOINABLE {
                "Joinable"
            } else {
                "Not Joinable"
            };
            
            // Convert game type to readable string
            let game_type = match lobby_type {
                FIVE_CARD_DRAW => "5 Card Draw",
                SEVEN_CARD_STUD => "7 Card Stud", 
                TEXAS_HOLD_EM => "Texas Hold'em",
                _ => "Unknown"
            };
            
            lobby_list.push(serde_json::json!({
                "name": lobby_name,
                "status": status,
                "type": game_type,
                "playerCount": player_count,
                "maxPlayers": max_player_count
            }));
        }
        
        let json = serde_json::json!({
            "lobbies": lobby_list
        }).to_string();
        if tx.is_none() {
            // Get all players in the server lobby
            let players = self.players.lock().await;
            let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
            // Send the updated lobby list to all players
            for tx in players_tx {
                let _ = tx.send(Message::text(json.clone()));
            }
        } else {
            // Send the updated lobby list to the specific player
            let tx = tx.unwrap();
            let _ = tx.send(Message::text(json.clone()));
        }
    }

    pub async fn get_lobby_names_and_status(&self) -> Vec<(String, i32, i32, i32, i32)> {
        self.lobby_names_and_status.lock().await.clone()
    }

    pub async fn lobby_exists(&self, lobby_name: String) -> bool {
        let lobby_names_and_status = self.lobby_names_and_status.lock().await;
        for (name, _, _, _, _) in lobby_names_and_status.iter() {
            if name == &lobby_name {
                return true;
            }
        }
        false
    }

    pub async fn get_lobby(&self, lobby_name: String) -> Option<Arc<Mutex<Lobby>>> {
        let lobbies = self.lobbies.lock().await;
        for lobby in lobbies.iter() {
            if let Ok(lobby_guard) = lobby.try_lock() {
                if lobby_guard.name == lobby_name {
                    return Some(lobby.clone());
                }
            }
        }
        None
    }

    pub async fn get_player(&self, username: &String) -> Option<Player> {
        let players = self.players.lock().await;
        for player in players.iter() {
            if player.name == *username {
                return Some(player.clone());
            }
        }
        None
    }

    pub async fn get_player_names_and_status(&self) -> Vec<(String, bool)> {
        let players = self.players.lock().await;
        players.iter()
            .map(|p| (p.name.clone(), p.ready))
            .collect()
    }

    pub async fn broadcast_json(&self, message: String) {
        // Iterate through all players and send the message to each one
        let players = self.players.lock().await;
        for player in players.iter() {
            let _ = player.tx.send(warp::ws::Message::text(message.clone()));
        }
    }
    pub async fn broadcast(&self, message: String) {
        
        // Check if the message is already valid JSON, otherwise format it
        let json_message = if message.trim().starts_with('{') && message.trim().ends_with('}') {
            // Message appears to be JSON already
            message.clone()
        } else {
            // Wrap message in a JSON structure
            serde_json::json!({
                "message": message
            }).to_string()
        };
        
        // Broadcast to players
        {
            println!("broadcast to players");
            let players = self.players.lock().await;
            println!("players lock acquired");
            for player in players.iter() {
                let _ = player.tx.send(Message::text(json_message.clone()));
            }
        }

        // Broadcast to spectators
        {
            println!("broadcast to spectators");
            let spectators = self.spectators.lock().await;
            for spectator in spectators.iter() {
                let _ = spectator.tx.send(Message::text(json_message.clone()));
            }
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

    pub async fn check_ready(&self, username: String) -> (i32, i32) {
        let mut players = self.players.lock().await;
        // self.broadcast(format!("{} is ready!", username)).await;
        if let Some(player) = players.iter_mut().find(|p| p.name == username) {
            player.ready = !player.ready;
        }
        let mut ready_player_count = 0;
        for player in players.iter() {
            if player.ready {
                ready_player_count += 1;
            }
        }
        return (ready_player_count, self.current_player_count);
    }

    pub async fn reset_ready(&self) {
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            player.ready = false;
        }
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
    
    pub async fn setup_game(&mut self) {
        self.first_betting_player = (self.first_betting_player + 1) % self.current_player_count;
        self.current_player_index = self.first_betting_player;
        self.current_player_turn = self.players.lock().await[self.first_betting_player as usize].name.clone();
        println!("current player turn: {}", self.current_player_turn);
        self.game_state = START_OF_ROUND;
        self.deck.shuffle();
        println!("lobby {} set up for startin game.", self.name);
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

    pub async fn get_player_by_name(&self, player_name: &str) -> Option<Player> {
        let players = self.players.lock().await;
        players.iter().find(|p| p.name == player_name).cloned()
    }

    /// Updates a player's state in this lobby
    pub async fn update_player_state(&mut self, player_name: &str, new_state: i32) -> bool {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == player_name) {
            player.state = new_state;
            return true;
        }
        false
    }

    pub async fn update_player_stats(&self, player_name: &str, wallet: i32, games_played: i32, games_won: i32) {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == player_name) {
            player.wallet = wallet;
            player.games_played = games_played;
            player.games_won = games_won;
        }
    }

    pub async fn set_player_ready(&self, player_name: &str, ready_state: bool) {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == player_name) {
            player.ready = ready_state;
        }
    }
    
    
    pub async fn get_next_player(&mut self, reset: bool) {
        if reset{
            self.current_player_index = self.first_betting_player;
        } else {
            self.current_player_index = (self.current_player_index + 1) % self.current_player_count;
        }
        loop{
            if self.players.lock().await[self.current_player_index as usize].state != FOLDED {
                self.current_player_turn = self.players.lock().await[self.current_player_index as usize].name.clone();
                return;
            } else {
                self.current_player_index = (self.current_player_index + 1) % self.current_player_count;
            }
        }
    }
    
    pub async fn update_player_hand(&mut self, player_name: &str, hand: Vec<i32>) {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == player_name) {
            player.hand = hand;
        }
    }

    pub async fn display_hands(&self) {
        let mut hands_data = Vec::new();
        {
            let players = self.players.lock().await;
            // Collect all player hands
            for player in players.iter() {
                // Create a JSON structure for each player's hand
                let hand_obj = serde_json::json!({
                    "playerName": player.name,
                    "hand": player.hand,
                    "state": player.state  // Include state to check if player folded
                });
                
                hands_data.push(hand_obj);
            }
        }
        
        // Create the complete hands update message
        let hands_message = serde_json::json!({
            "type": "handsUpdate",
            "hands": hands_data
        });
        
        // Convert to string
        let hands_json = hands_message.to_string();
        
        // Broadcast to all players
        self.broadcast(hands_json).await;
        
        // Log for debugging
        println!("Broadcasted hand information to all players");
    }
    
    /// Sends the current lobby information to the client.
    pub async fn send_lobby_info(&self) {
        // Get lobby information
        let game_type = match self.game_type {
            lobby::FIVE_CARD_DRAW => "5 Card Draw",
            lobby::SEVEN_CARD_STUD => "7 Card Stud",
            lobby::TEXAS_HOLD_EM => "Texas Hold'em",
            _ => "Unknown"
        };
        
        let player_count = self.get_player_count().await;
        let max_players = match self.game_type {
            lobby::FIVE_CARD_DRAW => 5,
            lobby::SEVEN_CARD_STUD => 7,
            lobby::TEXAS_HOLD_EM => 10,
            _ => 10
        };
        // Create JSON response
        let lobby_info = serde_json::json!({
            "lobbyInfo": {
                "name": self.name,
                "gameType": game_type,
                "playerCount": player_count,
                "maxPlayers": max_players
            }
        });
        
        let players = self.players.lock().await;
        let player_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        for tx in player_tx {
            // Send lobby information to all players in the lobby
            tx.send(Message::text(lobby_info.to_string())).unwrap();
        }
    }
    
    pub async fn send_lobby_game_info(&self){
        // Create JSON response
        let game_info = serde_json::json!({
            "gameInfo": {
                "gameState": self.game_state,
                "pot": self.pot,
                "currentMaxBet": self.current_max_bet,
                "communityCards": self.community_cards.lock().await.clone(),
                "currentPlayerTurn": self.current_player_turn,
            }
        });
        
        let players = self.players.lock().await;
        let player_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        for tx in player_tx {
            // Send game information to all players in the lobby
            tx.send(Message::text(game_info.to_string())).unwrap();
        }
    }

    /// Sends the current player list to the client with hand information.
    pub async fn send_player_list(&self) {
        // Build player list with hands
        let player_info = self.get_player_names_and_status().await;
        let mut players = Vec::new();
        
        // Get all players with their hands
        {
            let players_lock = self.players.lock().await;
            for (name, ready) in player_info {
                // Find the player to get their hand and state
                if let Some(player) = players_lock.iter().find(|p| p.name == name) {
                    players.push(serde_json::json!({
                        "name": name,
                        "ready": ready,
                        "hand": player.hand,
                        "state": player.state,
                        "wallet": player.wallet,
                        "chips": player.wallet // For compatibility with UI
                    }));
                } else {
                    // Fallback if player not found
                    players.push(serde_json::json!({
                        "name": name,
                        "ready": ready,
                        "hand": Vec::<i32>::new(),
                        "state": lobby::IN_LOBBY
                    }));
                }
            }
        }
        
        // Create JSON response
        let player_list = serde_json::json!({
            "players": players,
        });

        let players = self.players.lock().await;
        let player_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
        for tx in player_tx {
            // Send player list to all players in the lobby
            tx.send(Message::text(player_list.to_string())).unwrap();
        }
        println!("players list with hands sent");
    }

}



