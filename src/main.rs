//! # Poker Server
//! 
//! This module contains the main function for the Poker server.
//! 
//! The server is implemented using the `warp` web framework and provides a WebSocket
//! interface for clients to connect to. The server manages player registration, login,
//! and lobby creation, as well as game logic for playing Poker.
//! 
//! The server uses a SQLite database to store player information and statistics.
//! 
//! The server supports the following features:
//! - Player registration and login
//! - Lobby creation and joining
//! - Game setup and management
//! - Player statistics tracking
//! 
//! The server is designed to handle multiple concurrent clients and games, with each
//! client connecting via a WebSocket connection.
//! 
//! The server is implemented using asynchronous Rust with the `tokio` runtime.
//! 
//! # Usage
//! 
//! To start the server, run the following command:
//! 
//! ```bash
//! cargo run
//! ```
//! 
//! The server will start on `localhost:1112` and listen for incoming WebSocket connections.
//! 
//! Clients can connect to the server using a WebSocket client, such as `websocat` or a web browser.
//! 
//! # Dependencies
//! 
//! The server uses the following dependencies:
//! - `warp` for the web framework and WebSocket handling
//! - `sqlx` for the SQLite database interaction
//! - `uuid` for generating unique player IDs
//! - `tokio` for the asynchronous runtime
//! 
//! # Modules
//! 
//! The server is organized into the following modules:
//! - `database` - Database module for player registration, login, and statistics
//! - `deck` - Deck module for managing the deck of cards
//! - `lobby` - Lobby module for managing players and lobbies
mod database;
mod deck;
mod lobby;
mod games;

use futures_util::stream::SplitStream;
use futures_util::{StreamExt, SinkExt};
use rand::seq::index;
use warp::Filter;
use warp::ws::{Message, WebSocket};
use std::sync::Arc;
use database::Database;
use sqlx::SqlitePool;
use uuid::Uuid;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration};
use lobby::*;
use deck::Deck;

use serde::Deserialize;
use serde_json::Result as JsonResult;

#[derive(Deserialize)]
#[serde(tag = "action", content = "data")]
enum ClientMessage {
    Disconnect,
    Login { username: String },
    Register { username: String },
    Ready,
    Quit,
    Help,
    CreateLobby { lobby_name: String, game_type: i32 },
    JoinLobby { lobby_name: String, spectate: bool},
    ShowLobbies,
    ShowStats,
    ShowPlayers,
    ShowLobbyInfo,
    ShowHand,
    // Add additional actions as needed.
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let db_pool = initialize_db().await;
    let database = Arc::new(Database::new(db_pool.clone()));
    let server_lobby = Arc::new(Mutex::new(
        Lobby::new(lobby::NOT_SET, "Server Lobby".to_string()).await
    ));

    // WebSocket route
    let ws_route = warp::path("connect")
        .and(warp::ws())
        .and(with_db(database.clone()))
        .and(with_lobby(server_lobby.clone()))
        .map(|ws: warp::ws::Ws, db, lobby| {
            ws.on_upgrade(move |socket| handle_connection(socket, db, lobby))
        });

    let index_route = warp::path::end()
        .map(|| warp::reply::html(include_str!("../static/index.html")));

    // This should be in your main.rs where you define routes
    let login_route = warp::path("login")
        .map(|| warp::reply::html(include_str!("../static/login.html")));

    let server_lobby_route = warp::path("server_lobby")
        .map(|| warp::reply::html(include_str!("../static/server_lobby.html")));

    let lobby_route = warp::path("lobby")
        .map(|| warp::reply::html(include_str!("../static/lobby.html")));

    let stats_route = warp::path("stats")
        .map(|| warp::reply::html(include_str!("../static/stats.html")));

    let five_card = warp::path("five_card")
        .and(warp::fs::dir("../static/five_card.html"));

    let seven_card = warp::path("sevn_card")
        .and(warp::fs::dir("../static/seven_card.html"));

    let texas_hold_em = warp::path("texas_holdem")
        .and(warp::fs::dir("../static/texas_holdem.html"));

    let static_files = warp::path("static")
        .and(warp::fs::dir("../static"));

    // Combine routes
    let routes = ws_route
        .or(index_route)
        .or(login_route)
        .or(server_lobby_route)
        .or(lobby_route)
        .or(stats_route)
        .or(five_card)
        .or(seven_card)
        .or(texas_hold_em)
        .or(static_files)
        .with(warp::cors()
            .allow_any_origin()
            .allow_headers(vec!["content-type"])
            .allow_methods(vec!["GET", "POST"]));
    println!("Server starting on http://localhost:1112");
    
    warp::serve(routes)
        .run(([0, 0, 0, 0], 1112))
        .await;
    Ok(())
}

async fn initialize_db() -> SqlitePool {
    // Initialize the SQLite database connection pool.
    let db_pool = SqlitePool::connect("sqlite://poker.db")
        .await
        .expect("Failed to connect to database");
    db_pool
}

fn with_db(
    db: Arc<Database>
) -> impl Filter<Extract = (Arc<Database>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

fn with_lobby(
    lobby: Arc<Mutex<Lobby>>
) -> impl Filter<Extract = (Arc<Mutex<Lobby>>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || lobby.clone())
}

/// Retrieves the names and statuses of all lobbies from the server.
/// 
/// This function locks the `server_lobby` asynchronously, then calls 
/// `get_lobby_names_and_status` to obtain a list of lobby names and their 
/// corresponding statuses.
/// 
/// # Returns
/// 
/// A list of tuples where each tuple contains the name of a lobby and its status.
async fn get_lobbies_json(server_lobby: Arc<Mutex<Lobby>>) -> String {
    let lobbies = server_lobby.lock().await.get_lobby_names_and_status().await;
    let mut lobby_list = Vec::new();
    
    for (lobby_name, lobby_status, lobby_type, player_count, max_player_count) in lobbies {
        // Convert status code to string
        let status = if lobby_status == lobby::JOINABLE {
            "Joinable"
        } else {
            "Not Joinable"
        };
        
        // Convert game type to readable string
        let game_type = match lobby_type {
            lobby::FIVE_CARD_DRAW => "5 Card Draw",
            lobby::SEVEN_CARD_STUD => "7 Card Stud", 
            lobby::TEXAS_HOLD_EM => "Texas Hold'em",
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
    
    serde_json::json!({
        "lobbies": lobby_list
    }).to_string()
}

/// Handles a new WebSocket connection.
/// 
/// This function is called for each new WebSocket connection and is responsible for
/// processing the player's input and sending messages back to the client.
/// 
/// # Arguments
/// 
/// * `ws` - The WebSocket connection.
/// * `db` - The database connection pool.
/// * `server_lobby` - The server lobby containing all players and lobbies.
/// 
/// # Returns
/// 
/// This function does not return a value, but it sends messages to the client
/// via the WebSocket connection.
async fn handle_connection(ws: WebSocket, db: Arc<Database>, server_lobby: Arc<Mutex<Lobby>>) {
    // Split websocket into tx/rx and create a channel to forward messages.
    let (mut ws_tx, ws_rx) = ws.split();
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Forward messages from our own channel to the websocket.
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            let _ = ws_tx.send(message).await;
        }
    });
    let mut curr_player = Player {
        name: "".to_string(),
        id: "".to_string(),
        hand: Vec::new(),
        wallet: 0,
        tx: tx.clone(),
        rx: Arc::new(Mutex::new(ws_rx)),
        state: lobby::LOGGING_IN,
        current_bet: 0,
        dealer: false,
        ready: false,
        games_played: 0,
        games_won: 0,
        lobby: server_lobby.clone(),
    };

    // Send initial welcome message without redirect
    tx.send(Message::text(r#"{"message": "Welcome to Poker!"}"#)).unwrap();

    {// CHAT/LOGIN phase: expect JSON messages from the client (sent via UI buttons/textboxes).
        while let Some(result) = curr_player.rx.lock().await.next().await {
            if let Ok(msg) = result {
                if let Ok(text) = msg.to_str() {
                    // Attempt to parse the incoming JSON message.
                    let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                    match client_msg {
                        Ok(ClientMessage::Login { username }) => {
                            // Attempt login using your database logic.
                            if let Ok(Some(_id)) = db.login_player(&username).await {
                                tx.send(Message::text(
                                    format!(r#"{{"message": "Welcome back, {}!", "redirect": "server_lobby"}}"#, username)
                                )).unwrap();
                                // Create a new player instance and add to lobby as needed.
                                curr_player.name = username.clone();
                                curr_player.id = _id.to_string();
                                curr_player.wallet = db.get_player_wallet(&username).await.unwrap_or(1000) as i32;
                                curr_player.state = lobby::IN_SERVER;
                                curr_player.lobby = server_lobby.clone();

                                server_lobby.lock().await.add_player(curr_player.clone()).await;
                                server_lobby.lock().await.broadcast_player_count().await;
                                break;  // Exit login loop.
                            } else {
                                tx.send(Message::text(r#"{"message": "Login failed. Please try again."}"#)).unwrap();
                            }
                        }
                        Ok(ClientMessage::Register { username }) => {
                            // Attempt registration using your DB logic.
                            if db.register_player(&username).await.is_ok() {
                                tx.send(Message::text(
                                    format!(r#"{{"message": "Registration successful! Welcome, {}!", "redirect": "server_lobby"}}"#, username)
                                )).unwrap();
                                // Create a new player instance and add to lobby as needed.
                                curr_player.name = username.clone();
                                curr_player.id = Uuid::new_v4().to_string();
                                curr_player.wallet = 1000;
                                curr_player.state = lobby::IN_SERVER;
                                curr_player.lobby = server_lobby.clone();
                                
                                server_lobby.lock().await.add_player(curr_player.clone()).await;
                                server_lobby.lock().await.broadcast_player_count().await;
                                break; // Exit login loop.
                            } else {
                                tx.send(Message::text(r#"{"message": "Registration failed. Try again."}"#)).unwrap();
                            }
                        }
                        Ok(ClientMessage::Quit) => {
                            tx.send(Message::text(r#"{"message": "Goodbye!", "redirect": "index"}"#)).unwrap();
                            server_lobby.lock().await.remove_player("".to_string()).await;
                            server_lobby.lock().await.broadcast_player_count().await;
                            return;
                        }
                        _ => {
                            // For unsupported actions before login/registration:
                            tx.send(Message::text(r#"{"message": "Unsupported action during login phase."}"#)).unwrap();
                        }
                    }
                }
            }
        }
    }
    
    // After login/registration, handle server lobby phase
    if curr_player.state == lobby::IN_SERVER {
        println!("Player logged in successfully.");
        handle_server_lobby(curr_player, server_lobby, db).await;
    }

    println!("exited");
    
    // Continue processing further lobby/game actions using similar JSON messages.
    // For example, receiving ClientMessage::JoinLobby or ClientMessage::Ready will update game state,
    // call join_lobby, or trigger the game state machine (which broadcasts messages to all players).
    // Each action response can include a "redirect" field so that the UI switches pages (e.g., to in_game.html or stats.html).
    
    // (Your detailed lobby and game logic handling goes here.)
}

/// Handles player interaction while in the server lobby.
/// 
/// This function processes messages received from the client when they are in the server lobby,
/// such as creating or joining game lobbies, viewing available lobbies, etc.
/// 
/// # Arguments
/// 
/// * `player` - The current player.
/// * `server_lobby` - The server lobby containing all players and lobbies.
/// * `db` - The database connection pool.
async fn handle_server_lobby(mut player: Player, server_lobby: Arc<Mutex<Lobby>>, db: Arc<Database>) {
    let tx = player.tx.clone();
    
    // At this point, the client is successfully logged in and has been redirected to the lobby.
    // You can now send a JSON message with lobby options that your UI (e.g., lobby.html) processes.
    loop {
        let result = {
            let mut rx = player.rx.lock().await;
            match rx.next().await {
                Some(res) => res,
                None => continue,
            }
        };
        println!("reached here");
        
        println!("Received message: {:?}", result);
        if let Ok(msg) = result {
            if let Ok(text) = msg.to_str() {
                // Attempt to parse the incoming JSON message.
                let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                println!("Received message: {}", text);
                match client_msg {
                    Ok(ClientMessage::Disconnect) => {
                        server_lobby.lock().await.remove_player(player.name.clone()).await;
                        server_lobby.lock().await.broadcast_player_count().await;
                        break;
                    }
                    Ok(ClientMessage::ShowPlayers) => {
                        // Show players in the lobby.
                        let player_count = server_lobby.lock().await.get_player_count().await;
                        let msg = serde_json::json!({
                            "playerCount": player_count
                        });
                        tx.send(Message::text(msg.to_string())).unwrap();
                    }
                    Ok(ClientMessage::ShowLobbies) => {
                        // Generate JSON with lobby information and send it to the client
                        let lobbies_json = get_lobbies_json(server_lobby.clone()).await;
                        tx.send(Message::text(lobbies_json)).unwrap();
                    }
                    Ok(ClientMessage::CreateLobby { lobby_name, game_type }) => {
                        // Check if lobby name already exists
                        if server_lobby.lock().await.lobby_exists(lobby_name.clone()).await {
                            tx.send(Message::text(r#"{"error": "Lobby name already exists"}"#)).unwrap();
                        } else {
                            // Create a new lobby with the specified name and game type
                            let new_lobby = Arc::new(Mutex::new(Lobby::new(game_type, lobby_name.clone()).await));
                            
                            // Add the new lobby to the server
                            server_lobby.lock().await.add_lobby(new_lobby).await;
                            
                            // Send success message
                            tx.send(Message::text(format!(r#"{{"message": "Lobby '{}' created successfully"}}"#, lobby_name))).unwrap();
                        }
                    }
                    Ok(ClientMessage::JoinLobby { lobby_name, spectate }) => {
                        let join_result = player.player_join_lobby(server_lobby.clone(), lobby_name.clone(), spectate).await;
                        server_lobby.lock().await.update_lobby_names_status(lobby_name.clone()).await;
                        if join_result == lobby::SUCCESS {
                            // Successfully joined the lobby
                            tx.send(Message::text(
                                format!(r#"{{"message": "Successfully joined lobby: {}!", "redirect": "lobby"}}"#, lobby_name.clone())
                            )).unwrap();
                            let result = join_lobby(server_lobby.clone(), player.clone(), db.clone(), spectate.clone()).await;
                            if result == "Disconnect" {
                                break;
                            }
                        } else {
                            // Failed to join lobby
                            let message = if spectate {
                                "Failed to join lobby as spectator."
                            } else {
                                "Failed to join lobby. The lobby may be full or not joinable."
                            };
                            tx.send(Message::text(format!(r#"{{"message": "{}"}}"#, message))).unwrap();
                        }
                    }
                    Ok(ClientMessage::ShowStats) => {
                        // Fetch and display player stats
                        let stats = db.player_stats(&player.name).await;
                        if let Ok(stats) = stats {
                            tx.send(Message::text(serde_json::json!({
                                "stats": {
                                    "username": player.name,
                                    "gamesPlayed": stats.games_played,
                                    "gamesWon": stats.games_won,
                                    "wallet": stats.wallet
                                }
                            }).to_string())).unwrap();
                        } else {
                            tx.send(Message::text(r#"{"error": "Failed to retrieve stats"}"#)).unwrap();
                        }
                    }
                    _ => {
                        // For unsupported actions after login/registration:
                        tx.send(Message::text(r#"{"message": "Unsupported action after login."}"#)).unwrap();
                    }
                }
            }
        }
    }
}

/// Handles a player joining a lobby.
/// 
/// This function is called when a player joins a lobby and is responsible for processing
/// the player's input and sending messages back to the client.
/// 
/// # Arguments
/// 
/// * `server_lobby` - The server lobby containing all players and lobbies.
/// * `player` - The player joining the lobby.
/// * `db` - The database connection pool.
/// 
/// # Returns
/// 
/// This function returns a `String` indicating the exit status of the player.
async fn join_lobby(server_lobby: Arc<Mutex<Lobby>>, mut player: Player, db: Arc<Database>, spectator: bool) -> String {
    player.state = lobby::IN_LOBBY;
    let player_lobby = player.lobby.clone();
    let tx = player.tx.clone();
    println!("{} has joined lobby: {}", player.name, player_lobby.lock().await.name);
    
    // Send initial lobby information
    send_lobby_info(&player_lobby, &tx).await;
    send_player_list(&player_lobby, &tx).await;
    
    loop {
        let result = {
            let mut rx = player.rx.lock().await;
            match rx.next().await {
                Some(res) => res,
                None => continue,
            }
        };
        
        if let Ok(msg) = result {
            if let Ok(text) = msg.to_str() {
                // Attempt to parse the incoming JSON message
                let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                println!("Received message in lobby: {}", text);
                
                let lobby_state = player_lobby.lock().await.game_state.clone();
                let lobby_name = player_lobby.lock().await.name.clone();
                println!("Lobby {} state: {}", lobby_name, lobby_state);
                
                if lobby_state == lobby::JOINABLE || lobby_state == lobby::GAME_LOBBY_FULL {
                    match client_msg {
                        Ok(ClientMessage::Disconnect) => {
                            server_lobby.lock().await.remove_player(player.name.clone()).await;
                            server_lobby.lock().await.broadcast_player_count().await;
                            return "disconnect".to_string();
                        }
                        Ok(ClientMessage::Quit) => {
                            // QUIT LOBBY - Return to server lobby
                            let lobby_status = player_lobby.lock().await.remove_player(player.name.clone()).await;
                            if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                server_lobby.lock().await.remove_lobby(lobby_name.clone()).await;
                            }
                            server_lobby.lock().await.broadcast_player_count().await;
                            
                            // Send redirect back to server lobby
                            tx.send(Message::text(r#"{"message": "Leaving lobby...", "redirect": "server_lobby"}"#)).unwrap();
                            return "Normal".to_string();
                        }
                        Ok(ClientMessage::Disconnect) => {
                            // Player disconnected entirely
                            let lobby_status = player_lobby.lock().await.remove_player(player.name.clone()).await;
                            if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                server_lobby.lock().await.remove_lobby(lobby_name.clone()).await;
                            }
                            server_lobby.lock().await.remove_player(player.name.clone()).await;
                            server_lobby.lock().await.broadcast_player_count().await;
                            
                            // Update player stats
                            if let Err(e) = db.update_player_stats(&player).await {
                                eprintln!("Failed to update player stats: {}", e);
                            }
                            
                            return "Disconnect".to_string();
                        }
                        Ok(ClientMessage::ShowLobbyInfo) => {
                            // Send lobby information to client
                            send_lobby_info(&player_lobby, &tx).await;
                            // Update player list
                            send_player_list(&player_lobby, &tx).await;
                        }
                        Ok(ClientMessage::Ready) => {
                            // READY UP
                            let mut all_ready = 0;
                            player.ready = !player.ready; // Toggle ready status
                            
                            let (ready_player_count, lobby_player_count) = 
                                player_lobby.lock().await.ready_up(player.name.clone()).await;
                            
                            // Update all clients with the new player list
                            // Create player list message and broadcast it to all players
                            let player_info = player_lobby.lock().await.get_player_names_and_status().await;
                            let mut players = Vec::new();
                            for (name, ready) in player_info {
                                players.push(serde_json::json!({
                                    "name": name,
                                    "ready": ready
                                }));
                            }
                            
                            let player_list = serde_json::json!({
                                "players": players
                            });
                            
                            player_lobby.lock().await.broadcast_json(player_list.to_string()).await;
                            
                            if ready_player_count == lobby_player_count && lobby_player_count >= 2 {
                                all_ready = 1;
                                
                                // Send notification that all players are ready
                                let msg = serde_json::json!({
                                    "message": "All players ready. Starting game..."
                                });
                                player_lobby.lock().await.broadcast_json(msg.to_string()).await;
                                
                                sleep(Duration::from_secs(2)).await;
                                
                                // Start the game
                                player_lobby.lock().await.game_state = lobby::START_OF_ROUND;


                                /*  Start the game  */



                                /*                  */

                            } else if lobby_player_count < 2 {
                                // Not enough players
                                let msg = serde_json::json!({
                                    "message": "Need at least 2 players to start the game"
                                });
                                tx.send(Message::text(msg.to_string())).unwrap();
                            } else {
                                // Not all players are ready
                                let msg = serde_json::json!({
                                    "message": format!("Players ready: {}/{}", ready_player_count, lobby_player_count)
                                });
                                tx.send(Message::text(msg.to_string())).unwrap();
                            }
                        }
                        Ok(ClientMessage::ShowStats) => {
                            // Get and send player stats
                            let stats = db.player_stats(&player.name).await;
                            if let Ok(stats) = stats {
                                let stats_json = serde_json::json!({
                                    "stats": {
                                        "username": player.name,
                                        "gamesPlayed": stats.games_played,
                                        "gamesWon": stats.games_won,
                                        "wallet": stats.wallet
                                    }
                                });
                                tx.send(Message::text(stats_json.to_string())).unwrap();
                            } else {
                                tx.send(Message::text(r#"{"error": "Failed to retrieve stats"}"#)).unwrap();
                            }
                        }
                        _ => {
                            // Unsupported action in lobby
                            tx.send(Message::text(r#"{"message": "Unsupported action in lobby"}"#)).unwrap();
                        }
                    }
                } else {
                    // Game is in progress
                    println!("Lobby in progress.");
                    tx.send(Message::text(r#"{"message": "Game is in progress, certain actions are restricted"}"#)).unwrap();
                }
            }
        }
    }
}

/// Sends the current lobby information to the client.
async fn send_lobby_info(player_lobby: &Arc<Mutex<Lobby>>, tx: &mpsc::UnboundedSender<Message>) {
    let lobby = player_lobby.lock().await;
    
    // Get lobby information
    let game_type = match lobby.game_type {
        lobby::FIVE_CARD_DRAW => "5 Card Draw",
        lobby::SEVEN_CARD_STUD => "7 Card Stud",
        lobby::TEXAS_HOLD_EM => "Texas Hold'em",
        _ => "Unknown"
    };
    
    let player_count = lobby.get_player_count().await;
    let max_players = match lobby.game_type {
        lobby::FIVE_CARD_DRAW => 5,
        lobby::SEVEN_CARD_STUD => 7,
        lobby::TEXAS_HOLD_EM => 10,
        _ => 10
    };
    
    // Create JSON response
    let lobby_info = serde_json::json!({
        "lobbyInfo": {
            "name": lobby.name,
            "gameType": game_type,
            "playerCount": player_count,
            "maxPlayers": max_players
        }
    });
    
    let players = lobby.players.lock().await;
    let player_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    for tx in player_tx {
        // Send lobby information to all players in the lobby
        tx.send(Message::text(lobby_info.to_string())).unwrap();
    }
}

/// Sends the current player list to the client.
async fn send_player_list(player_lobby: &Arc<Mutex<Lobby>>, tx: &mpsc::UnboundedSender<Message>) {
    let lobby = player_lobby.lock().await;
    
    // Build player list
    let player_info = lobby.get_player_names_and_status().await;
    let mut players = Vec::new();
    for (name, ready) in player_info {
        players.push(serde_json::json!({
            "name": name,
            "ready": ready
        }));
    }
    
    // Create JSON response
    let player_list = serde_json::json!({
        "players": players
    });

    let players = lobby.players.lock().await;
    let player_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    for tx in player_tx {
        // Send player list to all players in the lobby
        tx.send(Message::text(player_list.to_string())).unwrap();
    }
}

