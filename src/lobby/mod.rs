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
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, mpsc::UnboundedSender, Mutex};
use warp:: ws::Message;
use games::{get_best_hand, get_hand_type};


// Lobby attribute definitions
pub const MAX_PLAYER_COUNT: i32 = 5;
pub const EMPTY: i32 = -1;
pub const JOINABLE: i32 = 0;
pub const START_OF_ROUND: i32 = 1;
pub const ANTE: i32 = 2;
pub const SMALL_AND_BIG_BLIND: i32 = 19;
pub const BIG_BLIND: i32 = 20;
pub const DEAL_COMMUNITY_CARDS: i32 = 21;
pub const DEAL_CARDS: i32 = 3;
pub const FIRST_BETTING_ROUND: i32 = 14;
pub const SECOND_BETTING_ROUND: i32 = 15;
pub const BETTING_ROUND: i32 = 16;
pub const DRAW: i32 = 5;
pub const BRING_IN: i32 = 50;

pub const SHOWDOWN: i32 = 7;
pub const END_OF_ROUND: i32 = 8;
pub const UPDATE_DB: i32 = 9;
pub const TURN_ROUND: i32 = 10;
pub const FLOP_ROUND: i32 = 11;
pub const RIVER_ROUND: i32 = 12;

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




#[derive(Clone)]
pub struct Lobby {
    pub name: String,
    // Use Arc<Mutex<...>> so the Lobby struct can #[derive(Clone)]
    pub players: Arc<Mutex<Vec<Player>>>,
    pub spectators: Arc<Mutex<Vec<Player>>>,
    pub to_be_deleted: Vec<String>,
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
    pub community_cards: Vec<i32>,
    pub current_player_turn: String,
    pub current_player_index: i32,
    pub turns_remaining: i32,
    pub deal_card_counter: i32,
    pub betting_round_counter: i32,
    pub small_blinds_done: bool,
    pub big_blinds_done: bool,
    pub call_amount: i32,
}

impl Lobby {
    pub async fn new(lobby_type: i32, lobby_name: String) -> Self {
        let player_count;
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
            spectators: Arc::new(Mutex::new(Vec::new())),
            to_be_deleted: Vec::new(),
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
            community_cards: Vec::new(),
            current_player_turn: "".to_string(),
            current_player_index: 0,
            turns_remaining: 0,
            deal_card_counter: 0,
            betting_round_counter: 0,
            small_blinds_done: false,
            big_blinds_done: false,
            call_amount: 0,
        }
    }

    pub async fn get_player_count(&self) -> i32 {
        self.current_player_count
    }

    pub async fn add_player(&mut self, mut player: Player) {
        {
            let mut players = self.players.lock().await;
            player.state = player::IN_LOBBY;
            players.push(player);
        } // Release the immutable borrow of self.players here
        
        self.current_player_count += 1;
        if self.current_player_count == self.max_player_count {
            self.game_state = GAME_LOBBY_FULL;
        } else {
            self.game_state = JOINABLE;
        }
        self.new_player_join().await;
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

    pub async fn get_player_names_and_status(&self) -> Vec<(String, bool)> {
        let players = self.players.lock().await;
        players.iter()
            .map(|p| (p.name.clone(), p.ready))
            .collect()
    }

    pub async fn broadcast_json(&self, json_message: String) {
        // Broadcast to players
        {
            let players = self.players.lock().await;
            for player in players.iter() {
                let _ = player.tx.send(Message::text(json_message.clone()));
            }
        }

        // Broadcast to spectators
        {
            let spectators = self.spectators.lock().await;
            for spectator in spectators.iter() {
                let _ = spectator.tx.send(Message::text(json_message.clone()));
            }
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
        
        // Use the broadcast_json function to send the message
        self.broadcast_json(json_message).await;
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

    pub async fn new_player_join(&mut self) {
        self.turns_remaining += 1;

        // Send initial lobby information - broad to all players in lobby
        self.send_lobby_info().await;
        self.send_player_list().await;
    }

    pub async fn check_ready(&mut self, username: String) {
        self.turns_remaining = self.current_player_count;
        let mut players = self.players.lock().await;
        // self.broadcast(format!("{} is ready!", username)).await;
        if let Some(player) = players.iter_mut().find(|p| p.name == username) {
            player.ready = !player.ready;
        }
    }

    pub async fn reset_ready(&mut self) {
        self.turns_remaining = self.current_player_count;
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            player.ready = false;
            player.state = player::IN_LOBBY;
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
        self.turns_remaining = self.current_player_count;
        {
            let mut players = self.players.lock().await;
            for player in players.iter_mut() {
                player.state = player::IN_GAME;
                player.hand.clear();
                player.current_bet = 0;
                player.ready = false;
            }
        }
        self.game_state = START_OF_ROUND;
        self.deck.shuffle();
        println!("lobby {} set up for startin game.", self.name);
    }

    pub async fn check_end_game(&self) -> bool {
        let mut active_count = 0;
        let players = self.players.lock().await;
        for player in players.iter() {
            if player.state != player::FOLDED && player.state != player::ALL_IN {
                active_count += 1;
            }
        }
        return active_count == 0 || active_count == 1;
    }

    pub async fn clear_betting(&mut self) {
        self.current_max_bet = 0;
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            player.current_bet = 0;
        }
    }

    /// Handles the showdown phase of the game, where players reveal their hands and determine the winner.
    /// The function evaluates the hands of all players and determines the winner(s) based on the hand rankings.
    /// It also updates the players' wallets and game statistics.
    /// 
    /// # Arguments
    /// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
    /// 
    /// # Returns
    /// 
    /// This function does not return a value. It updates the players' wallets and game statistics.
    /// It also handles the display of hands to active players.
    pub async fn showdown(&self) -> Vec<String> {
        let mut players = self.players.lock().await;
        let mut winning_players: Vec<Player> = Vec::new(); // keeps track of winning players at the end, accounting for draws
        let mut winning_players_names: Vec<String> = Vec::new();
        let mut winning_hand = (-1, -1, -1, -1, -1, -1); // keeps track of current highest hand, could change when incrementing between players
        let mut winning_players_indices: Vec<i32> = Vec::new();
        let mut player_hand_type: (i32, i32, i32, i32, i32, i32);
        let mut winners = Vec::new();
        for player in players.iter_mut() {
            if player.state == player::FOLDED {
                continue;
            };
            let player_hand = player.hand.clone();
            if self.game_type == SEVEN_CARD_STUD || self.game_type == TEXAS_HOLD_EM {
                // already has hand ranking
                player_hand_type = (player.hand[0], player.hand[1], player.hand[2], player.hand[3], player.hand[4], player.hand[5]);
            }
            else {
                player_hand_type = get_hand_type(&player_hand);
            }
            if player_hand_type.0 > winning_hand.0
                || (player_hand_type.0 == winning_hand.0 && player_hand_type.1 > winning_hand.1)
            {
                winning_hand = player_hand_type;
                winning_players.clear();
                winning_players_names.clear();
                winning_players.push(player.clone());
                winning_players_names.push(player.name.clone());
                winning_players_indices.clear();
            } else if player_hand_type.0 == winning_hand.0 && player_hand_type.1 == winning_hand.1 {
                winning_players.push(player.clone());
                winning_players_names.push(player.name.clone());
            }
        }
        let winning_player_count = winning_players.len();
        let pot_share = self.pot / winning_player_count as i32;
        for i in 0..winning_player_count {
            for j in 0..players.len() {
                if players[j].name == winning_players[i].name {
                    winners.push(players[j].name.clone());
                    players[j].games_won += 1;
                    players[j].wallet += pot_share;
                    println!("Player {} wins {}!", players[j].name, pot_share);
                    println!("Player {} wallet: {}", players[j].name, players[j].wallet);
                }
            }
        }
        winners
    }

    pub async fn finished_game(&mut self) {
        // Reset the game state and player hands
        self.game_state = JOINABLE;
        self.pot = 0;
        self.current_max_bet = 0;
        self.community_cards.clear();
        self.turns_remaining = self.current_player_count;
        self.deal_card_counter = 0;
        self.betting_round_counter = 0;
        self.small_blinds_done = false;
        self.big_blinds_done = false;
        
        // Reset players' states and hands
        {
            let mut players = self.players.lock().await;
            for player in players.iter_mut() {
                player.state = player::IN_LOBBY;
                player.hand.clear();
                player.current_bet = 0;
                player.ready = false;
                player.played_game = false;
                player.won_game = false;
            }
        }
        for player_name in self.to_be_deleted.clone() {
            println!("Removing disconnected player: {}", player_name);
            self.remove_player(player_name.clone()).await;
        }
        self.to_be_deleted.clear();

        self.update_db().await;
    }
    
    pub async fn showdown_texas(&self) -> Vec<String> {
        let mut players = self.players.lock().await;
        let mut winning_players: Vec<Player> = Vec::new(); // keeps track of winning players at the end, accounting for draws
        let mut winning_players_names: Vec<String> = Vec::new();
        let mut winning_hand = (-1, -1, -1, -1, -1, -1); // keeps track of current highest hand, could change when incrementing between players
        let mut winning_players_indices: Vec<i32> = Vec::new();
        let mut player_hand_type: (i32, i32, i32, i32, i32, i32);
        let mut winners: Vec<String> = Vec::new();
        for player in players.iter_mut() {
            if player.state == player::FOLDED {
                continue;
            };
            let player_hand = player.hand.clone();
            
            if self.game_type == SEVEN_CARD_STUD || self.game_type == TEXAS_HOLD_EM {
                // already has hand ranking
                player_hand_type = (player.hand[0], player.hand[1], player.hand[2], player.hand[3], player.hand[4], player.hand[5]);
            }
            else {
                player_hand_type = get_hand_type(&player_hand);
            }
            
            // Compare hand types first
            if player_hand_type.0 > winning_hand.0 {
                // Better hand type, clear previous winners
                winning_hand = player_hand_type;
                winning_players.clear();
                winning_players_names.clear();
                winning_players.push(player.clone());
                winning_players_names.push(player.name.clone());
                winning_players_indices.clear();
            } 
            // If hand types are equal, compare all five cards in sequence
            else if player_hand_type.0 == winning_hand.0 {
                // Compare first card (highest)
                if player_hand_type.1 > winning_hand.1 {
                    winning_hand = player_hand_type;
                    winning_players.clear();
                    winning_players_names.clear();
                    winning_players.push(player.clone());
                    winning_players_names.push(player.name.clone());
                    winning_players_indices.clear();
                } 
                else if player_hand_type.1 == winning_hand.1 {
                    // Compare second card
                    if player_hand_type.2 > winning_hand.2 {
                        winning_hand = player_hand_type;
                        winning_players.clear();
                        winning_players_names.clear();
                        winning_players.push(player.clone());
                        winning_players_names.push(player.name.clone());
                        winning_players_indices.clear();
                    }
                    else if player_hand_type.2 == winning_hand.2 {
                        // Compare third card
                        if player_hand_type.3 > winning_hand.3 {
                            winning_hand = player_hand_type;
                            winning_players.clear();
                            winning_players_names.clear();
                            winning_players.push(player.clone());
                            winning_players_names.push(player.name.clone());
                            winning_players_indices.clear();
                        }
                        else if player_hand_type.3 == winning_hand.3 {
                            // Compare fourth card
                            if player_hand_type.4 > winning_hand.4 {
                                winning_hand = player_hand_type;
                                winning_players.clear();
                                winning_players_names.clear();
                                winning_players.push(player.clone());
                                winning_players_names.push(player.name.clone());
                                winning_players_indices.clear();
                            }
                            else if player_hand_type.4 == winning_hand.4 {
                                // Compare fifth card
                                if player_hand_type.5 > winning_hand.5 {
                                    winning_hand = player_hand_type;
                                    winning_players.clear();
                                    winning_players_names.clear();
                                    winning_players.push(player.clone());
                                    winning_players_names.push(player.name.clone());
                                    winning_players_indices.clear();
                                }
                                else if player_hand_type.5 == winning_hand.5 {
                                    // It's a complete tie, add this player as co-winner
                                    winning_players.push(player.clone());
                                    winning_players_names.push(player.name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
        
        let winning_player_count = winning_players.len();
        let pot_share = self.pot / winning_player_count as i32;
        for i in 0..winning_player_count {
            for j in 0..players.len() {
                if players[j].name == winning_players[i].name {
                    winners.push(players[j].name.clone());
                    players[j].games_won += 1;
                    players[j].wallet += pot_share;
                    players[j].won_game = true;
                    println!("Player {} wins {}!", players[j].name, pot_share);
                    println!("Player {} wallet: {}", players[j].name, players[j].wallet);
                }
            }
        }
        winners
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

    pub async fn update_player_played_game(&mut self, player_ref: &Player) -> bool {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == player_ref.name) {
            player.played_game = true;
            return true;
        }
        false
    }

    pub async fn update_player_reference(&mut self, player_ref: &Player) {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == player_ref.name) {
            player.hand = player_ref.hand.clone();
            player.wallet = player_ref.wallet;
            player.state = player_ref.state;
            player.current_bet = player_ref.current_bet;
            player.ready = player_ref.ready;
            player.games_played = player_ref.games_played;
            player.games_won = player_ref.games_won;
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
        let player = self.players.lock().await[self.current_player_index as usize].clone();
        self.current_player_turn = player.name.clone();
        self.call_amount = self.current_max_bet - player.current_bet;
        println!("lobby call amount: {}", self.call_amount);
    }
    
    pub async fn update_player_hand(&mut self, player_name: &str, hand: Vec<i32>) {
        let mut players = self.players.lock().await;
        if let Some(player) = players.iter_mut().find(|p| p.name == player_name) {
            player.hand = hand;
        }
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
                "maxPlayers": max_players,
                "callAmount": self.call_amount,
            }
        });
        
        self.broadcast(lobby_info.to_string()).await;
    }
    
    pub async fn send_lobby_game_info(&self){
        // Create JSON response
        let game_info = serde_json::json!({
            "gameInfo": {
                "gameState": self.game_state,
                "pot": self.pot,
                "currentMaxBet": self.current_max_bet,
                "communityCards": self.community_cards.clone(),
                "currentPlayerTurn": self.current_player_turn,
                "callAmount": self.call_amount,
            }
        });
        
        self.broadcast(game_info.to_string()).await;
    }

    /// Sends the current player list to the client with hand information.
    pub async fn send_player_list(&self) {
        // Build player list with hands
        let player_info = self.get_player_names_and_status().await;
        let mut players = Vec::new();
        let mut spectators = Vec::new();
        
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
                        "state": player::IN_LOBBY
                    }));
                }
            }
        }
        {
            // Get all spectators
            let spectators_lock = self.spectators.lock().await;
            for spectator in spectators_lock.iter() {
                spectators.push(serde_json::json!({
                    "name": spectator.name,
                }));
            }
        }
        
        // Create JSON response
        let player_list = serde_json::json!({
            "players": players,
            "spectators": spectators,
        });

        self.broadcast(player_list.to_string()).await;
    }
    
    // Add this method to your Lobby impl
    pub async fn reset_current_bets(&mut self) {
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            if player.state != player::FOLDED && player.state != player::ALL_IN {
                player.current_bet = 0;
                player.state = player::IN_GAME;
            }
        }
        self.current_max_bet = 0;
    }
    
    // Also add this method to help with game reset
    pub async fn reset_game_for_new_round(&mut self) {
        // Clear the community cards
        self.community_cards.clear();
        
        // Reset pot
        self.pot = 0;
        
        // Reset current max bet
        self.current_max_bet = 0;
        
        // Reset player states and bets
        let mut players = self.players.lock().await;
        for player in players.iter_mut() {
            player.current_bet = 0;
            player.hand.clear();
            player.state = player::IN_LOBBY;
        }
        self.deck = Deck::new();
        
        // Move dealer position
        self.first_betting_player = (self.first_betting_player + 1) % self.current_player_count;
    }
}




