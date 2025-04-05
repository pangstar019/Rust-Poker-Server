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
                player.games_played += 1; // if dealt cards then they played
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
    let current_max_bet = lobby.current_max_bet;
    match action {
        ClientMessage::Check => {
            println!("{}: check command received", player.name);
            // only check when there is no bet to call
            if player.current_bet == lobby.current_max_bet && lobby.current_max_bet == 0 {
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
    assert!(hand.len() == 7);
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
    assert!(hand.len() == 5);

    let mut ranks: Vec<i32> = hand
        .iter()
        .map(|&card| if card % 13 != 0 { card % 13 } else { 13 })
        .collect();
    ranks.sort();

    let suits: Vec<i32> = hand.iter().map(|&card| card / 13).collect();

    // Check for flush
    let flush = suits.iter().all(|&suit| suit == suits[0]);

    // Check for straight
    let straight = ranks.windows(2).all(|w| w[1] == w[0] + 1);

    if flush && straight {
        return (8, ranks[4], ranks[4], 0, 0, 0);
    }

    // Check for four of a kind
    for i in 0..2 {
        if ranks[i] == ranks[i + 1] && ranks[i] == ranks[i + 2] && ranks[i] == ranks[i + 3] {
            return if i == 0 {
                (7, ranks[i], ranks[4], 0, 0, 0)
            } else {
                (7, ranks[i], ranks[0], 0, 0, 0)
            };
        }
    }

    // Improved full house detection
    if (ranks[0] == ranks[1] && ranks[2] == ranks[3] && ranks[2] == ranks[4])
        || (ranks[0] == ranks[1] && ranks[1] == ranks[2] && ranks[3] == ranks[4])
    {
        return (6, ranks[2], ranks[0], 0, 0, 0); // Full house
    }

    if flush {
        return (5, ranks[4], ranks[3], ranks[2], ranks[1], ranks[0]);
    }

    if straight {
        return (4, ranks[4], 0, 0, 0, 0);
    }

    // Check 3 of a kind
    for i in 0..3 {
        if ranks[i] == ranks[i + 1] && ranks[i] == ranks[i + 2] {
            return match i {
                0 => (3, ranks[i], ranks[4], ranks[3], 0, 0),
                1 => (3, ranks[i], ranks[4], ranks[0], 0, 0),
                2 => (3, ranks[i], ranks[1], ranks[0], 0, 0),
                _ => unreachable!(),
            };
        }
    }

    // Check two pair
    if ranks[0] == ranks[1] && ranks[2] == ranks[3] {
        return (2, ranks[0].max(ranks[2]), ranks[0].min(ranks[2]), ranks[4], 0, 0);
    } else if ranks[0] == ranks[1] && ranks[3] == ranks[4] {
        return (2, ranks[0].max(ranks[3]), ranks[0].min(ranks[3]), ranks[2], 0, 0);
    } else if ranks[1] == ranks[2] && ranks[3] == ranks[4] {
        return (2, ranks[1].max(ranks[3]), ranks[1].min(ranks[3]), ranks[0], 0, 0);
    }

    // Check one pair
    for i in 0..4 {
        if ranks[i] == ranks[i + 1] {
            return match i {
                0 => (1, ranks[i], ranks[4], ranks[3], ranks[2], 0),
                1 => (1, ranks[i], ranks[4], ranks[3], ranks[0], 0),
                2 => (1, ranks[i], ranks[4], ranks[1], ranks[0], 0),
                3 => (1, ranks[i], ranks[2], ranks[1], ranks[0], 0),
                _ => unreachable!(),
            };
        }
    }

    // High card
    (0, ranks[4], ranks[3], ranks[2], ranks[1], ranks[0])
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
        // println!("Player hand before update {} hand: {:?}", player.name, player.hand);
        let player_hand = if lobby.game_type == TEXAS_HOLD_EM {
            let community_cards = lobby.community_cards.lock().await.clone();
            [player.hand.clone(), community_cards].concat() // make 7 cards
        } else {
            player.hand.clone()
        };
        
        // let mut translated_cards: String = Default::default();
        // for (count, card) in player.hand.iter().enumerate() {
        //     translated_cards.push_str(&format!("{}. ", count + 1));
        //     translated_cards.push_str(translate_card(*card).await.as_str());
        //     translated_cards.push_str("\n");
        // }
        // println!("Player {} hand:\n{}", player.name, translated_cards.trim_end_matches(", "));
        
        let best_hand = get_best_hand(&player_hand);
        player.hand = vec![best_hand.0, best_hand.1, best_hand.2, best_hand.3, best_hand.4, best_hand.5];
        // println!("Player hand updated {} hand: {:?}", player.name, player.hand);
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
    names.push(blind_player.name.clone());
    println!("smal blind player current bet: {}", blind_player.current_bet);

    let blind_player = &mut players[big_blind_player_i as usize];
    blind_player.wallet -= big_blind;
    blind_player.current_bet += big_blind;
    names.push(blind_player.name.clone());
    println!("big blind player current bet: {}", blind_player.current_bet);


    lobby.pot += small_blind;
    lobby.pot += big_blind;

    lobby.current_max_bet = big_blind;


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
                let stats = db.player_stats(&player_name).await;
                if let Ok(stats) = stats {
                    player.wallet = stats.wallet;
                    player.games_played = 0;
                    player.games_won = 0;
                } else {
                    tx.send(Message::text(r#"{"error": "Failed to retrieve wallet"}"#)).unwrap();
                    // add player to be deleted, then kick to server
                }
                loop {
                    if let Ok(mut lobby_guard) = player_lobby.try_lock(){
                        if lobby_guard.current_player_turn == player_name{
                            match  lobby_guard.game_state {
                                lobby::JOINABLE => {
                                    continue;
                                }
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
                                            if lobby_guard.game_state == lobby::FIRST_BETTING_ROUND {
                                                lobby_guard.game_state = lobby::DRAW;
                                            } else if lobby_guard.game_state == lobby::SECOND_BETTING_ROUND {
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
                                lobby::DRAW => {
                                    tx.send(Message::text(r#"{"message": "Drawing Round"}"#)).unwrap();
                                    player.current_bet = 0; // reset attribute from betting round
                                    
                                    // Check if current player isn't folded
                                    if player.state != player::FOLDED && player.state != player::ALL_IN {
                                        // Notify player it's their turn to draw
                                        tx.send(Message::text(r#"{"message": "Your turn to draw cards."}"#)).unwrap();
                                        
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
                                    {
                                        // Display all players' hands to everyone
                                        let players = lobby_guard.players.lock().await;
                                        
                                        // Construct data for all active hands
                                        let mut all_hands_data = Vec::new();
                                        for player in players.iter() {
                                            let state = player.state.clone();
                                            
                                            all_hands_data.push(serde_json::json!({
                                                "playerName": player.name,
                                                "state": state,
                                                "hand": player.hand,
                                            }));
                                        }
                                        // Send all hands data to all players
                                        let all_hands_json = serde_json::json!({
                                            "type": "showdownHands",
                                            "hands": all_hands_data
                                        });
                                        lobby_guard.broadcast(all_hands_json.to_string()).await;
                                    }
                                    
                                    // Determine winner(s) and award pot
                                    lobby_guard.showdown().await;
                                    
                                    // Wait briefly before ending the round
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                    lobby_guard.game_state = lobby::UPDATE_DB;
                                }
                                lobby::UPDATE_DB => {
                                    // Update player stats and wallets in database
                                    tx.send(Message::text(r#"{"message": "Game Ended"}"#)).unwrap();

                                    let player_states = lobby_guard.get_player_names_and_status().await;
                                    for (player_name, _) in player_states {
                                        // Reset player ready status
                                        lobby_guard.set_player_ready(&player_name, false).await;
                                        // Reset player state to IN_LOBBY
                                        lobby_guard.update_player_state(&player_name, player::IN_LOBBY).await;
                                    }
                                    
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

                                /*
                                Need to integrate with game logic so the indexing will work
                                also need to update DB for player in-game wallet/games_played
                                 */
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
// pub async fn seven_card_game_state_machine(server_lobby: Arc<Mutex<Lobby>>, mut player: Player, db: Arc<Database>) -> String {
//     // Same structure updates as Texas Hold'em, adding check_for_spectators
//     let mut betting_round_count = 1;
//     let mut deal_card_counter = 1;
//     loop {
//         match lobby.game_state {
//             START_OF_ROUND => {
//                 lobby.first_betting_player =(lobby.first_betting_player + 1) % lobby.current_player_count;
//                 lobby.game_state = DEAL_CARDS;
//             }
//             DEAL_CARDS => {
//                 lobby.broadcast("Dealing cards...".to_string()).await;
//                 if deal_card_counter == 1 {
//                     lobby.deck.shuffle(); // shuffle card deck
//                     deal_cards_7(lobby, deal_card_counter).await; // deal first 2 face down then deal 3rd face up
//                     deal_card_counter += 1;
//                     lobby.game_state = BRING_IN;
//                     continue;
//                 }
//                 deal_cards_7(lobby, deal_card_counter).await; // deal 1 face up card
//                 if deal_card_counter != 5 {
//                     deal_card_counter += 1;
//                 }
//                 lobby.game_state = BETTING_ROUND;
//             }
//             BRING_IN => {
//                 lobby.broadcast("Bring In stage".to_string()).await;
//                 bring_in(lobby).await;
//                 lobby.game_state = BETTING_ROUND;
//             }
//             BETTING_ROUND => {
//                 lobby.broadcast(format!("------Betting round {}!------", betting_round_count)).await;
//                 // betting_round(lobby).await;
                
                
//                 if lobby.game_state == SHOWDOWN {
//                     continue;
//                 } else if betting_round_count == 5 {
//                     lobby.game_state = SHOWDOWN;
//                     continue;
//                 } else {
//                     lobby.game_state = DEAL_CARDS;
//                     lobby.broadcast(format!("Betting round {} complete!\nCurrent pot: {}", betting_round_count, lobby.pot)).await;
//                 }
//                 betting_round_count += 1;
//             }
//             SHOWDOWN => {
//                 lobby.broadcast("------Showdown Round!------".to_string()).await;
//                 get_rid_of_x(&lobby).await;
//                 update_players_hand(lobby).await; // first update the players hands
//                 lobby.showdown().await; // then should be able to call the same showdown function
//                 lobby.game_state = END_OF_ROUND;
//             }
//             END_OF_ROUND => {
//                 lobby.game_state = UPDATE_DB;
//             }
//             UPDATE_DB => {
//                 lobby.pot = 0;
//                 lobby.update_db().await;
//                 lobby.reset_ready().await;
//                 break;
//             }
//             _ => {
//                 panic!("Invalid game state: {}", lobby.game_state);
//             }
//         }
//     }
//     return "".to_string();
// }

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
                                    lobby_guard.game_state = lobby::SMALL_AND_BIG_BLIND;
                                    
                                    // Initialize turns counter for tracking player actions
                                    lobby_guard.turns_remaining = lobby_guard.current_player_count;
                                    lobby_guard.send_lobby_game_info().await;
                                }
                                lobby::SMALL_AND_BIG_BLIND => {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                    lobby_guard.broadcast("Adding Small and Big Blind".to_string()).await;
                                    blinds(&mut lobby_guard).await;
                                    
                                    // Update the first betting player to be after the big blind
                                    lobby_guard.first_betting_player = (lobby_guard.first_betting_player + 3) % lobby_guard.current_player_count;
                                    lobby_guard.get_next_player(true).await;
                                    
                                    lobby_guard.game_state = DEAL_CARDS;
                                    lobby_guard.send_lobby_game_info().await;
                                }
                                lobby::DEAL_CARDS => {
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
                                        
                                        // Reset betting for river betting round
                                        lobby_guard.reset_current_bets().await;
                                        lobby_guard.game_state = BETTING_ROUND;
                                        lobby_guard.turns_remaining = lobby_guard.current_player_count;
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
                                            if lobby_guard.game_state == lobby::FIRST_BETTING_ROUND {
                                                lobby_guard.game_state = lobby::DRAW;
                                            } else if lobby_guard.game_state == lobby::SECOND_BETTING_ROUND {
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
                                    
                                    // lobby_guard.broadcast(format!("------ Betting round {} ------", betting_round_count).to_string()).await;
                                    
                                    // if player.state != player::FOLDED && player.state != player::ALL_IN {
                                    //     // Handle betting for this player
                                    //     loop {
                                    //         let result = {
                                    //             let mut rx = player.rx.lock().await;
                                    //             match rx.next().await {
                                    //                 Some(res) => res,
                                    //                 None => continue,
                                    //             }
                                    //         };

                                    //         if let Ok(msg) = result {
                                    //             if let Ok(text) = msg.to_str() {
                                    //                 let client_msg: JsonResult<ClientMessage> = serde_json::from_str(text);
                                                    
                                    //                 match client_msg {
                                    //                     Ok(ClientMessage::Disconnect) => {
                                    //                         // Handle player disconnect during game
                                    //                         player.state = player::FOLDED;
                                    //                         lobby_guard.update_player_state(&player_name, player::FOLDED).await;
                                    //                         lobby_guard.broadcast(format!("{} disconnected and folded", player_name)).await;
                                    //                         break;
                                    //                     }
                                    //                     Ok(valid_action) => {
                                    //                         let prev_player_bet = player.current_bet.clone();
                                                            
                                    //                         // Process betting action
                                    //                         if betting_round_texas(&mut player, lobby_guard.current_max_bet.clone(), valid_action).await {
                                    //                             // Update player state in the lobby
                                    //                             lobby_guard.players.lock().await[lobby_guard.current_player_index as usize].current_bet = player.current_bet;
                                    //                             lobby_guard.players.lock().await[lobby_guard.current_player_index as usize].state = player.state;
                                    //                             lobby_guard.players.lock().await[lobby_guard.current_player_index as usize].wallet = player.wallet;
                                                                
                                    //                             // Handle different player actions
                                    //                             match player.state {
                                    //                                 player::CHECKED | player::CALLED | player::FOLDED => {
                                    //                                     lobby_guard.turns_remaining -= 1;
                                    //                                     if player.state == player::CALLED {
                                    //                                         lobby_guard.pot += player.current_bet - prev_player_bet;
                                    //                                     }
                                    //                                 }
                                    //                                 player::ALL_IN => {
                                    //                                     if player.current_bet > lobby_guard.current_max_bet {
                                    //                                         lobby_guard.current_max_bet = player.current_bet.clone();
                                    //                                         lobby_guard.turns_remaining = lobby_guard.current_player_count - 1;
                                    //                                     } else {
                                    //                                         lobby_guard.turns_remaining -= 1;
                                    //                                     }
                                    //                                     lobby_guard.pot += player.current_bet - prev_player_bet;
                                    //                                 }
                                    //                                 player::RAISED => {
                                    //                                     lobby_guard.current_max_bet = player.current_bet.clone();
                                    //                                     lobby_guard.pot += player.current_bet - prev_player_bet;
                                                                        
                                    //                                     // Reset turns counter when someone raises
                                    //                                     lobby_guard.turns_remaining = lobby_guard.current_player_count - 1;
                                    //                                 }
                                    //                                 _ => {}
                                    //                             }
                                                                
                                    //                             // Check for round completion
                                    //                             if lobby_guard.turns_remaining == 0 {
                                    //                                 // End of betting round
                                    //                                 if betting_round_count == 4 {
                                    //                                     // After river betting - go to showdown
                                    //                                     lobby_guard.game_state = SHOWDOWN;
                                    //                                 } else {
                                    //                                     // Move to next dealing phase
                                    //                                     lobby_guard.game_state = DEAL_CARDS;
                                    //                                     betting_round_count += 1;
                                    //                                 }
                                    //                                 lobby_guard.get_next_player(true).await;
                                    //                             } else {
                                    //                                 // Move to next player
                                    //                                 lobby_guard.get_next_player(false).await;
                                    //                             }
                                                                
                                    //                             // Send updated game state to all players
                                    //                             lobby_guard.send_lobby_game_info().await;
                                    //                             lobby_guard.send_player_list().await;
                                                                
                                    //                             // Check if all but one player folded
                                    //                             let mut folded_count = 0;
                                    //                             for player in lobby_guard.players.lock().await.iter() {
                                    //                                 if player.state == player::FOLDED {
                                    //                                     folded_count += 1;
                                    //                                 }
                                    //                             }
                                    //                             if folded_count == lobby_guard.current_player_count - 1 {
                                    //                                 // If only one player remains, go to showdown
                                    //                                 lobby_guard.game_state = SHOWDOWN;
                                    //                                 lobby_guard.send_lobby_game_info().await;
                                    //                             }
                                                                
                                    //                             break;
                                    //                         }
                                    //                     }
                                    //                     _ => {
                                    //                         // Invalid action
                                    //                         tx.send(Message::text(r#"{"error": "Invalid action"}"#)).unwrap();
                                    //                     }
                                    //                 }
                                    //             }
                                    //         }
                                    //     }
                                    // } else {
                                    //     // Skip folded or all-in players
                                    //     lobby_guard.turns_remaining -= 1;
                                        
                                    //     if lobby_guard.turns_remaining == 0 {
                                    //         // End of betting round
                                    //         if betting_round_count == 4 {
                                    //             lobby_guard.game_state = SHOWDOWN;
                                    //         } else {
                                    //             lobby_guard.game_state = DEAL_CARDS;
                                    //             betting_round_count += 1;
                                    //         }
                                    //         lobby_guard.get_next_player(true).await;
                                    //     } else {
                                    //         lobby_guard.get_next_player(false).await;
                                    //     }
                                        
                                    //     lobby_guard.send_lobby_game_info().await;
                                    //     lobby_guard.send_player_list().await;
                                    // }
                                }
                                lobby::SHOWDOWN => {
                                    lobby_guard.broadcast("------ Showdown! ------".to_string()).await;
                                    
                                    // Update player hands with best 5 cards from their hole cards + community cards
                                    update_players_hand(&lobby_guard).await;
                                    
                                    // Display all players' hands
                                    let active_players = {
                                        let players = lobby_guard.players.lock().await;
                                        players.iter()
                                            .filter(|p| p.state != player::FOLDED)
                                            .map(|p| p.clone())
                                            .collect::<Vec<Player>>()
                                    };
                                    
                                    // Get community cards
                                    let community_cards = lobby_guard.community_cards.lock().await.clone();
                                    
                                    // Construct data for all active hands
                                    let mut all_hands_data = Vec::new();
                                    for player in active_players.iter() {
                                        let hand_type = get_hand_type(&player.hand[1..6]);
                                        let hand_name = hand_type_to_string(hand_type.0);
                                        
                                        all_hands_data.push(serde_json::json!({
                                            "playerName": player.name,
                                            "hand": player.hand[1..3].to_vec(),  // Only include hole cards
                                            "handName": hand_name,
                                            "handRank": hand_type
                                        }));
                                    }
                                    
                                    // Send all hands and community cards data to all players
                                    let all_hands_json = serde_json::json!({
                                        "type": "showdownHands",
                                        "hands": all_hands_data,
                                        "communityCards": community_cards
                                    });
                                    lobby_guard.broadcast(all_hands_json.to_string()).await;
                                    
                                    // Determine winner(s) and award pot
                                    lobby_guard.showdown().await;
                                    
                                    // Wait briefly before ending the round
                                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                                    lobby_guard.game_state = UPDATE_DB;
                                }
                                lobby::UPDATE_DB => {
                                    // Update player stats and wallets in database
                                    let player_states = lobby_guard.get_player_names_and_status().await;
                                    for (player_name, _) in player_states {
                                        // Reset player ready status
                                        lobby_guard.set_player_ready(&player_name, false).await;
                                        // Reset player state to IN_LOBBY
                                        lobby_guard.update_player_state(&player_name, player::IN_LOBBY).await;
                                    }
                                    
                                    // Reset game state and prepare for a new game
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
                player.state = player::IN_LOBBY;
                player_lobby.lock().await.update_player_state(&player_name, player::IN_LOBBY).await;
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
