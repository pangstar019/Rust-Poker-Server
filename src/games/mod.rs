//! # Poker Game Logic Module
//!
//! This module implements the core game logic for various poker games, including Five Card Draw, Seven Card Stud, and Texas Hold'em. 
//! It is designed to handle all aspects of gameplay, such as dealing cards, managing betting rounds, determining winners, and updating game states.
//!
//! ## Features
//! - **Game State Management**: Implements state machines for each poker variant to control the flow of the game.
//! - **Card Dealing**: Handles the distribution of cards to players, including community cards for games like Texas Hold'em.
//! - **Betting Rounds**: Manages betting rounds, including actions like checking, raising, calling, folding, and going all-in.
//! - **Hand Evaluation**: Determines the best hand for each player and ranks them to decide the winner.
//! - **Player Actions**: Supports player actions such as exchanging cards during a drawing round or paying blinds and antes.
//! - **Concurrency**: Uses asynchronous programming with `async/await` to handle multiple players and game events concurrently.
//! - **WebSocket Integration**: Designed to work with a WebSocket server for real-time communication with players.
//!
//! ## Constants
//! This module defines a variety of constants to represent game states, player states, and return codes. These constants are used throughout the module to ensure consistency and readability.
//!
//! ## Supported Poker Variants
//! - **Five Card Draw**: A classic poker game where players are dealt five cards and can exchange cards during a drawing round.
//! - **Seven Card Stud**: A poker game where players are dealt seven cards, with a mix of face-up and face-down cards, and must form the best five-card hand.
//! - **Texas Hold'em**: A popular poker variant where players are dealt two private cards and share five community cards.
//!
//! ## Game Flow
//! Each poker variant follows a specific sequence of game states, such as:
//! 1. Start of Round
//! 2. Ante or Blinds
//! 3. Card Dealing
//! 4. Betting Rounds
//! 5. Showdown
//! 6. End of Round and Database Update
//!
//! ## Testing
//! The module includes unit tests to verify the correctness of hand evaluation logic and other critical functions. These tests ensure that the game logic adheres to poker rules and handles edge cases correctly.
//!
//! ## Dependencies
//! - **Tokio**: For asynchronous programming and synchronization primitives.
//! - **Warp**: For WebSocket communication.
//! - **SQLx**: For database interactions.
//! - **Futures**: For handling asynchronous tasks.
//!
//! ## Usage
//! This module is intended to be used as part of a larger poker server application. It interacts with other modules, such as the lobby and deck modules, to provide a complete poker experience.
//!
//! ## Notes
//! - The module assumes that player actions are received via WebSocket messages and processed asynchronously.
//! - The game logic is designed to be extensible, allowing for the addition of new poker variants or custom rules.
//!
//! This module handles the game logic for different poker games.
//! It includes functions for dealing cards, managing betting rounds, and determining the winner.
//! It also includes functions for handling player actions and updating the game state.
//! The module is designed to be used with a WebSocket server and uses async/await for concurrency.

use super::*;
use crate::lobby::{self, Lobby};
use crate::player::{self, Player};
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, Mutex};
use warp:: ws::Message;

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

/// Deals cards to players in a 7 Card Stud game.
/// The first two cards are face-down, the third card is face-up.
/// The last card is face-down.
/// The function also handles the display of hands to active players.
/// 
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// * `round` - The current round of the game (0 for the first round, 1 for the last card).
/// 
/// # Returns
/// This function does not return a value. It updates the players' hands and displays them to active players.
async fn deal_cards_7(lobby: &mut Lobby, round: usize) {
    let mut players = lobby.players.lock().await;
    let mut count;
    for player in players.iter_mut() {
        count = 0;
        loop {
            if player.state != player::FOLDED {
                let card = lobby.deck.deal();
                let card_value = if round == 1 {
                    // First two cards face-down, third card face-up
                    if player.hand.len() < 2 {
                        card + 53 // First two cards are face-down
                    } else {
                        card // Third card is face-up
                    }
                } else if round == 5 {
                    card + 53 // Last card is face-down
                } else {
                    card // All other rounds are face-up
                };

                player.hand.push(card_value);
                if round == 1 {
                    count += 1;
                    if count == 3 {
                        break;
                    }
                }
                else {break}
            }
        }
    }
    // Get active players
    let active_players: Vec<_> = players.iter().filter(|p| p.state != player::FOLDED).collect();
    let players_tx: Vec<_> = active_players.iter().map(|p| p.tx.clone()).collect();
    let players_hands: Vec<_> = active_players.iter().map(|p| p.hand.clone()).collect();
    display_hand(players_tx, players_hands).await;
}

/// Deals cards to players in a Texas Hold'em game.
/// The function handles the dealing of community cards and player hands based on the round.
/// 
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// * `round` - The current round of the game (1 for pre-flop, 2 for flop, etc.).
/// 
/// # Returns
/// This function does not return a value. It updates the players' hands and community cards, and displays them to active players.
/// It also handles the display of hands to active players.
pub async fn deal_cards_texas(lobby: &mut Lobby, round: usize) {
    let mut community_cards = lobby.community_cards.lock().await;
    let mut players = lobby.players.lock().await;
    let players_tx = players.iter().filter(|p| p.state != player::FOLDED).map(|p| p.tx.clone()).collect::<Vec<_>>();
    match round {
        1 => {
            for player in players.iter_mut() {
                if player.state != player::FOLDED {
                    // deal 2 cards to each player
                    player.hand.push(lobby.deck.deal());
                    player.hand.push(lobby.deck.deal());
                } 
            }
            let players_hands = players.iter().filter(|p| p.state != player::FOLDED).map(|p| p.hand.clone()).collect::<Vec<_>>(); // get all hands
            display_hand(players_tx.clone(), players_hands.clone()).await;
            return;
        }
        2 => {
            // for flop round, deals 3 community
            for _ in 0..3 {
                community_cards.push(lobby.deck.deal());
            }
        }
        _ => {
            // any other round the same
            community_cards.push(lobby.deck.deal());
        }
    }
    let players_tx = players.iter().filter(|p| p.state != player::FOLDED).map(|p| p.tx.clone()).collect::<Vec<_>>();
    let mut message = String::from("Community cards:\n");
    for (i, card) in community_cards.iter().enumerate() {
        message.push_str(&format!("{}. {}\n", i + 1, translate_card(*card).await));
    }
    lobby.lobby_wide_send(players_tx.clone(), message).await;
}

/// Handles the betting round for players in a poker game.
/// The function manages player actions such as checking, raising, calling, folding, and going all-in.
/// It also updates the game state and player statistics.
/// 
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the players' wallets and game statistics.
/// It also handles the display of hands to active players.
pub async fn betting_round(player: &mut Player, lobby: &mut tokio::sync::MutexGuard<'_, lobby::Lobby,>, action: ClientMessage) -> (bool, bool) {
    let mut valid_action = true;
    let mut reset = false;
    let player_prev_bet = player.current_bet;
    println!("{}: player_prev_bet: {}", player.name, player_prev_bet);
    let current_max_bet = lobby.current_max_bet;

    match action {
        ClientMessage::Check => {
            println!("{}: check command received", player.name);
            println!("DEBUG VALUES - player.current_bet={}, lobby.current_max_bet={}", player.current_bet, lobby.current_max_bet);
            // only check when there is no bet to call
            if lobby.current_max_bet == 0 {
                player.tx.send(Message::text(r#"{"message": "Checked"}"#)).unwrap();
                player.state = player::CHECKED;
                return (valid_action, reset);
            } else {
                valid_action = false;
                return (valid_action, reset);
            }
        }
        ClientMessage::Fold => {
            println!("{}: fold command received", player.name);
            player.state = player::FOLDED;
            return (valid_action, reset);
        }
        ClientMessage::Call => {
            println!("{}: call command received", player.name);
            let call_amount = current_max_bet - player_prev_bet;
            if player.wallet >= call_amount {
                player.wallet -= call_amount;
                lobby.pot += call_amount;
                player.current_bet = current_max_bet;
                if player.wallet == 0 {
                    player.state = player::ALL_IN;
                } else {
                    player.state = player::CALLED;
                }
                return (valid_action, reset);
            } else {
                valid_action = false;
                return (valid_action,reset);
            }
        }
        ClientMessage::Raise { amount } => {
            println!("{}: raise command received", player.name);
            if amount > 0 && amount <= player.wallet {
                if amount > current_max_bet - player_prev_bet {
                    player.state = player::RAISED;
                    player.wallet -= amount;
                    lobby.pot += amount;
                    player.current_bet += amount;
                    lobby.current_max_bet = player.current_bet;
                    reset = true;
                    if player.wallet == 0 {
                        player.state = player::ALL_IN;
                    }
                    return (valid_action, reset);
                }
            }
            valid_action = false;
            return (valid_action, reset);
        }
        ClientMessage::AllIn => {
            println!("{}: all in command received", player.name);
            player.state = player::ALL_IN;
            let all_in_amount = player.wallet;
            player.wallet = 0;
            player.current_bet += all_in_amount;
            lobby.pot += all_in_amount;
            if player.current_bet > current_max_bet{
                lobby.current_max_bet = player.current_bet;
                reset = true;
            }
            return (valid_action, reset);
        }
        _ => {
            valid_action = false;
            return (valid_action, reset);
        }
    }
    
}

/// Handles the drawing round for players in a poker game.
/// The function allows players to choose between standing pat (keeping their hand) or exchanging cards.
/// 
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the players' hands.
/// It also handles the display of hands to active players.
// pub async fn drawing_round(lobby: &mut Lobby) {
//     let player_names = {
//         let players = lobby.players.lock().await;
//         players.iter()
//               .filter(|p| p.state != FOLDED)
//               .map(|p| p.name.clone())
//               .collect::<Vec<String>>()
//     };
    
//     if player_names.len() <= 1 {
//         // Only one player left, move on
//         return;
//     }
    
//     for player_name in player_names {
//         // Send options to the player
//         let player_tx = {
//             let players = lobby.players.lock().await;
//             players.iter()
//                   .find(|p| p.name == player_name)
//                   .map(|p| p.tx.clone())
//         };
        
//         if let Some(tx) = player_tx {
//             let message = "Drawing round!\nChoose an option:\n    1 - Stand Pat (Keep your hand)\n    2 - Exchange cards";
//             let _ = tx.send(Message::text(message));
//         }
        
//         // Process player's choice
//         let choice = lobby.process_player_input(&player_name).await;
        
//         match choice.as_str() {
//             "1" => {
//                 // Player chooses to stand pat
//                 if let Some(tx) = {
//                     let players = lobby.players.lock().await;
//                     players.iter()
//                           .find(|p| p.name == player_name)
//                           .map(|p| p.tx.clone())
//                 } {
//                     let _ = tx.send(Message::text("You chose to Stand Pat."));
//                 }
                
//                 // Broadcast to other players
//                 lobby.broadcast(format!("{} chose to Stand Pat.", player_name)).await;
//             }
//             "2" => {
//                 // Player chooses to exchange cards
//                 if let Some(tx) = {
//                     let players = lobby.players.lock().await;
//                     players.iter()
//                           .find(|p| p.name == player_name)
//                           .map(|p| p.tx.clone())
//                 } {
//                     let _ = tx.send(Message::text("Enter the indices of the cards you want to exchange (comma-separated, e.g., '1,2,3')"));
//                 }
                
//                 let indices_input = lobby.process_player_input(&player_name).await;
                
//                 // Parse indices
//                 let indices: Vec<usize> = indices_input
//                     .split(',')
//                     .filter_map(|s| s.trim().parse::<usize>().ok())
//                     .filter(|&i| i > 0) // 1-based indexing
//                     .map(|i| i - 1) // Convert to 0-based
//                     .collect();
                
//                 if !indices.is_empty() {
//                     // Get current hand and create a new hand without exchanged cards
//                     let current_hand = {
//                         let players = lobby.players.lock().await;
//                         players.iter()
//                               .find(|p| p.name == player_name)
//                               .map(|p| p.hand.clone())
//                               .unwrap_or_default()
//                     };
                    
//                     let mut new_hand = Vec::new();
//                     for (i, &card) in current_hand.iter().enumerate() {
//                         if !indices.contains(&i) {
//                             new_hand.push(card);
//                         }
//                     }
                    
//                     // Deal new cards to replace exchanged ones
//                     for _ in 0..indices.len() {
//                         new_hand.push(lobby.deck.deal());
//                     }
                    
//                     // Update player's hand
//                     lobby.update_player_hand(&player_name, new_hand).await;
                    
//                     // Display new hand to player
//                     let tx = {
//                         let players = lobby.players.lock().await;
//                         players.iter()
//                               .find(|p| p.name == player_name)
//                               .map(|p| p.tx.clone())
//                     };
                    
//                     let hand = {
//                         let players = lobby.players.lock().await;
//                         players.iter()
//                               .find(|p| p.name == player_name)
//                               .map(|p| p.hand.clone())
//                               .unwrap_or_default()
//                     };
                    
//                     if let Some(tx) = tx {
//                         display_hand(vec![tx], vec![hand]).await;
//                     }
                    
//                     // Broadcast to other players
//                     lobby.broadcast(format!("{} has exchanged {} cards.", player_name, indices.len())).await;
//                 }
//             }
//             "Disconnect" => {
//                 // Player disconnected
//                 lobby.update_player_state(&player_name, FOLDED).await;
//                 lobby.broadcast(format!("{} has disconnected and folded.", player_name)).await;
//             }
//             _ => {
//                 // Invalid choice, default to standing pat
//                 if let Some(tx) = {
//                     let players = lobby.players.lock().await;
//                     players.iter()
//                           .find(|p| p.name == player_name)
//                           .map(|p| p.tx.clone())
//                 } {
//                     let _ = tx.send(Message::text("Invalid choice. Standing pat by default."));
//                 }
//             }
//         }
//     }
// }


/// Translates a card number into a human-readable string representation.
/// The function handles the card's rank and suit, and returns a string like "Ace of Hearts" or "10 of Diamonds".
/// 
/// # Arguments
/// * `card` - An integer representing the card number (0-51 for standard cards, 53+ for face-down cards).
/// 
/// # Returns
/// 
/// This function returns a `String` representing the card's rank and suit.
pub async fn translate_card(card: i32) -> String {
    //if card is greater than 52 during the very final round of 7 card stud which is the showdown round
    //We will do card -53 to get the actual card value else if it is not that round yet we will just display X
        // let mut card_clone = card.clone();
        
        if card > 52 {
            return "X".to_string(); // Face-down card
        }


    let mut card_str: String = Default::default();
    let rank: i32 = card % 13;

    if rank == 0 {
        card_str.push_str("Ace");
    } else if rank <= 9 {
        card_str.push_str(&(rank + 1).to_string());
    } else if rank == 10 {
        card_str.push_str("Jack");
    } else if rank == 11 {
        card_str.push_str("Queen");
    } else if rank == 12 {
        card_str.push_str("King");
    }

    let suit: i32 = card / 13;
    if suit == 0 {
        card_str.push_str(" Hearts");
    } else if suit == 1 {
        card_str.push_str(" Diamond");
    } else if suit == 2 {
        card_str.push_str(" Spade");
    } else if suit == 3 {
        card_str.push_str(" Club");
    }
    return card_str;
}

/// Displays the players' hands to all active players in the game.
/// The function formats the hands into a readable string and sends it to each player's channel.
/// 
/// # Arguments
/// * `players_tx` - A vector of `UnboundedSender<Message>` representing the channels for each player.
/// * `players_hands` - A vector of vectors containing the players' hands (card numbers).
/// 
/// # Returns
/// 
/// This function does not return a value. It sends messages to the players' channels.
pub async fn display_hand(players_tx: Vec<UnboundedSender<Message>>, players_hands: Vec<Vec<i32>>) {
    // let players = self.players;
    let mut message: String;
    let mut index = 0;
    let mut count = 1;
    for tx in players_tx.iter().cloned() {
        let mut translated_cards: String = Default::default();
        for card in players_hands[index].iter().cloned() {
            // create a string like "count. "
            translated_cards.push_str(&format!("{}. ", count));
            translated_cards.push_str(translate_card(card.clone()).await.as_str());
            translated_cards.push_str("\n");
            count += 1;
        }
        count = 1;
        message = format!("Your hand:\n{}", translated_cards.trim_end_matches(", "));
        let _ = tx.send(Message::text(message.clone()));
        index += 1;
    }
}

// for 7 card stud, we will need to determine the best hand out of the 7 cards
/// This function takes a hand of 7 cards and returns the best hand possible.
/// It evaluates all combinations of 5 cards from the 7 and determines the best hand type.
/// 
/// # Arguments
/// * `hand` - A slice of integers representing the 7 cards in the hand.
/// 
/// # Returns
/// 
/// This function returns a tuple containing the best hand type and the ranks of the cards in the best hand.
/// The tuple format is (hand_type, rank1, rank2, rank3, rank4, rank5).
/// 
/// # Panics
/// 
/// This function will panic if the length of the hand is not 7.
pub fn get_best_hand(hand: &[i32]) -> (i32, i32, i32, i32, i32, i32) {
    // Replace the assertion with a check that returns a default hand type
    if hand.len() != 7 {
        println!("Warning: Hand length is {} instead of 7, returning default hand", hand.len());
        return (0, 0, 0, 0, 0, 0); // Return a default hand type (high card)
    }
    
    println!("Hand: {:?}", hand);
    let mut best_hand = (-1, -1, -1, -1, -1, -1);
    for i in 0..=2 {
        for j in (i + 1)..=3 {
            for k in (j + 1)..=4 {
                for l in (k + 1)..=5 {
                    for m in (l + 1)..=6 {
                        let current_hand = vec![hand[i], hand[j], hand[k], hand[l], hand[m]];
                        let current_hand_type = get_hand_type(&current_hand);
                        if current_hand_type > best_hand
                        {
                            best_hand = current_hand_type;
                        }
                    }
                }
            }
        }
    }
    println!("Best hand: {:?}", best_hand);
    best_hand
}

/// This function takes a hand of 5 cards and returns the hand type and ranks.
/// It evaluates the hand for various poker hands such as flush, straight, four of a kind, etc.
/// 
/// # Arguments
/// * `hand` - A slice of integers representing the 5 cards in the hand.
/// 
/// # Returns
/// 
/// This function returns a tuple containing the hand type and the ranks of the cards in the hand.
/// The tuple format is (hand_type, rank1, rank2, rank3, rank4, rank5).
pub fn get_hand_type(hand: &[i32]) -> (i32, i32, i32, i32, i32, i32) {
    // Remove the assertion to handle hands with fewer than 5 cards
    let hand_size = hand.len();
    
    // Convert cards to ranks (1-13) and sort
    let mut ranks: Vec<i32> = hand
        .iter()
        .map(|&card| if card % 13 != 0 { card % 13 } else { 13 })
        .collect();
    ranks.sort(); // Sort in ascending order
    
    let suits: Vec<i32> = hand.iter().map(|&card| card / 13).collect();
    
    // Only check for flush and straight if we have 5 cards
    if hand_size == 5 {
        // Check for flush
        let flush = suits.iter().all(|&suit| suit == suits[0]);
        
        // Check for straight
        let straight = ranks.windows(2).all(|w| w[1] == w[0] + 1);
        
        if flush && straight {
            return (8, ranks[4], ranks[4], 0, 0, 0);
        }
        
        if flush {
            return (5, ranks[4], ranks[3], ranks[2], ranks[1], ranks[0]);
        }
        
        if straight {
            return (4, ranks[4], 0, 0, 0, 0);
        }
    }
    
    // Count occurrences of each rank
    let mut rank_counts = std::collections::HashMap::new();
    for &rank in &ranks {
        *rank_counts.entry(rank).or_insert(0) += 1;
    }
    
    // Sort ranks by count (descending), then by rank value (descending)
    let mut rank_count_pairs: Vec<(i32, i32)> = rank_counts.into_iter().collect();
    rank_count_pairs.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    
    // Four of a kind (needs at least 4 cards)
    if hand_size >= 4 && !rank_count_pairs.is_empty() && rank_count_pairs[0].1 == 4 {
        let kicker = if hand_size > 4 && rank_count_pairs.len() > 1 { 
            rank_count_pairs[1].0 
        } else { 
            0 
        };
        return (7, rank_count_pairs[0].0, kicker, 0, 0, 0);
    }
    
    // Full house (needs exactly 5 cards)
    if hand_size == 5 && rank_count_pairs.len() >= 2 && rank_count_pairs[0].1 == 3 && rank_count_pairs[1].1 == 2 {
        return (6, rank_count_pairs[0].0, rank_count_pairs[1].0, 0, 0, 0);
    }
    
    // Three of a kind (needs at least 3 cards)
    if hand_size >= 3 && !rank_count_pairs.is_empty() && rank_count_pairs[0].1 == 3 {
        let mut kickers = Vec::new();
        for &(rank, count) in &rank_count_pairs[1..] {
            if count == 1 {
                kickers.push(rank);
            }
        }
        kickers.sort_by(|a, b| b.cmp(a));
        
        let k1 = kickers.get(0).copied().unwrap_or(0);
        let k2 = kickers.get(1).copied().unwrap_or(0);
        
        return (3, rank_count_pairs[0].0, k1, k2, 0, 0);
    }
    
    // Two pair (needs at least 4 cards)
    if hand_size >= 4 && rank_count_pairs.len() >= 2 && rank_count_pairs[0].1 == 2 && rank_count_pairs[1].1 == 2 {
        let kicker = if hand_size >= 5 && rank_count_pairs.len() >= 3 { 
            rank_count_pairs[2].0 
        } else { 
            0 
        };
        return (2, rank_count_pairs[0].0, rank_count_pairs[1].0, kicker, 0, 0);
    }
    
    // One pair (needs at least 2 cards)
    if hand_size >= 2 && !rank_count_pairs.is_empty() && rank_count_pairs[0].1 == 2 {
        let mut kickers = Vec::new();
        for &(rank, count) in &rank_count_pairs[1..] {
            if count == 1 {
                kickers.push(rank);
            }
        }
        kickers.sort_by(|a, b| b.cmp(a));
        
        let k1 = kickers.get(0).copied().unwrap_or(0);
        let k2 = kickers.get(1).copied().unwrap_or(0);
        let k3 = kickers.get(2).copied().unwrap_or(0);
        
        return (1, rank_count_pairs[0].0, k1, k2, k3, 0);
    }
    
    // High card
    let mut high_cards = Vec::new();
    let mut ranks_desc = ranks.clone();
    ranks_desc.sort_by(|a, b| b.cmp(a));
    
    for i in 0..hand_size.min(5) {
        high_cards.push(ranks_desc[i]);
    }
    
    while high_cards.len() < 5 {
        high_cards.push(0);
    }
    
    (0, 
     *high_cards.get(0).unwrap_or(&0), 
     *high_cards.get(1).unwrap_or(&0), 
     *high_cards.get(2).unwrap_or(&0), 
     *high_cards.get(3).unwrap_or(&0), 
     *high_cards.get(4).unwrap_or(&0))
}

async fn sync_player_state(player: &mut Player, lobby: &mut tokio::sync::MutexGuard<'_, lobby::Lobby>) {
    let players_lock = lobby.players.lock().await;
    if let Some(updated_player) = players_lock.iter().find(|p| p.name == player.name) {
        player.current_bet = updated_player.current_bet;
        player.wallet = updated_player.wallet;
        player.state = updated_player.state;
        player.hand = updated_player.hand.clone();
        player.games_played = updated_player.games_played;
        player.games_won = updated_player.games_won;
    }
}

// gets players best hand of the 7 cards
/// this is used for 7 card stud and texas holdem
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the players' hands with their best hand.
/// It also handles the display of hands to active players.
pub async fn update_players_hand(lobby: &Lobby) {
    let mut players = lobby.players.lock().await;
    for player in players.iter_mut() {
        if player.state == player::FOLDED {
            continue;
        }
        
        // Save the original hole cards
        let original_hole_cards = if player.hand.len() >= 2 {
            vec![player.hand[0], player.hand[1]]
        } else {
            Vec::new()
        };
        
        // Create 7-card hand for evaluation
        let player_hand = if lobby.game_type == TEXAS_HOLD_EM {
            let community_cards = lobby.community_cards.lock().await.clone();
            [original_hole_cards.clone(), community_cards].concat() // make 7 cards
        } else {
            player.hand.clone()
        };
        
        // Get best 5-card hand
        let best_hand = get_best_hand(&player_hand);
        
        // Update player's hand to include original hole cards plus best hand info
        player.hand = vec![
            best_hand.0, best_hand.1, best_hand.2, best_hand.3, best_hand.4, best_hand.5,
        ];
    }
}

///This is the bring in bet for seven card draw and the rule for this is
///The player with the lowest-ranking up-card pays the bring-in, and betting proceeds after that in normal clockwise order
/// and to break ties in card ranks we will use the suit order of spades, hearts, diamonds, and clubs
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the players' wallets and game statistics.
/// It also handles the display of hands to active players.
pub async fn bring_in(lobby: &mut Lobby) {
    let mut players = lobby.players.lock().await;
    let mut lowest_up_card = 14;
    let mut lowest_up_card_player = 0;
    for (i, player) in players.iter().enumerate() {
        if player.state != player::FOLDED {
            if player.hand[2] % 13 < lowest_up_card {
                lowest_up_card = player.hand[2] % 13;
                lowest_up_card_player = i;
            }
        }
    }
    let bring_in = 15;
    players[lowest_up_card_player].wallet -= bring_in;
    players[lowest_up_card_player].current_bet += bring_in;
    lobby.pot += bring_in;
    players[lowest_up_card_player].state = player::CALLED;
    let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    lobby.lobby_wide_send(players_tx, format!("{} has the lowest up card and pays the bring-in of {}", players[lowest_up_card_player].name, bring_in)).await;

}

/// This function is used to remove the X cards from the players hand
/// It is used for the final round of 7 card stud where the players have to show their hands
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the players' hands by removing the X cards.
/// It also handles the display of hands to active players.
pub async fn get_rid_of_x(lobby: &Lobby) {
    let mut players = lobby.players.lock().await;
    for player in players.iter_mut() {
        if player.state == player::FOLDED {
            continue;
        }
        for card in player.hand.iter_mut() {
            if *card > 52 {
                *card -= 53;
            }
        }
        display_hand(vec![player.tx.clone()], vec![player.hand.clone()]).await;
    }
}

/// This function is used to handle the blinds for the poker game.
/// It deducts the small and big blinds from the respective players' wallets,
/// adds the blinds to the pot,
/// and sends a message to all players about the blinds paid.
/// 
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the players' wallets and the pot.
/// It also handles the display of blinds to all players.
pub async fn blinds(lobby: &mut Lobby) {
    let mut players = lobby.players.lock().await;
    let small_blind_player_i = (lobby.first_betting_player + 1) % lobby.current_player_count;
    let big_blind_player_i = (lobby.first_betting_player + 2) % lobby.current_player_count;
    let big_blind = 10;
    let small_blind = 5;

    let mut names: Vec<String> = Vec::new();
    // let big_blind_player = &mut players[big_blind_player_i as usize];
    
    let blind_player = &mut players[small_blind_player_i as usize];
    blind_player.wallet -= small_blind;
    blind_player.current_bet += small_blind;
    blind_player.state = player::CALLED;
    names.push(blind_player.name.clone());
    println!("smal blind player current bet: {}", blind_player.current_bet);
    println!("FUCK YOUU: {}",players[small_blind_player_i as usize].current_bet);

    let blind_player = &mut players[big_blind_player_i as usize];
    blind_player.wallet -= big_blind;
    blind_player.current_bet += big_blind;
    blind_player.state = player::CALLED;
    names.push(blind_player.name.clone());
    println!("big blind player current bet: {}", blind_player.current_bet);


    lobby.pot += small_blind;
    lobby.pot += big_blind;
    lobby.current_max_bet = big_blind;


    // Make a copy of the players for debugging
    let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    lobby.lobby_wide_send(players_tx, format!("{} has paid the small blind of {}\n{} has paid the big blind of {}", names[0], small_blind, names[1], big_blind)).await;
}

/// Converts a hand type integer to a readable string description.
/// 
/// # Arguments
/// * `hand_type` - An integer representing the hand type (0-8).
/// 
/// # Returns
/// This function returns a string representation of the hand type.
fn hand_type_to_string(hand_type: i32) -> String {
    match hand_type {
        0 => "High Card".to_string(),
        1 => "One Pair".to_string(),
        2 => "Two Pair".to_string(),
        3 => "Three of a Kind".to_string(),
        4 => "Straight".to_string(),
        5 => "Flush".to_string(),
        6 => "Full House".to_string(),
        7 => "Four of a Kind".to_string(),
        8 => "Straight Flush".to_string(),
        _ => "Unknown Hand".to_string(),
    }
}

/// This function is used to handle the game state machine for a five-card poker game.
/// It manages the different states of the game, including ante, dealing cards, betting rounds, drawing rounds, and showdown.
/// 
/// # Arguments
/// 
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function returns a string indicating the result of the game state machine execution.
/// It also handles the display of game information to all players.
pub async fn five_card_game_state_machine(server_lobby: Arc<Mutex<Lobby>>, mut player: Player, db: Arc<Database>) -> String {
    let player_name = player.name.clone();
    let player_lobby = player.lobby.clone();
    let lobby_name = player_lobby.lock().await.name.clone();
    let tx = player.tx.clone();
    
    // Update player state through the lobby
    {
        let mut lobby = player_lobby.lock().await;
        lobby.set_player_ready(&player_name, false).await;
        lobby.update_player_state(&player_name, player::IN_LOBBY).await;
        player.state = player::IN_LOBBY;
    }
    
    println!("{} has joined lobby: {}", player_name, player_lobby.lock().await.name);
    player_lobby.lock().await.new_player_join().await;


    // Add a delay of one second
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    // update player attribute from db
    let stats = db.player_stats(&player_name).await;
    if let Ok(stats) = stats {
        player.wallet = stats.wallet;
        player.games_played = stats.games_played;
        player.games_won = stats.games_won;
    } else {
        tx.send(Message::text(r#"{"error": "Failed to retrieve wallet"}"#)).unwrap();
        // add player to be deleted, then kick to server
    }
    
    loop {
        match player.state {
            player::IN_LOBBY => {
                println!("player {} is in lobby", player_name);

                loop {
                    let result = {
                        // Get next message from the player's websocket
                        let mut rx = player.rx.lock().await;
                        match rx.next().await {
                            Some(res) => res,
                            None => continue,
                        }
                    };
                    
                    if let Ok(msg) = result {
                        if let Ok(text) = msg.to_str() {
                            // Parse the incoming JSON message
                            let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                            
            
                            match client_msg {
                                Ok(ClientMessage::Quit) => {
                                    // QUIT LOBBY - Return to server lobby
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
                                    
                                    // Send redirect back to server lobby
                                    tx.send(Message::text(r#"{"message": "Leaving lobby...", "redirect": "server_lobby"}"#)).unwrap();
                                    return "Normal".to_string();
                                }
                                Ok(ClientMessage::Disconnect) => {
                                    // Player disconnected entirely
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name.clone()).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
            
                                    server_lobby.lock().await.remove_player(player_name.clone()).await;
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    
                                    // Update player stats from database
                                    if let Err(e) = db.update_player_stats(&player).await {
                                        eprintln!("Failed to update player stats: {}", e);
                                    }
                                    
                                    return "Disconnect".to_string();
                                }
                                Ok(ClientMessage::ShowLobbyInfo) => {
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
                                }
                                Ok(ClientMessage::Ready) => {
                                        // READY UP - through the lobby
                                        player_lobby.lock().await.check_ready(player_name.clone()).await;
                                        player_lobby.lock().await.send_player_list().await;
                                }
                                Ok(ClientMessage::ShowStats) => {
                                    // Get and send player stats
                                    let stats = db.player_stats(&player_name).await;

                                    if let Ok(stats) = stats {
                                        player.wallet = stats.wallet;
                                        player.games_played = stats.games_played;
                                        player.games_won = stats.games_won;
                                        player_lobby.lock().await.update_player_reference(&player).await;
                                        let stats_json = serde_json::json!({
                                            "stats": {
                                                "username": player_name,
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
                                Ok(ClientMessage::StartGame) => {
                                    // Start the game
                                    println!("player: {}, received start game", player.name.clone());
                                    let mut player_lobby_guard = player_lobby.lock().await;
                                    player_lobby_guard.turns_remaining -= 1;
                                    println!("turns remaining: {}", player_lobby_guard.turns_remaining);
                                    if player_lobby_guard.turns_remaining == 0 {
                                        player_lobby_guard.setup_game().await;
                                    }
                                    player.state = player::IN_GAME;
                                    break;
                                    
                                }
                                _ => {
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
            _ => { // player is in game
                println!("player {} is in game", player_name);
                let stats = db.player_stats(&player_name).await;
                if let Ok(stats) = stats {
                    player.wallet = stats.wallet;
                    player.games_played = 0;
                    player.games_won = 0;
                } else {
                    tx.send(Message::text(r#"{"error": "Failed to retrieve wallet"}"#)).unwrap();
                    // add player to be deleted, then kick to server
                }
                let mut exit = false;
                while !exit {
                    if let Ok(mut lobby_guard) = player_lobby.try_lock(){
                        if lobby_guard.game_state == lobby::JOINABLE {
                            drop(lobby_guard);
                            tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                        } else {
                            if lobby_guard.current_player_turn == player_name{
                                match  lobby_guard.game_state {
                                    lobby::START_OF_ROUND => {
                                        lobby_guard.game_state = lobby::ANTE;
                                        
                                        // Initialize turns counter for tracking player actions
                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                        lobby_guard.send_lobby_game_info().await;
                                    }
                                    lobby::ANTE => {
                                        println!("ante round current player: {}", player_name);
                                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                        tx.send(Message::text(r#"{"message": "Ante Round"}"#)).unwrap();
                                        println!("ante round message sent to player: {}", player_name);
                                        if player.wallet > 10 {
                                            // Deduct ante from player wallet and add to pot
                                            player.wallet -= 10;
                                            player.games_played += 1;
                                            lobby_guard.update_player_reference(&player).await;
                                            lobby_guard.pot += 10;
                                        } else {
                                            // Not enough money, mark as folded
                                            lobby_guard.update_player_state(&player_name, player::FOLDED).await;
                                            player.state = player::FOLDED;
                                        }
                                        lobby_guard.turns_remaining -= 1;
                                        {
                                            let stats_json = serde_json::json!({
                                                "stats": {
                                                    "username": player_name,
                                                    "gamesPlayed": player.games_played,
                                                    "gamesWon": player.games_won,
                                                    "wallet": player.wallet
                                                }
                                            });
                                            tx.send(Message::text(stats_json.to_string())).unwrap();
                                        }
                                        if lobby_guard.turns_remaining == 0{
                                            if lobby_guard.check_end_game().await {
                                                // game over if all or all-but-one players are folded or disconnected
                                                lobby_guard.game_state = lobby::SHOWDOWN;
                                            } else {
                                                // carry on if multiple players are still in the game
                                                lobby_guard.game_state = lobby::DEAL_CARDS;
                                            }
                                            lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                            lobby_guard.get_next_player(true).await;
                                            println!("ante round complete");
                                        } else {
                                            lobby_guard.get_next_player(false).await;
                                        }
                                        lobby_guard.send_lobby_game_info().await;
                                    }
                                    lobby::DEAL_CARDS => {
                                        println!("deal round current player: {}", player_name);
                                        tx.send(Message::text(r#"{"message": "Dealing Cards....."}"#)).unwrap();
                                        // Deal 5 cards to each active player
                                        if player.state != player::FOLDED {
                                            if player.hand.len() < 5 {
                                                player.hand.push(lobby_guard.deck.deal());
                                                lobby_guard.update_player_hand(&player_name, player.clone().hand).await;
                                            } else {
                                                lobby_guard.turns_remaining -= 1;
                                                if lobby_guard.turns_remaining == 0 {
                                                    if lobby_guard.check_end_game().await {
                                                        // game over if all or all-but-one players are folded or disconnected
                                                        lobby_guard.game_state = lobby::SHOWDOWN;
                                                    } else {
                                                        // carry on if multiple players are still in the game
                                                        lobby_guard.game_state = lobby::FIRST_BETTING_ROUND;
                                                    }
                                                    lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                                    lobby_guard.get_next_player(true).await;
                                                    lobby_guard.send_player_list().await;
                                                    lobby_guard.send_lobby_game_info().await;
                                                    println!("all cards dealt, moving to first betting round with player turn: {}", lobby_guard.current_player_turn);
                                                    continue;
                                                }
                                            }
                                        }
                                        lobby_guard.get_next_player(false).await;
                                    }
                                    lobby::FIRST_BETTING_ROUND | lobby::SECOND_BETTING_ROUND => {
                                        println!("betting round current player {}", player_name);
                                        // skip the player if they are folded or all in
                                        if player.state != player::FOLDED && player.state != player::ALL_IN {
                                            lobby_guard.send_lobby_game_info().await;
                                            loop {
                                                let result = {
                                                    // Get next message from the player's websocket
                                                    let mut rx = player.rx.lock().await;
                                                    match rx.next().await {
                                                        Some(res) => res,
                                                        None => continue,
                                                    }
                                                };
    
                                                if let Ok(msg) = result {
                                                    if let Ok(text) = msg.to_str() {
                                                        // Parse the incoming JSON message
                                                        let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                                                        match client_msg {
                                                            Ok(ClientMessage::Disconnect) => {
                                                                /*
                                                                Add current player into to-be-rmoved list and keep their player reference active within the players vector
                                                                
                                                                
                                                                 */
    
    
                                                                // // Player disconnected entirely
                                                                // let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                                                // if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                                                //     lobby_guard.remove_lobby(lobby_name.clone()).await;
                                                                // } else {
                                                                //     lobby_guard.update_lobby_names_status(lobby_name).await;
                                                                // }
                                                                // lobby_guard.broadcast_player_count().await;
                                                                // lobby_guard.send_lobby_info().await;
                                                                // lobby_guard.send_player_list().await;
                                        
                                                                // lobby_guard.remove_player(player_name.clone()).await;
                                                                // lobby_guard.broadcast_player_count().await;
                                                                
                                                                // // Update player stats from database
                                                                // if let Err(e) = db.update_player_stats(&player).await {
                                                                //     eprintln!("Failed to update player stats: {}", e);
                                                                // }
                                                                
                                                                // return "Disconnect".to_string();
                                                            }
                                                            _ => {
                                                                // pass in the players input and validate it (check, call, raise, fold, all in)
                                                                if let Ok(action) = client_msg {
                                                                    let (valid_action, reset) = betting_round(&mut player, &mut lobby_guard, action).await;
                                                                    if valid_action {
                                                                        println!("valid action");
                                                                        // update the server lobby player reference with updated clone data
                                                                        lobby_guard.update_player_reference(&player).await;
                                                                        if reset {
                                                                            println!("reseting turns remaining");
                                                                            // reset the turns_remaining counter if the player raised
                                                                            lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                                                        }
                                                                        break;
                                                                    }
                                                                } else {
                                                                    println!("Invalid client message received BAD, they try again");
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        lobby_guard.turns_remaining -= 1;
                                        println!("player {} finish turn", player_name);
                                        if lobby_guard.check_end_game().await {
                                            lobby_guard.clear_betting().await;
                                            player.current_bet = 0;
                                            lobby_guard.game_state = lobby::SHOWDOWN;
                                        } else {
                                            if lobby_guard.turns_remaining == 0 {
                                                if lobby_guard.game_state == lobby::FIRST_BETTING_ROUND {
                                                    lobby_guard.game_state = lobby::DRAW;
                                                } else if lobby_guard.game_state == lobby::SECOND_BETTING_ROUND {
                                                    lobby_guard.game_state = lobby::SHOWDOWN;
                                                }
                                                lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                                lobby_guard.clear_betting().await;
                                                player.current_bet = 0;
                                                lobby_guard.get_next_player(true).await;
                                                println!("betting round finished, next player turn: {}", lobby_guard.current_player_turn);
                                            } else {
                                                lobby_guard.get_next_player(false).await;
                                                println!("next player turn: {}", lobby_guard.current_player_turn);
                                            }
                                        }
                                        lobby_guard.send_lobby_game_info().await;
                                        lobby_guard.send_player_list().await;
                                        
                                    }
                                    lobby::DRAW => {
                                        println!("drawing round for player {}", player_name);
                                        tx.send(Message::text(r#"{"message": "Drawing Round"}"#)).unwrap();
                                        player.current_bet = 0; // reset attribute from betting round
                                        
                                        // Check if current player isn't folded
                                        if player.state != player::FOLDED {
                                            // Notify player it's their turn to draw
                                            let turn_message = serde_json::json!({
                                                "message": "Your turn to draw cards.",
                                                "action": "draw",
                                                "yourTurn": true,
                                                "gameState": lobby::DRAW
                                            });
                                            tx.send(Message::text(turn_message.to_string())).unwrap();
                                              
                                            // Send game info with the DRAW_PHASE state to trigger UI
                                            lobby_guard.send_lobby_game_info().await;
                                            
                                            // Wait for player's selection of cards to exchange
                                            loop {
                                                let result = {
                                                    // Get next message from the player's websocket
                                                    let mut rx = player.rx.lock().await;
                                                    match rx.next().await {
                                                        Some(res) => res,
                                                        None => continue,
                                                    }
                                                };
                                                if let Ok(msg) = result {
                                                    if let Ok(text) = msg.to_str() {
                                                        // Parse the incoming JSON message
                                                        let draw_msg: Result<serde_json::Value, _> = serde_json::from_str(text);
                                                        
                                                        if let Ok(draw_data) = draw_msg {
                                                            // Check if this is a DrawCards action
                                                            if let Some("DrawCards") = draw_data.get("action").and_then(|a| a.as_str()) {
                                                                // Get the indices of cards to replace
                                                                if let Some(indices) = draw_data.get("cardIndices").and_then(|i| i.as_array()) {
                                                                    let indices: Vec<usize> = indices
                                                                        .iter()
                                                                        .filter_map(|idx| idx.as_i64().map(|i| i as usize))
                                                                        .collect();
                                                                    
                                                                    // Get current hand
                                                                    let mut new_hand = player.hand.clone();
                                                                    
                                                                    // Replace selected cards with new ones
                                                                    for &idx in indices.iter() {
                                                                        if idx < new_hand.len() {
                                                                            new_hand[idx] = lobby_guard.deck.deal();
                                                                        }
                                                                    }
                                                                    
                                                                    // Update player's hand
                                                                    player.hand = new_hand.clone();
                                                                    lobby_guard.update_player_hand(&player_name, new_hand).await;
                                                                    
                                                                    // Broadcast to other players
                                                                    let exchanged_count = indices.len();
                                                                    lobby_guard.broadcast(format!("{} exchanged {} cards.", player_name, exchanged_count)).await;
                                                                    println!("{} exchanged {} cards.", player_name, exchanged_count);
                                                                    
                                                                    // Move to the next player
                                                                    lobby_guard.turns_remaining -= 1;
                                                                    if lobby_guard.turns_remaining == 0 {
                                                                        // All players have completed their draws
                                                                        lobby_guard.game_state = lobby::SECOND_BETTING_ROUND;
                                                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                                                        lobby_guard.get_next_player(true).await;
                                                                        lobby_guard.send_lobby_game_info().await;
                                                                        lobby_guard.send_player_list().await;
                                                                        println!("Drawing round complete, moving to second betting round");
                                                                    } else {
                                                                        lobby_guard.get_next_player(false).await;
                                                                        lobby_guard.send_lobby_game_info().await;
                                                                        lobby_guard.send_player_list().await;
                                                                    }
                                                                    
                                                                    // Update game info
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            // Skip players who are folded or all-in
                                            lobby_guard.turns_remaining -= 1;
                                            if lobby_guard.turns_remaining == 0 {
                                                // lobby_guard.game_state = SECOND_BETTING_ROUND;
                                                lobby_guard.game_state = SHOWDOWN;
    
                                                lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                                lobby_guard.get_next_player(true).await;
                                            } else {
                                                lobby_guard.get_next_player(false).await;
                                            }
                                            lobby_guard.send_lobby_game_info().await;
                                            lobby_guard.send_player_list().await;
                                        }
                                    }
                                    lobby::SHOWDOWN => {
                                        tx.send(Message::text(r#"{"message": "Showdown Round"}"#)).unwrap();
                                        lobby_guard.turns_remaining -= 1;
                                        if lobby_guard.turns_remaining == 0 {
                                            // First determine winner(s) before creating showdown data
                                            let winners = lobby_guard.showdown().await;
                                            let showdown_data;
                                            {
                                                // Display all players' hands to everyone
                                                let players = lobby_guard.players.lock().await;
                                                
                                                // Construct data for all active hands
                                                let mut all_hands_data = Vec::new();
                                                for player in players.iter() {
                                                    if player.state != player::FOLDED {
                                                        // Check if this player is a winner
                                                        let is_winner = winners.contains(&player.name);
                                                        
                                                        let hand_data = serde_json::json!({
                                                            "playerName": player.name,
                                                            "hand": player.hand[0..].to_vec(), // Remove hand type from the array sent
                                                            "winner": is_winner // Set winner flag based on the calculated winners
                                                        });
                                                        all_hands_data.push(hand_data);
                                                    }
                                                }
                                                
                                                // Create a formatted winner message
                                                let winner_message = if winners.len() > 0 {
                                                    format!("{} won the pot of ${}", winners.join(", "), lobby_guard.pot)
                                                } else {
                                                    "No winners determined".to_string()
                                                };
                                                
                                                // Send all hands data to all players - using proper command format
                                                showdown_data = serde_json::json!({
                                                    "command": "showdownHands",
                                                    "data": {
                                                        "hands": all_hands_data,
                                                        "pot": lobby_guard.pot,
                                                        "winnerMessage": winner_message
                                                    }
                                                });
                                            }
                                            lobby_guard.broadcast_json(showdown_data.to_string()).await;
                                            println!("Showdown data sent to all players");
                                            
                                            // Wait briefly before ending the round
                                            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                            lobby_guard.game_state = lobby::UPDATE_DB;
                                            lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                            lobby_guard.get_next_player(true).await;
                                        } else {
                                            // Proceed to next player
                                            lobby_guard.get_next_player(false).await;
                                        }
                                    }
                                    lobby::UPDATE_DB => {
                                        // Update player stats and wallets in database
                                        tx.send(Message::text(r#"{"message": "Game Ended"}"#)).unwrap();
                                        lobby_guard.turns_remaining -= 1;
                                        lobby_guard.get_next_player(false).await;
                                        if lobby_guard.turns_remaining == 0 {
                                            println!("player {} activating finished_game", player_name);
                                            lobby_guard.finished_game().await;
                                            lobby_guard.send_lobby_game_info().await;
                                            lobby_guard.send_player_list().await;
                                            println!("finished_game completed");
                                        }
                                        player.hand.clear();
                                        player.state = player::IN_LOBBY;
                                        exit = true;
                                        drop(lobby_guard);                                }
                                    _ => {
                                        panic!("Invalid game state: {}", lobby_guard.game_state);
                                    }
                                }
                            } else {
                                drop(lobby_guard);
                            }
                        }
                    }
                    let result = {
                        // Get next message from the player's websocket
                        let mut rx = player.rx.lock().await;
                        match rx.next().await {
                            Some(res) => res,
                            None => continue,
                        }
                    };
                    if let Ok(msg) = result {
                        if let Ok(text) = msg.to_str() {
                            // Parse the incoming JSON message
                            let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                            match client_msg {
                                Ok(ClientMessage::Disconnect) => {
                                    // Player disconnected entirely
                                    {
                                        let mut lobby_guard = player_lobby.lock().await;
                                        // Add to the to_be_deleted list
                                        // Update player state to folded if in a game
                                        if !lobby_guard.to_be_deleted.contains(&player_name) {
                                            lobby_guard.to_be_deleted.push(player_name.clone());
                                        }
                                        lobby_guard.update_player_state(&player_name, player::FOLDED).await;
                                        
                                        // Mark player as disconnected for UI display
                                        let mut players = lobby_guard.players.lock().await;
                                        if let Some(p) = players.iter_mut().find(|p| p.name == player_name) {
                                            p.disconnected = true;
                                        }
                                        
                                        // Notify other players
                                        let disconnect_msg = serde_json::json!({
                                            "message": format!("{} has disconnected and folded.", player_name),
                                            "playerDisconnected": {
                                                "name": player_name,
                                                "state": player::FOLDED
                                            }
                                        });
                                        lobby_guard.broadcast_json(disconnect_msg.to_string()).await;

                                        lobby_guard.send_lobby_game_info().await;
                                        lobby_guard.send_player_list().await;
                                    }

                                    if let Err(e) = db.update_player_stats(&player).await {
                                        eprintln!("Failed to update player stats: {}", e);
                                    }

                                    
                                    return "Disconnect".to_string();
                                }
                                _ => {
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // After each state transition, check for spectators
        // check_for_spectators(lobby_).await;
    }
}

/// This function is used to handle the game state machine for a seven-card poker game.
/// It manages the different states of the game, including dealing cards, betting rounds, and showdown.
/// 
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the game state and player statistics.
/// It also handles the display of game information to all players.

pub async fn seven_card_game_state_machine(server_lobby: Arc<Mutex<Lobby>>, mut player: Player, db: Arc<Database>) -> String {
    let player_name = player.name.clone();
    let player_lobby = player.lobby.clone();
    let tx = player.tx.clone();
    
    // Update player state through the lobby
    {
        let mut lobby = player_lobby.lock().await;
        lobby.set_player_ready(&player_name, false).await;
        lobby.update_player_state(&player_name, player::IN_LOBBY).await;
        player.state = player::IN_LOBBY;
    }
    
    println!("{} has joined lobby: {}", player_name, player_lobby.lock().await.name);
    player_lobby.lock().await.new_player_join().await;
    
    // Add a delay of one second
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    loop {
        match player.state {
            player::IN_LOBBY => {
                loop {
                    let result = {
                        // Get next message from the player's websocket
                        let mut rx = player.rx.lock().await;
                        match rx.next().await {
                            Some(res) => res,
                            None => continue,
                        }
                    };
                    
                    if let Ok(msg) = result {
                        if let Ok(text) = msg.to_str() {
                            // Parse the incoming JSON message
                            let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                            
                            let lobby_name = player_lobby.lock().await.name.clone();
            
                            match client_msg {
                                Ok(ClientMessage::Quit) => {
                                    // QUIT LOBBY - Return to server lobby
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
                                    
                                    // Send redirect back to server lobby
                                    tx.send(Message::text(r#"{"message": "Leaving lobby...", "redirect": "server_lobby"}"#)).unwrap();
                                    return "Normal".to_string();
                                }
                                Ok(ClientMessage::Disconnect) => {
                                    // Player disconnected entirely
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name.clone()).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
            
                                    server_lobby.lock().await.remove_player(player_name.clone()).await;
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    
                                    // Update player stats from database
                                    if let Err(e) = db.update_player_stats(&player).await {
                                        eprintln!("Failed to update player stats: {}", e);
                                    }
                                    
                                    return "Disconnect".to_string();
                                }
                                Ok(ClientMessage::ShowLobbyInfo) => {
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
                                }
                                Ok(ClientMessage::Ready) => {
                                    // READY UP - through the lobby
                                    player_lobby.lock().await.check_ready(player_name.clone()).await;
                                    player_lobby.lock().await.send_player_list().await;
                                }
                                Ok(ClientMessage::ShowStats) => {
                                    // Get and send player stats
                                    let stats = db.player_stats(&player_name).await;
                                    if let Ok(stats) = stats {
                                        let stats_json = serde_json::json!({
                                            "stats": {
                                                "username": player_name,
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
                                Ok(ClientMessage::StartGame) => {
                                    // Start the game
                                    println!("player: {}, received start game", player.name.clone());
                                    let mut started = false;
                                    while !started {
                                        if let Ok(mut player_lobby_guard) = player_lobby.try_lock() {
                                            player_lobby_guard.turns_remaining -= 1;
                                            println!("turns remaining: {}", player_lobby_guard.turns_remaining);
                                            if player_lobby_guard.turns_remaining == 0 {
                                                player_lobby_guard.setup_game().await;
                                            }
                                            player_lobby_guard.update_player_state(&player_name, player::IN_GAME).await;
                                            player.state = player::IN_GAME;
                                            started = true;
                                        }
                                    }
                                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                                    break;
                                }
                                _ => {
                                    // Unsupported action in lobby: disregard
                                    if let Ok(player_lobby_guard) = player_lobby.try_lock() {
                                        if player_lobby_guard.game_state != JOINABLE{
                                            break;
                                        }
                                    } else {
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => { // player is in game
                // Load player stats from database for wallet
                let stats = db.player_stats(&player_name).await;
                if let Ok(stats) = stats {
                    player.wallet = stats.wallet;
                    player.games_played = 0;
                    player.games_won = 0;
                } else {
                    tx.send(Message::text(r#"{"error": "Failed to retrieve wallet"}"#)).unwrap();
                }
                                
                loop {
                    if let Ok(mut lobby_guard) = player_lobby.try_lock() {
                        if lobby_guard.current_player_turn == player_name {
                            match lobby_guard.game_state {
                                JOINABLE => {
                                    continue;
                                }
                                START_OF_ROUND => {
                                    lobby_guard.first_betting_player = (lobby_guard.first_betting_player + 1) % lobby_guard.current_player_count;
                                    lobby_guard.game_state = DEAL_CARDS;
                                    lobby_guard.deck.shuffle(); // Shuffle the deck at the start of the round
                                    // Initialize turns counter for tracking player actions
                                    lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                    lobby_guard.send_lobby_game_info().await;
                                }
                                DEAL_CARDS => {
                                    // Seven-card stud dealing logic
                                    println!("DEALING CARDS to player {}", player.name.clone());
                                    if player.state != player::FOLDED {
                                        // Deal cards according to the rules of Seven Card Stud
                                        if lobby_guard.deal_card_counter == 0 {
                                            // check if the player has money to play. if they do not, skip them and player state = folded
                                            if player.wallet == 0 {
                                                player.state = player::FOLDED;
                                                lobby_guard.update_player_state(&player_name, player.state).await;
                                                continue;
                                            }
                                            // First dealing round: 2 down, 1 up
                                            // Deal first two cards face down
                                            for _ in 0..2 {
                                                let card = lobby_guard.deck.deal();
                                                player.hand.push(card + 53); // +53 indicates face down
                                            }
                                            // Deal third card face up
                                            player.hand.push(lobby_guard.deck.deal());

                                            // they are dealt cards, they played a game
                                            player.games_played += 1;
                                            lobby_guard.update_player_reference(&player).await;
                                        }
                                        else if lobby_guard.deal_card_counter >= 1 && lobby_guard.deal_card_counter < 4 {
                                            // Deal one face-up card
                                            player.hand.push(lobby_guard.deck.deal());
                                        } else if lobby_guard.deal_card_counter == 4 {
                                            // Deal the final card face down
                                            let card = lobby_guard.deck.deal();
                                            player.hand.push(card + 53); // +53 indicates face down
                                        }
                                        lobby_guard.update_player_hand(&player_name, player.clone().hand).await;
                                    }
                                    lobby_guard.turns_remaining -= 1;
                                    if lobby_guard.turns_remaining == 0 {
                                        /*
                                            calculate the player with best hand of face up cards
                                            and update to the next current player
                                        */
                                        if lobby_guard.deal_card_counter == 0 {
                                            lobby_guard.game_state = BRING_IN;
                                            // If this player has the lowest up card, they pay bring-in
                                            let mut lowest_up_card = 14;                                    
                                            let mut lowest_up_card_player_idx = -1;
                                            {
                                                let players = lobby_guard.players.lock().await;
                                                for (i, p) in players.iter().enumerate() {
                                                    if p.state != player::FOLDED && p.hand.len() >= 3 {
                                                        let card_value = if p.hand[2] % 13 == 0 { 13 } else { p.hand[2] % 13 }; // Treat Ace as the highest card
                                                        if card_value < lowest_up_card {
                                                            lowest_up_card = card_value;
                                                            lowest_up_card_player_idx = i as i32;
                                                        }
                                                    }
                                                }
                                            }
                                            let player_name = lobby_guard.players.lock().await[lowest_up_card_player_idx as usize].name.clone();
                                            lobby_guard.current_player_turn = player_name;
                                            lobby_guard.current_player_index = lowest_up_card_player_idx;
                                        }
                                        else if lobby_guard.deal_card_counter < 4 {
                                            lobby_guard.game_state = BETTING_ROUND;
                                            let mut best_p_index ;
                                            {
                                                let players = lobby_guard.players.lock().await;
                                                let mut best_hand = (-1, -1, -1, -1, -1, -1); // Initialize best hand tuple
                                                let mut best_player_index = -1;
                                                
                                                for (i, p) in players.iter().enumerate() {
                                                    if (p.state != player::FOLDED && p.state != player::ALL_IN) && p.hand.len() >= 3 {
                                                        let face_up_cards: Vec<i32> = p.hand.iter()
                                                            .skip(2) // Skip the first two face-down cards
                                                            .filter(|&&card| card <= 52) // Only include face-up cards
                                                            .map(|&card| card) // Keep original card values
                                                            .collect();
                                                        println!("{} cards {:?}", p.name, p.hand);
                                                        println!("{} face up cards {:?}", p.name, face_up_cards);
                                                        let hand_type = get_hand_type(&face_up_cards); // Use only the face-up cards
                                                        println!("{} hand type: {:?}", p.name, hand_type);
                                                        if hand_type > best_hand {
                                                            best_hand = hand_type;
                                                            best_player_index = i as i32;
                                                            println!("player index with best hand {}", best_player_index)
                                                        }
                                                    }
                                                    else {best_player_index = 0;}
                                                    
                                                }
                                                // drop(players);
                                                // if best_player_index != -1 {
                                                //     lobby_guard.current_player_index = best_player_index;
                                                // } else {
                                                //     lobby_guard.current_player_index = lobby_guard.current_player_index; // Default to current player if no valid hands found
                                                // }
                                                best_p_index = best_player_index;
                                            }
                                            lobby_guard.current_player_index = best_p_index;
                                            let player_name = lobby_guard.players.lock().await[best_p_index as usize].name.clone();
                                            println!("player name of best current hand {}", player_name);
                                            lobby_guard.current_player_turn = player_name;
                                        } 
                                        else {
                                            lobby_guard.game_state = BETTING_ROUND;
                                            // order is preserved in the last betting round since last card is face down, we already know who had the strongest hand
                                            lobby_guard.get_next_player(false).await;
                                        }
                                        lobby_guard.deal_card_counter += 1;
                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                        lobby_guard.send_player_list().await;
                                        lobby_guard.send_lobby_game_info().await;
                                    } else {
                                        lobby_guard.get_next_player(false).await;
                                        lobby_guard.send_player_list().await;
                                        lobby_guard.send_lobby_game_info().await;
                                    }
                                }
                                BRING_IN => {
                                    lobby_guard.broadcast("Bring In stage".to_string()).await;
                                    
                                    let bring_in_amount = 15; // Standard bring-in amount
                                    player.wallet -= bring_in_amount;
                                    player.current_bet += bring_in_amount;
                                    player.state = player::CALLED;
                                    lobby_guard.pot += bring_in_amount;
                                    lobby_guard.current_max_bet = bring_in_amount;
                                    
                                    // Update the player in the lobby
                                    lobby_guard.players.lock().await[lobby_guard.current_player_index as usize].wallet = player.wallet;
                                    lobby_guard.players.lock().await[lobby_guard.current_player_index as usize].current_bet = player.current_bet;
                                    lobby_guard.players.lock().await[lobby_guard.current_player_index as usize].state = player.state;
                                    
                                    // Broadcast the bring-in action
                                    lobby_guard.broadcast(format!("{} has the lowest up card and pays the bring-in of {}", player_name, bring_in_amount)).await;
                                    
                                    // Set up for the betting round
                                    lobby_guard.game_state = BETTING_ROUND;
                                    lobby_guard.turns_remaining = lobby_guard.current_player_count - 1; // -1 because bring-in player already acted
                                    
                                    // Move to next player BEFORE sending game info (this is the key fix)
                                    lobby_guard.get_next_player(false).await; // Use true to ensure we move to the next valid player
                                    
                                    // Now send game info with the new player turn already set
                                    lobby_guard.send_lobby_game_info().await;
                                    lobby_guard.send_player_list().await;
                                }
                                lobby::BETTING_ROUND => {
                                    println!("betting round current player {}", player_name);
                                    tx.send(Message::text(r#"{"message": "Betting Round"}"#)).unwrap();
                                    // skip the player if they are folded or all in
                                    if player.state != player::FOLDED && player.state != player::ALL_IN {
                                        loop {
                                            let result = {
                                                // Get next message from the player's websocket
                                                let mut rx = player.rx.lock().await;
                                                match rx.next().await {
                                                    Some(res) => res,
                                                    None => continue,
                                                }
                                            };

                                            if let Ok(msg) = result {
                                                if let Ok(text) = msg.to_str() {
                                                    // Parse the incoming JSON message
                                                    let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                                                    match client_msg {
                                                        Ok(ClientMessage::Disconnect) => {
                                                            /*
                                                            Add current player into to-be-rmoved list and keep their player reference active within the players vector
                                                            
                                                            
                                                             */
                                                        }
                                                        _ => {
                                                            // pass in the players input and validate it (check, call, raise, fold, all in)
                                                            if let Ok(action) = client_msg {
                                                                let (valid_action, reset) = betting_round(&mut player, &mut lobby_guard, action).await;
                                                                if valid_action {
                                                                    println!("valid action");
                                                                    // update the server lobby player reference with updated clone data
                                                                    lobby_guard.update_player_reference(&player).await;
                                                                    if reset {
                                                                        println!("reseting turns remaining");
                                                                        // reset the turns_remaining counter if the player raised
                                                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                                                    }
                                                                    break;
                                                                }
                                                            } else {
                                                                println!("Invalid client message received BAD, they try again");
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    lobby_guard.turns_remaining -= 1;
                                    println!("player {} finish turn", player_name);
                                    if lobby_guard.check_end_game().await {
                                        lobby_guard.clear_betting().await;
                                        lobby_guard.game_state = lobby::SHOWDOWN;
                                    } else {
                                        if lobby_guard.turns_remaining == 0 {
                                            if lobby_guard.betting_round_counter < 4 {
                                                println!("moving to dealing another card");
                                                lobby_guard.game_state = lobby::DEAL_CARDS;
                                            } else if lobby_guard.betting_round_counter == 4 {
                                                lobby_guard.game_state = lobby::SHOWDOWN;
                                                break;
                                            }
                                            lobby_guard.betting_round_counter += 1;
                                            // broadcast the betting round count
                                            println!("Betting round {}", lobby_guard.betting_round_counter);
                                            lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                            lobby_guard.clear_betting().await;
                                            lobby_guard.get_next_player(false).await;
                                            println!("betting round finished, next player turn: {}", lobby_guard.current_player_turn);
                                        } else {
                                            lobby_guard.get_next_player(false).await;
                                            println!("next player turn: {}", lobby_guard.current_player_turn);
                                        }
                                    }
                                    lobby_guard.send_lobby_game_info().await;
                                    lobby_guard.send_player_list().await;
                                    
                                }
                                SHOWDOWN => {
                                    lobby_guard.broadcast("------Showdown Round!------".to_string()).await;
                                    
                                    // Reveal all face-down cards
                                    get_rid_of_x(&lobby_guard).await;
                                    lobby_guard.send_player_list().await;

                                    // create a copy of players hands
                                    let mut player_hands_copy = Vec::new();
                                    for player in lobby_guard.players.lock().await.iter() {
                                        if player.state != player::FOLDED{
                                            let mut hand = player.hand.clone();
                                            // Remove face-down cards (53) from the hand
                                            hand.retain(|&card| card <= 52);
                                            player_hands_copy.push(hand);
                                        }
                                    }
                                    // Find best 5-card hand from 7 cards
                                    update_players_hand(&lobby_guard).await;
                                    
                                    // Determine winner(s) and award pot
                                    lobby_guard.showdown().await;
                                    
                                    // reasign hands so ui doesnt ruin everything
                                    {
                                        let mut players = lobby_guard.players.lock().await;
                                        let mut hand_iter = player_hands_copy.iter();
                                        for player in players.iter_mut() {
                                            if player.state != player::FOLDED {
                                                if let Some(hand) = hand_iter.next() {
                                                    player.hand = hand.clone();
                                                }
                                            }
                                        }
                                    }

                                    // Wait briefly before ending the round
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                    
                                    lobby_guard.game_state = UPDATE_DB;
                                }
                                UPDATE_DB => {
                                    lobby_guard.update_db().await;
                                    lobby_guard.send_lobby_game_info().await;
                                    // lobby_guard.send_player_list().await;
                                    // Update player stats and wallets in database
                                    let player_states = lobby_guard.get_player_names_and_status().await;
                                    for (player_name, _) in player_states {
                                        // Reset player ready status
                                        lobby_guard.set_player_ready(&player_name, false).await;
                                        // Reset player state to IN_LOBBY
                                        lobby_guard.update_player_state(&player_name, player::IN_LOBBY).await;
                                    }
                                    
                                    // Reset game variables
                                    lobby_guard.pot = 0;
                                    lobby_guard.current_max_bet = 0;
                                    lobby_guard.betting_round_counter = 0;
                                    lobby_guard.deal_card_counter = 0;
                                    
                                    // Send game end notification to all clients
                                    let end_game_msg = serde_json::json!({
                                        "gameEnd": true,
                                        "message": "Game has ended. All players need to ready up to start a new game."
                                    });
                                    lobby_guard.broadcast(end_game_msg.to_string()).await;
                                    
                                    // Reset game state back to JOINABLE
                                    lobby_guard.game_state = lobby::JOINABLE;
                                    lobby_guard.send_lobby_game_info().await;
                                    lobby_guard.send_player_list().await;
                                    println!("Game ended, lobby reset to JOINABLE state");
                                    break;
                                }
                                _ => {
                                    panic!("Invalid game state: {}", lobby_guard.game_state);
                                }
                            }
                        } else {
                            drop(lobby_guard);
                        }
                    }
                    
                    // Handle incoming messages when it's not player's turn
                    let result = {
                        let mut rx = player.rx.lock().await;
                        match rx.next().await {
                            Some(res) => res,
                            None => continue,
                        }
                    };
                    
                    if let Ok(msg) = result {
                        if let Ok(text) = msg.to_str() {
                            let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                            match client_msg {
                                Ok(ClientMessage::Disconnect) => {
                                    // Handle player disconnection
                                    let lobby_name = player_lobby.lock().await.name.clone();
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name.clone()).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
            
                                    server_lobby.lock().await.remove_player(player_name.clone()).await;
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    
                                    // Update player stats from database
                                    if let Err(e) = db.update_player_stats(&player).await {
                                        eprintln!("Failed to update player stats: {}", e);
                                    }

                                    
                                    return "Disconnect".to_string();
                                }
                                _ => {
                                    continue;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// This function is used to handle the game state machine for a Texas Hold'em poker game.
/// It manages the different states of the game, including blinds, dealing cards, betting rounds, and showdown.
/// 
/// # Arguments
/// * `lobby` - A mutable reference to the `Lobby` struct, which contains the game state and player information.
/// 
/// # Returns
/// 
/// This function does not return a value. It updates the game state and player statistics.
/// It also handles the display of game information to all players.
pub async fn texas_holdem_game_state_machine(server_lobby: Arc<Mutex<Lobby>>, mut player: Player, db: Arc<Database>) -> String {
    let player_name = player.name.clone();
    let player_lobby = player.lobby.clone();
    let tx = player.tx.clone();
    
    // Update player state through the lobby
    {
        let mut lobby = player_lobby.lock().await;
        lobby.set_player_ready(&player_name, false).await;
        lobby.update_player_state(&player_name, player::IN_LOBBY).await;
        player.state = player::IN_LOBBY;
    }
    
    println!("{} has joined lobby: {}", player_name, player_lobby.lock().await.name);
    player_lobby.lock().await.new_player_join().await;

    // Add a delay of one second
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    loop {
        match player.state {
            player::IN_LOBBY => {
                loop {
                    let result = {
                        // Get next message from the player's websocket
                        let mut rx = player.rx.lock().await;
                        match rx.next().await {
                            Some(res) => res,
                            None => continue,
                        }
                    };
                    
                    if let Ok(msg) = result {
                        if let Ok(text) = msg.to_str() {
                            // Parse the incoming JSON message
                            let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                            
                            let lobby_name = player_lobby.lock().await.name.clone();
            
                            match client_msg {
                                Ok(ClientMessage::Quit) => {
                                    // QUIT LOBBY - Return to server lobby
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
                                    
                                    // Send redirect back to server lobby
                                    tx.send(Message::text(r#"{"message": "Leaving lobby...", "redirect": "server_lobby"}"#)).unwrap();
                                    return "Normal".to_string();
                                }
                                Ok(ClientMessage::Disconnect) => {
                                    // Player disconnected entirely
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name.clone()).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
            
                                    server_lobby.lock().await.remove_player(player_name.clone()).await;
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    
                                    // Update player stats from database
                                    if let Err(e) = db.update_player_stats(&player).await {
                                        eprintln!("Failed to update player stats: {}", e);
                                    }
                                    
                                    return "Disconnect".to_string();
                                }
                                Ok(ClientMessage::ShowLobbyInfo) => {
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
                                }
                                Ok(ClientMessage::Ready) => {
                                    // READY UP - through the lobby
                                    player_lobby.lock().await.check_ready(player_name.clone()).await;
                                    player_lobby.lock().await.send_player_list().await;
                                }
                                Ok(ClientMessage::ShowStats) => {
                                    // Get and send player stats
                                    let stats = db.player_stats(&player_name).await;
                                    if let Ok(stats) = stats {
                                        let stats_json = serde_json::json!({
                                            "stats": {
                                                "username": player_name,
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
                                Ok(ClientMessage::StartGame) => {
                                    // Start the game
                                    println!("player: {}, received start game", player.name.clone());
                                    let mut started = false;
                                    while !started {
                                        if let Ok(mut player_lobby_guard) = player_lobby.try_lock() {
                                            // Check if enough players are ready to start
                                            let player_states = player_lobby_guard.get_player_names_and_status().await;
                                            let ready_players = player_states.iter().filter(|(_, ready)| *ready).count();
                                            
                                            // If more than 1 player is ready, start the game immediately
                                            if ready_players >= 2 {
                                                player_lobby_guard.setup_game().await;
                                                
                                                // Update all ready players to IN_GAME state
                                                for (player_name, ready) in player_states {
                                                    if ready {
                                                        player_lobby_guard.update_player_state(&player_name, player::IN_GAME).await;
                                                    }
                                                }
                                                
                                                player.state = player::IN_GAME;
                                                started = true;
                                            } else {
                                                // Not enough ready players yet
                                                tx.send(Message::text(r#"{"message": "Need at least 2 ready players to start the game"}"#)).unwrap();
                                                break;
                                            }
                                        }
                                    }
                                    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                                    break;
                                }
                                _ => {
                                    // Unsupported action in lobby: disregard
                                    if let Ok(player_lobby_guard) = player_lobby.try_lock() {
                                        if player_lobby_guard.game_state != JOINABLE{
                                            break;
                                        }
                                    } else {
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => { // player is in game
                let stats = db.player_stats(&player_name).await;
                if let Ok(stats) = stats {
                    player.wallet = stats.wallet;
                    player.games_played = 0;
                    player.games_won = 0;
                } else {
                    tx.send(Message::text(r#"{"error": "Failed to retrieve wallet"}"#)).unwrap();
                }
                
                let mut betting_round_count = 1;
                let mut deal_card_counter = 1;
                
                loop {
                    if let Ok(mut lobby_guard) = player_lobby.try_lock() {
                        if lobby_guard.current_player_turn == player_name {
                            match lobby_guard.game_state {
                                lobby::JOINABLE => {
                                    continue;
                                }
                                lobby::START_OF_ROUND => {
                                    // Increment games_played for ALL players at the start of a round
                                    {
                                        let mut players = lobby_guard.players.lock().await;
                                        for p in players.iter_mut() {
                                            // Only increment for active players (not folded/disconnected)
                                            if p.state != player::FOLDED {
                                                p.games_played += 1;
                                                println!("Incrementing games_played for player {}: now {}", p.name, p.games_played);
                                            }
                                        }
                                    }
                                    
                                    lobby_guard.game_state = lobby::SMALL_AND_BIG_BLIND;
                                    
                                    // Initialize turns counter for tracking player actions
                                    lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                    lobby_guard.send_lobby_game_info().await;
                                }
                                lobby::SMALL_AND_BIG_BLIND => {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    lobby_guard.broadcast("Adding Small and Big Blind".to_string()).await;
                                    
                                    // Implement blinds logic directly in the state machine instead of calling the function
                                    let big_blind = 10;
                                    let small_blind = 5;
                                    let mut names: Vec<String> = Vec::new();
                                    
                                    // Calculate small blind and big blind player indices
                                    let small_blind_player_i = (lobby_guard.first_betting_player + 1) % lobby_guard.current_player_count;
                                    let big_blind_player_i = (lobby_guard.first_betting_player + 2) % lobby_guard.current_player_count;
                                    
                                    // Apply blinds
                                    let players_tx;
                                    {
                                        let mut players = lobby_guard.players.lock().await;
                                        
                                        // Small blind
                                        let small_blind_player = &mut players[small_blind_player_i as usize];
                                        small_blind_player.wallet -= small_blind;
                                        small_blind_player.current_bet = small_blind; // Set directly instead of adding
                                        small_blind_player.state = player::CALLED;
                                        names.push(small_blind_player.name.clone());
                                        println!("Small blind player ({}) current bet: {}", small_blind_player.name, small_blind_player.current_bet);
                                        
                                        // Big blind
                                        let big_blind_player = &mut players[big_blind_player_i as usize];
                                        big_blind_player.wallet -= big_blind;
                                        big_blind_player.current_bet = big_blind; // Set directly instead of adding
                                        big_blind_player.state = player::CALLED;
                                        names.push(big_blind_player.name.clone());
                                        println!("Big blind player ({}) current bet: {}", big_blind_player.name, big_blind_player.current_bet);
                                        
                                        // Get player transactions before releasing the lock
                                        players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
                                    }
                                    
                                    // Update pot and max bet after players lock is dropped
                                    lobby_guard.pot += small_blind + big_blind;
                                    lobby_guard.current_max_bet = big_blind;
                                    
                                    // Send message to players
                                    lobby_guard.lobby_wide_send(players_tx, format!("{} has paid the small blind of {}\n{} has paid the big blind of {}", 
                                        names[0], small_blind, names[1], big_blind)).await;
                                    
                                    // Debug current bets before state change
                                    {
                                        let players = lobby_guard.players.lock().await;
                                        for player in players.iter() {
                                            println!("Before DEAL_CARDS - Player {} bet: {}", player.name, player.current_bet);
                                        }
                                    }
                                    
                                    // Update the first betting player to be after the big blind
                                    lobby_guard.first_betting_player = (lobby_guard.first_betting_player + 3) % lobby_guard.current_player_count;
                                    lobby_guard.get_next_player(true).await;
                                    
                                    // Transition to DEAL_CARDS state
                                    lobby_guard.game_state = lobby::DEAL_CARDS;
                                    
                                    // Send updated game information without resetting bets
                                    lobby_guard.send_lobby_game_info().await;
                                    lobby_guard.send_player_list().await;
                                    
                                    // Debug current bets after state change
                                    {
                                        let players = lobby_guard.players.lock().await;
                                        for player in players.iter() {
                                            println!("After DEAL_CARDS - Player {} bet: {}", player.name, player.current_bet);
                                        }
                                    }
                                }
                                lobby::DEAL_CARDS => {
                                    // If player doesn't have enough money to play then fold them
                                    if player.wallet < 10 {
                                        player.state = player::FOLDED;
                                        player_lobby.lock().await.update_player_state(&player_name, player::FOLDED).await;
                                        tx.send(Message::text(r#"{"message": "You have been folded due to insufficient funds"}"#)).unwrap();
                                        continue;
                                    }
                                    //print deal card counter
                                    println!("deal card counter {}", deal_card_counter);
                                    if deal_card_counter == 1 {
                                        // Pre-flop: Deal 2 hole cards to each player
                                        lobby_guard.broadcast("Dealing hole cards...".to_string()).await;
                                        lobby_guard.deck.shuffle();
                                        deal_cards_texas(&mut lobby_guard, deal_card_counter).await;
                                        deal_card_counter += 1;
                                        
                                        // Move to first betting round (pre-flop)
                                        lobby_guard.game_state = BETTING_ROUND;
                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                        lobby_guard.send_player_list().await; // Add this line to send cards to clients
                                        lobby_guard.send_lobby_game_info().await;
                                    } else if deal_card_counter == 2 {
                                        // Flop: Deal 3 community cards
                                        lobby_guard.broadcast("Dealing the flop...".to_string()).await;
                                        deal_cards_texas(&mut lobby_guard, deal_card_counter).await;
                                        deal_card_counter += 1;
                                        
                                        // Reset betting for flop betting round
                                        lobby_guard.reset_current_bets().await;
                                        lobby_guard.game_state = BETTING_ROUND;
                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                        lobby_guard.send_lobby_game_info().await;
                                    } else if deal_card_counter == 3 {
                                        // Turn: Deal 1 community card
                                        lobby_guard.broadcast("Dealing the turn...".to_string()).await;
                                        deal_cards_texas(&mut lobby_guard, deal_card_counter).await;
                                        deal_card_counter += 1;
                                        
                                        // Reset betting for turn betting round
                                        lobby_guard.reset_current_bets().await;
                                        lobby_guard.game_state = BETTING_ROUND;
                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                        lobby_guard.send_lobby_game_info().await;
                                    } else if deal_card_counter == 4 {
                                        // River: Deal last community card
                                        lobby_guard.broadcast("Dealing the river...".to_string()).await;
                                        deal_cards_texas(&mut lobby_guard, deal_card_counter).await;
                                        deal_card_counter += 1;
                                        
                                        // Reset betting for river betting round
                                        lobby_guard.reset_current_bets().await;
                                        lobby_guard.game_state = BETTING_ROUND;
                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                        lobby_guard.send_lobby_game_info().await;
                                    }else if deal_card_counter == 5 {
                                        // After the river, go to showdown
                                        lobby_guard.game_state = SHOWDOWN;
                                        lobby_guard.send_lobby_game_info().await;
                                    }
                                }
                                lobby::BETTING_ROUND => {
                                    println!("betting round current player {}", player_name);
                                    tx.send(Message::text(r#"{"message": "Betting Round"}"#)).unwrap();
                                    // skip the player if they are folded or all in
                                    if player.state != player::FOLDED && player.state != player::ALL_IN {
                                        loop {
                                            let result = {
                                                // Get next message from the player's websocket
                                                let mut rx = player.rx.lock().await;
                                                match rx.next().await {
                                                    Some(res) => res,
                                                    None => continue,
                                                }
                                            };

                                            if let Ok(msg) = result {
                                                if let Ok(text) = msg.to_str() {
                                                    // Parse the incoming JSON message
                                                    let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                                                    match client_msg {
                                                        Ok(ClientMessage::Disconnect) => {
                                                            /*
                                                            Add current player into to-be-rmoved list and keep their player reference active within the players vector
                                                            
                                                            
                                                             */


                                                            // // Player disconnected entirely
                                                            // let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                                            // if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                                            //     lobby_guard.remove_lobby(lobby_name.clone()).await;
                                                            // } else {
                                                            //     lobby_guard.update_lobby_names_status(lobby_name).await;
                                                            // }
                                                            // lobby_guard.broadcast_player_count().await;
                                                            // lobby_guard.send_lobby_info().await;
                                                            // lobby_guard.send_player_list().await;
                                    
                                                            // lobby_guard.remove_player(player_name.clone()).await;
                                                            // lobby_guard.broadcast_player_count().await;
                                                            
                                                            // // Update player stats from database
                                                            // if let Err(e) = db.update_player_stats(&player).await {
                                                            //     eprintln!("Failed to update player stats: {}", e);
                                                            // }
                                                            
                                                            // return "Disconnect".to_string();
                                                        }
                                                        _ => {
                                                            // pass in the players input and validate it (check, call, raise, fold, all in)
                                                            if let Ok(action) = client_msg {
                                                                sync_player_state(&mut player, &mut lobby_guard).await;
                                                                let (valid_action, reset) = betting_round(&mut player, &mut lobby_guard, action).await;
                                                                if valid_action {
                                                                    println!("valid action");
                                                                    // update the server lobby player reference with updated clone data
                                                                    lobby_guard.update_player_reference(&player).await;
                                                                    if reset {
                                                                        println!("reseting turns remaining");
                                                                        // reset the turns_remaining counter if the player raised
                                                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                                                    }
                                                                    break;
                                                                }
                                                            } else {
                                                                println!("Invalid client message received BAD, they try again");
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    lobby_guard.turns_remaining -= 1;
                                    println!("player {} finish turn", player_name);
                                    if lobby_guard.check_end_game().await {
                                        lobby_guard.clear_betting().await;
                                        lobby_guard.game_state = lobby::SHOWDOWN;
                                    } else {
                                        if lobby_guard.turns_remaining == 0 {
                                            if deal_card_counter < 4 {
                                                // If there are more cards to deal (flop, turn, river), go back to DEAL_CARDS
                                                lobby_guard.game_state = lobby::DEAL_CARDS;
                                            } else {
                                                // After the river betting round, go to SHOWDOWN
                                                lobby_guard.game_state = lobby::SHOWDOWN;
                                            }
                                            lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                            lobby_guard.clear_betting().await;
                                            lobby_guard.get_next_player(true).await;
                                            println!("betting round finished, next player turn: {}", lobby_guard.current_player_turn);
                                        } else {
                                            lobby_guard.get_next_player(false).await;
                                            println!("next player turn: {}", lobby_guard.current_player_turn);
                                        }
                                    }
                                    lobby_guard.send_lobby_game_info().await;
                                    lobby_guard.send_player_list().await;
                                    
                                }
                                lobby::SHOWDOWN => {
                                    lobby_guard.broadcast("------ Showdown! ------".to_string()).await;
                                    
                                    // Store a map of original player hands before evaluation
                                    let original_hands = {
                                        let players = lobby_guard.players.lock().await;
                                        let mut hands_map = std::collections::HashMap::new();
                                        for p in players.iter() {
                                            if p.hand.len() >= 2 {
                                                hands_map.insert(p.name.clone(), p.hand.clone());
                                            }
                                        }
                                        hands_map
                                    };
                                    
                                    // Update player hands with best 5 cards from their hole cards + community cards
                                    update_players_hand(&lobby_guard).await;
                                    
                                    // Get ALL players, including folded ones
                                    let all_players = {
                                        let players = lobby_guard.players.lock().await;
                                        players.iter().map(|p| p.clone()).collect::<Vec<Player>>()
                                    };
                                    
                                    // Get community cards
                                    let community_cards = lobby_guard.community_cards.lock().await.clone();
                                    
                                    // Construct data for all hands, including folded players
                                    let mut all_hands_data = Vec::new();
                                    for player in all_players.iter() {
                                        if player.state == player::FOLDED {
                                            // Include folded players with their state and original hole cards
                                            all_hands_data.push(serde_json::json!({
                                                "playerName": player.name,
                                                "hand": original_hands.get(&player.name).unwrap_or(&Vec::new()).clone(),
                                                "state": player::FOLDED,
                                                "handName": "Folded"
                                            }));
                                        } else {
                                            // Non-folded players with their hand data
                                            let hand_type = (player.hand[0], player.hand[1], player.hand[2], player.hand[3], player.hand[4], player.hand[5]);
                                            let hand_name = hand_type_to_string(hand_type.0);
                                            
                                            all_hands_data.push(serde_json::json!({
                                                "playerName": player.name,
                                                "hand": original_hands.get(&player.name).unwrap_or(&Vec::new()).clone(),
                                                "state": player.state,
                                                "handName": hand_name,
                                                "handRank": hand_type
                                            }));
                                        }
                                    }
                                    
                                    // Send all hands and community cards data to all players
                                    let all_hands_json = serde_json::json!({
                                        "type": "showdownHands",
                                        "hands": all_hands_data,
                                        "communityCards": community_cards
                                    });
                                    lobby_guard.broadcast(all_hands_json.to_string()).await;
                                    
                                    // Determine winner(s) and award pot
                                    lobby_guard.showdown_texas().await;
                                    
                                    // Wait briefly before ending the round
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                    lobby_guard.game_state = UPDATE_DB;
                                }
                                lobby::UPDATE_DB => {
                                    // Update player stats and wallets in database
                                    let player_states = lobby_guard.get_player_names_and_status().await;
                                    
                                    // First explicitly set all players to IN_LOBBY and not ready
                                    {
                                        let mut players = lobby_guard.players.lock().await;
                                        for p in players.iter_mut() {
                                            println!("Setting player {} state to IN_LOBBY", p.name);
                                            p.state = player::IN_LOBBY;
                                            p.ready = false;
                                        }
                                    }
                                    
                                    // Update the database with player statistics
                                    sync_player_state(&mut player, &mut lobby_guard).await;
                                    lobby_guard.update_db().await;
                                    
                                    // Now reset the game for a new round
                                    lobby_guard.reset_game_for_new_round().await;
                                    
                                    
                                    // Send game end notification
                                    let end_game_msg = serde_json::json!({
                                        "gameEnd": true,
                                        "message": "Game has ended. All players need to ready up to start a new game."
                                    });
                                    lobby_guard.broadcast(end_game_msg.to_string()).await;
                                    
                                    // Reset game state to JOINABLE
                                    lobby_guard.game_state = lobby::JOINABLE;
                                    lobby_guard.send_lobby_game_info().await;
                                    lobby_guard.send_player_list().await;
                                    println!("Game ended, lobby reset to JOINABLE state");
                                    
                                    // IMPORTANT: Set the current player's state correctly before breaking
                                    player.state = player::IN_LOBBY;
                                    //print all player states in lobby
                                    println!("All player states in lobby after game end:");
                                    {
                                        let players = lobby_guard.players.lock().await;
                                        for p in players.iter() {
                                            println!("Player {} state: {}", p.name, p.state);
                                        }
                                    }
                                    break;
                                }
                                _ => {
                                    panic!("Invalid game state: {}", lobby_guard.game_state);
                                }
                            }
                        } else {
                            drop(lobby_guard);
                        }
                    }
                    
                    // Handle other player messages during the game
                    let result = {
                        let mut rx = player.rx.lock().await;
                        match rx.next().await {
                            Some(res) => res,
                            None => continue,
                        }
                    };
                    
                    if let Ok(msg) = result {
                        if let Ok(text) = msg.to_str() {
                            let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                            match client_msg {
                                Ok(ClientMessage::Disconnect) => {
                                    // Player disconnected entirely
                                    let lobby_name = player_lobby.lock().await.name.clone();
                                    let lobby_status = player_lobby.lock().await.remove_player(player_name.clone()).await;
                                    if lobby_status == lobby::GAME_LOBBY_EMPTY {
                                        server_lobby.lock().await.remove_lobby(lobby_name.clone()).await;
                                    } else {
                                        server_lobby.lock().await.update_lobby_names_status(lobby_name).await;
                                    }
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;

                                    server_lobby.lock().await.remove_player(player_name.clone()).await;
                                    server_lobby.lock().await.broadcast_player_count().await;
                                    
                                    // Update player stats from database
                                    if let Err(e) = db.update_player_stats(&player).await {
                                        eprintln!("Failed to update player stats: {}", e);
                                    }
                                    
                                    return "Disconnect".to_string();
                                }
                                Ok(ClientMessage::ShowLobbyInfo) => {
                                    player_lobby.lock().await.send_lobby_info().await;
                                    player_lobby.lock().await.send_player_list().await;
                                }
                                _ => {
                                    continue;
                                }
                            }
                        }
                    }
                }
                
                // Reset player state to IN_LOBBY after game ends
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_hand_type_high_card() {
        let hand = vec![0, 8, 23, 29, 51]; // Ace Hearts 9 Hearts Jack Diamond 4 Spade King Club
        let result = get_hand_type(&hand);
        assert_eq!(result, (0, 13, 12, 10, 8, 3));
    }

    #[test]
    fn test_get_hand_type_one_pair() {
        let hand = vec![2, 15, 32, 48, 18]; // 3 Hearts, 3 Diamond, 7 Spade, 10 Club, 6 Diamond
        let result = get_hand_type(&hand);
        assert_eq!(result, (1, 2, 9, 6, 5, 0)); // One pair of 3s, followed by high cards in descending order
    }
    #[test]
    fn test_get_hand_type_two_pair() {
        let hand = vec![2, 15, 32, 45, 18]; // 3 Hearts, 3 Diamond, 7 Spade, 7 Club, 6 Diamond
        let result = get_hand_type(&hand);
        assert_eq!(result, (2, 6, 2, 5, 0, 0)); // Two pair of 3s and 7s
    }
    #[test]
    fn test_get_hand_type_three_of_a_kind() {
        let hand = vec![2, 15, 28, 45, 18]; // 3 Hearts, 3 Diamond, 3 Spade, 7 Club, 6 Diamond
        let result = get_hand_type(&hand);
        assert_eq!(result, (3, 2, 6, 5, 0, 0)); // Three of a kind of 3s
    }

    #[test]
    fn test_get_hand_type_straight() {
        let hand = vec![2, 16, 30, 44, 19]; // 3 Hearts, 4 Diamond, 5 Spade, 6 Club, 7 Diamond
        let result = get_hand_type(&hand);
        assert_eq!(result, (4, 6, 0, 0, 0, 0)); // Straight from 3 to 7
    }

    #[test]
    fn test_get_hand_type_flush() {
        let hand = vec![6, 1, 2, 3, 4]; // 7 Hearts, 2 Hearts, 3 Hearts, 4 Hearts, 5 Hearts
        let result = get_hand_type(&hand);
        assert_eq!(result, (5, 6, 4, 3, 2, 1)); // Flush with Ace high
    }

    #[test]
    fn test_get_hand_type_full_house() {
        let hand = vec![2, 15, 28, 45, 19]; // 3 Hearts, 3 Diamond, 3 Spade, 7 Club, 7 Diamond
        let result = get_hand_type(&hand);
        assert_eq!(result, (6, 2, 2, 0, 0, 0)); // Full house with three of a kind and a pair
    }

    #[test]
    fn test_get_hand_type_four_of_a_kind() {
        let hand = vec![2, 15, 28, 41, 19]; // 3 Hearts, 3 Diamond, 3 Spade, 3 Club, 7 Diamond
        let result = get_hand_type(&hand);
        assert_eq!(result, (7, 2, 6, 0, 0, 0)); // Four of a kind with three of a kind and a pair
    }

    #[test]
    fn test_get_hand_type_straight_flush() {
        let hand = vec![2, 3, 4, 5, 6]; // 3 Hearts, 4 Hearts, 5 Hearts, 6 Hearts, 7 Hearts
        let result = get_hand_type(&hand);
        assert_eq!(result, (8, 6, 6, 0, 0, 0)); // Straight flush from 3 to 7
    }

    

}