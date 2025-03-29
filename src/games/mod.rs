use super::*;
use crate::Deck;
use crate::lobby::Lobby;
use crate::Player;
use futures::future::join_all;
use futures_util::future::{ready, Join};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::{mpsc, mpsc::UnboundedSender, Mutex};
use warp::{filters::ws::WebSocket, ws::Message};


// Lobby attribute definitions
pub const MAX_PLAYER_COUNT: i32 = 5;
const EMPTY: i32 = -1;
pub const JOINABLE: i32 = 0;
pub const START_OF_ROUND: i32 = 1;
const ANTE: i32 = 2;
const SMALL_AND_BIG_BLIND: i32 = 19;
const BIG_BLIND: i32 = 20;
const DEAL_COMMUNITY_CARDS: i32 = 21;
const DEAL_CARDS: i32 = 3;
const FIRST_BETTING_ROUND: i32 = 14;
const SECOND_BETTING_ROUND: i32 = 15;
const BETTING_ROUND: i32 = 16;
const DRAW: i32 = 5;
const BRING_IN: i32 = 50;

const SHOWDOWN: i32 = 7;
const END_OF_ROUND: i32 = 8;
const UPDATE_DB: i32 = 9;
const TURN_ROUND: i32 = 10;
const FLOP_ROUND: i32 = 11;
const RIVER_ROUND: i32 = 12;

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

pub async fn deal_cards(lobby: &mut Lobby) {
    match lobby.game_type {
        FIVE_CARD_DRAW => deal_cards_5(lobby).await,
        //needs to implement the round checks
        SEVEN_CARD_STUD => deal_cards_7(lobby, 0).await,
        // TEXAS_HOLD_EM => deal_cards_texas(lobby).await,
        _ => println!("Invalid game type."),
    }
}

async fn deal_cards_7(lobby: &mut Lobby, round: usize) {
    let mut players = lobby.players.lock().await;
    let mut count = 0;
    for player in players.iter_mut() {
        count = 0;
        loop {
            if player.state != FOLDED {
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
    let active_players: Vec<_> = players.iter().filter(|p| p.state != FOLDED).collect();
    let players_tx: Vec<_> = active_players.iter().map(|p| p.tx.clone()).collect();
    let players_hands: Vec<_> = active_players.iter().map(|p| p.hand.clone()).collect();
    display_hand(players_tx, players_hands).await;
}

// async fn deal_cards_texas(lobby: &mut Lobby, round: usize) {
//     let mut players = lobby.players.lock().await;
//     if round == 1 {
//         for player in players.iter_mut() {
//             loop {
//                 if player.state != FOLDED {
//                     // deal player cards
//                     let card1 = lobby.deck.deal();
//                     let card2 = lobby.deck.deal();
//                     player.hand.push(card1);
//                     player.hand.push(card2);
//                     break;
//                 }
//             }
//         }
//     }
//     // Get active players
//     let active_players: Vec<_> = players.iter().filter(|p| p.state != FOLDED).collect();
//     let players_tx: Vec<_> = active_players.iter().map(|p| p.tx.clone()).collect();
//     let players_hands: Vec<_> = active_players.iter().map(|p| p.hand.clone()).collect();
//     display_hand(players_tx, players_hands).await;
// }

async fn deal_cards_5(lobby: &mut Lobby) {
    let mut players = lobby.players.lock().await;
    for _ in 0..5 {
        for player in players.iter_mut() {
            if player.state != FOLDED {
                player.hand.push(lobby.deck.deal());
            }
        }
    }
    // print the hands to the players
    let players_tx = players.iter().filter(|p| p.state != FOLDED).map(|p| p.tx.clone()).collect::<Vec<_>>(); // get all tx's
    let players_hands = players.iter().filter(|p| p.state != FOLDED).map(|p| p.hand.clone()).collect::<Vec<_>>(); // get all hands
    display_hand(players_tx.clone(), players_hands.clone()).await;
}

pub async fn deal_cards_texas(lobby: &mut Lobby, round: usize) {
    let mut community_cards = lobby.community_cards.lock().await;
    let mut players = lobby.players.lock().await;
    let players_tx = players.iter().filter(|p| p.state != FOLDED).map(|p| p.tx.clone()).collect::<Vec<_>>();
    match round {
        1 => {
            for player in players.iter_mut() {
                if player.state != FOLDED {
                    // deal 2 cards to each player
                    player.hand.push(lobby.deck.deal());
                    player.hand.push(lobby.deck.deal());
                }
                player.games_played += 1; // if dealt cards then they played
            }
            let players_hands = players.iter().filter(|p| p.state != FOLDED).map(|p| p.hand.clone()).collect::<Vec<_>>(); // get all hands
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
    let players_tx = players.iter().filter(|p| p.state != FOLDED).map(|p| p.tx.clone()).collect::<Vec<_>>();
    let mut message = String::from("Community cards:\n");
    for (i, card) in community_cards.iter().enumerate() {
        message.push_str(&format!("{}. {}\n", i + 1, translate_card(*card).await));
    }
    lobby.lobby_wide_send(players_tx.clone(), message).await;
}


pub async fn ante(lobby: &mut Lobby) {
    let mut players = lobby.players.lock().await;
    for player in players.iter_mut() {
        if player.wallet > 10 {
            println!("player {} reached here -------",player.name);
            println!("Player {} antes 10.", player.name);
            lobby.pot += 10;
            player.wallet -= 10;
            player.games_played += 1;
        } else {
            player.state = FOLDED; // these guys cant play, spectator basically
        }
    }
    return;
}

//This is the bring in bet for seven card draw and the rule for this is
//The player with the lowest-ranking up-card pays the bring-in, and betting proceeds after that in normal clockwise order
// and to break ties in card ranks we will use the suit order of spades, hearts, diamonds, and clubs
pub async fn bring_in(lobby: &mut Lobby) {
    let mut players = lobby.players.lock().await;
    let mut lowest_up_card = 14;
    let mut lowest_up_card_player = 0;
    for (i, player) in players.iter().enumerate() {
        if player.state != FOLDED {
            if player.hand[2] % 13 < lowest_up_card {
                lowest_up_card = player.hand[2] % 13;
                lowest_up_card_player = i;
            }
        }
    }
    let bring_in = 10;
    players[lowest_up_card_player].wallet -= bring_in;
    players[lowest_up_card_player].current_bet += bring_in;
    lobby.pot += bring_in;
    players[lowest_up_card_player].state = CALLED;
    let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    lobby.lobby_wide_send(players_tx, format!("{} has the lowest up card and pays the bring-in of {}", players[lowest_up_card_player].name, bring_in)).await;

}

pub async fn betting_round(lobby: &mut Lobby) {
    let mut players = lobby.players.lock().await;
    if players.len() == 1 {
        // only one player left, move on
        return;
    }
    let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    // ensure all players have current_bet set to 0

    // let mut current_player_index = if lobby.game_type == TEXAS_HOLD_EM {
    //     (lobby.first_betting_player + 2) % lobby.current_player_count
    // } else {
    //     lobby.first_betting_player
    // };
    let mut current_player_index = lobby.first_betting_player;

    let mut current_lobby_bet = 0; // resets to 0 every betting round
    let mut players_remaining = lobby.current_player_count;
    let mut folded_count = 0;
    let mut all_folded = false;

    
    for player in players.iter_mut() {
        if player.state == FOLDED {
            folded_count += 1;
        }
    }
    // so players cant check after blinds
    if lobby.pot == 15 && lobby.game_type == TEXAS_HOLD_EM {
        current_lobby_bet = lobby.pot;
    }
    else{
        // only reset their current bet if its not the first betting round of texas holdem
        for player in players.iter_mut() {
            player.current_bet = 0; // reset all players to 0
        }
    }

    while players_remaining > 0 {
        println!("Current player index: {}", current_player_index);
        let player = &mut players[current_player_index as usize];
        if player.state == FOLDED || player.state == ALL_IN {
            current_player_index = (current_player_index + 1) % lobby.current_player_count;
            players_remaining -= 1;
            continue;
        }
        let message = format!(
                "Choose an option:\n1. Check\n2. Raise\n3. Call\n4. Fold\n5. All-in\n6. Display Hand\n\nYour amount to call: {}\nCurrent Pot: {}\nCurrent Wallet: {}",
                (current_lobby_bet - player.current_bet), lobby.pot, player.wallet
            );
        let _ = player.tx.send(Message::text(message));
        println!("Player {} current bet {}", player.name, player.current_bet);
        loop {
            let choice = player.get_player_input().await;
            // println!("player input: {}", choice.clone());

            match choice.as_str() {
                "1" => {
                    if current_lobby_bet == 0 {
                        player.state = CHECKED;
                        println!("checked");
                        // lobby.broadcast(format!("{} has checked.", player.name)).await;
                        lobby
                            .lobby_wide_send(
                                players_tx.clone(),
                                format!("{} has checked.", player.name),
                            )
                            .await;
                        players_remaining -= 1; // on valid moves, decrement the amount of players to make a move
                        break;
                    } else {
                        player
                            .tx
                            .send(Message::text(
                                "Invalid move: You can't check, there's a bet to call.",
                            ))
                            .ok();
                    }
                }
                "2" => {
                    // if player.state == RAISED {
                    //     player.tx.send(Message::text("You've already raised this round.\nCall or fold.",)).ok();
                    //     continue;
                    // }

                    let bet_diff = current_lobby_bet - player.current_bet;
                    if current_lobby_bet > 0 {
                        if player.wallet <= (current_lobby_bet - player.current_bet) {
                            player
                                .tx
                                .send(Message::text(
                                    "Invalid move: not enough cash to raise.\nCall or fold.",
                                ))
                                .ok();
                            continue;
                        }
                        // print the minimum the player has to bet to stay in the game
                        let _ = player.tx.send(Message::text(format!(
                            "Current minimum bet is: {}",
                            bet_diff
                        )));
                    } else {
                        let _ = player.tx.send(Message::text("Bet must be greater than 0."));
                    }
                    let _ = player.tx.send(Message::text(format!(
                        "Your current bet is: {}\nYour wallet balance: {}\nEnter your bet amount:",
                        player.current_bet, player.wallet
                    )));
                    // let _ = player.tx.send(Message::text("Enter your bet amount:"));
                    loop {
                        let bet_amount = player.get_player_input().await;
                        if let Ok(bet) = bet_amount.parse::<i32>() {
                            // doesnt allow calling (all in case included) or raising if the player doesnt have enough money
                            if bet > player.wallet || bet <= bet_diff || bet <= 0 {
                                player.tx.send(Message::text("Invalid raise.")).ok();
                            } else {
                                if bet == player.wallet {
                                    player.state = ALL_IN;
                                    // lobby.broadcast(format!("{} has gone all in!", player.name)).await;
                                    lobby
                                        .lobby_wide_send(
                                            players_tx.clone(),
                                            format!("{} has gone all in!", player.name),
                                        )
                                        .await;
                                } else {
                                    player.state = RAISED;
                                }
                                player.wallet -= bet;
                                player.current_bet += bet;
                                lobby.pot += bet;
                                current_lobby_bet = player.current_bet;

                                // lobby.broadcast(format!("{} has raised the pot to: {}", player.name, player.current_bet)).await;
                                lobby
                                    .lobby_wide_send(
                                        players_tx.clone(),
                                        format!(
                                            "{} has raised the pot to: {}",
                                            player.name, lobby.pot
                                        ),
                                    )
                                    .await;
                                // reset the betting cycle so every player calls/raises the new max bet or folds
                                players_remaining = lobby.current_player_count - 1;
                                break;
                            }
                        } else {
                            player
                                .tx
                                .send(Message::text("Invalid raise : not a number."))
                                .ok();
                        }
                    }
                    break;
                }
                "3" => {
                    if current_lobby_bet == 0 {
                        player
                            .tx
                            .send(Message::text("Invalid move: no bet to call."))
                            .ok();
                        continue;
                    }

                    let call_amount = current_lobby_bet - player.current_bet;
                    print!("call amount: {}", call_amount);
                    println!("current lobby bet: {}", current_lobby_bet);

                    if call_amount > player.wallet {
                        player
                            .tx
                            .send(Message::text(
                                "Invalid move: not enough cash.\nAll in or fold!",
                            ))
                            .ok();
                    } else {
                        player.wallet -= call_amount;
                        player.current_bet += call_amount;
                        lobby.pot += call_amount;
                        player.state = CALLED;
                        // lobby.broadcast(format!("{} has called the bet.", player.name)).await;
                        lobby
                            .lobby_wide_send(
                                players_tx.clone(),
                                format!("{} has called the bet.", player.name),
                            )
                            .await;
                        players_remaining -= 1;
                        break;
                    }
                }
                "4" => {
                    player.state = FOLDED;
                    // lobby.broadcast(format!("{} has folded.", player.name)).await;
                    lobby
                        .lobby_wide_send(players_tx.clone(), format!("{} has folded.", player.name))
                        .await;
                    folded_count += 1;

                    if folded_count == lobby.current_player_count - 1 {
                        all_folded = true;
                        // lobby.lobby_wide_send(players_tx.clone(), message.clone()).await;
                        lobby.game_state = SHOWDOWN; // if only one player left they won, send to showdown to handle the pot distribution
                        println!("All but one player folded, moving to showdown.");
                    }
                    players_remaining -= 1;
                    break;
                }
                "5" => {
                    // all in
                    // side pots not considered yet
                    if player.wallet > 0 {
                        lobby.pot += player.wallet;
                        player.current_bet += player.wallet;
                        player.wallet -= player.wallet;
                        if player.current_bet > current_lobby_bet {
                            current_lobby_bet = player.current_bet;
                            // reset the betting cycle if it pot was raised
                            players_remaining = lobby.current_player_count - 1;
                        } else {
                            players_remaining -= 1;
                        }
                        player.state = ALL_IN;
                        // lobby.broadcast(format!("{} has gone all in!", player.name)).await;
                        lobby
                            .lobby_wide_send(
                                players_tx.clone(),
                                format!("{} has gone all in!", player.name),
                            )
                            .await;
                        break;
                    }
                }
                "6" => {
                    let hand = player.hand.clone();
                    let mut message = String::from("Your hand:\n");
                    let mut i = 0;
                    for card in hand.iter() {
                        let card_str = translate_card(*card).await;
                        message.push_str(&format!("{}. {}\n", i+1, card_str));
                    }
                    let _ = player.tx.send(Message::text(message));
                }
                "Disconnect" => {
                    // lobby.broadcast(format!("{} has disconnected and folded.", player.name)).await;
                    lobby
                        .lobby_wide_send(
                            players_tx.clone(),
                            format!("{} has disconnected and folded.", player.name),
                        )
                        .await;
                    player.state = FOLDED;
                    // Handle disconnection properly
                    drop(player.clone().rx);
                    break;
                }
                _ => {
                    player
                        .tx
                        .send(Message::text("Invalid action, try again."))
                        .ok();
                }
            }
        }

        if all_folded == true {
            lobby.game_state = SHOWDOWN;
            break;
        }
        // Move to next player
        current_player_index = (current_player_index + 1) % lobby.current_player_count;
        // players_remaining -= 1; // ensure we give everyone a change to do an action
    }
    // if all but one player folded, the remaining player wins the pot
}

pub async fn drawing_round(lobby: &mut Lobby) {
    //As the drawing round starts, we will check if their status is folded, if it is, we will skip them
    //else we will continue the drawing round for that player and we will display a input menu of "Stand Pat or Exchange cards"
    //if the player chooses to exchange cards, we will remove the cards from their hand and deal them new cards the logic for this will be
    //once players chooses the index of cards they want to change, we will remove those cards from their hand and deal them new cards
    //If they stand pat nothing will happen and it will move to the next player
    //Once the cards are swap we will quickly display the cards to the player only.
    //Once all players have swapped their cards, we will move to the next betting round
    let mut players = lobby.players.lock().await;
    if players.len() == 1 {
        // only one player left, move on
        return;
    }

    let players_tx = players
        .iter()
        .filter(|p| p.state != FOLDED)
        .map(|p| p.tx.clone())
        .collect::<Vec<_>>(); // get all tx's
    let players_hands = players
        .iter()
        .filter(|p| p.state != FOLDED)
        .map(|p| p.hand.clone())
        .collect::<Vec<_>>(); // get all hands

    display_hand(players_tx.clone(), players_hands.clone()).await;
    let mut current_player_index = lobby.first_betting_player;
    let mut count = 0;
    let mut player_count = 0;
    for player in players.iter_mut() {
        if player.state != FOLDED {
            player_count += 1;
        }
    }
    loop {
        let player = &mut players[current_player_index as usize];
        if player.state == FOLDED {
            current_player_index = (current_player_index + 1) % lobby.current_player_count;
            continue;
        };
        if count == player_count {
            break;
        };
        println!("Drawing round for: {}", player.name);

        player.tx.send(Message::text("Drawing round!")).ok();
        loop {
            let message = format!(
                "Choose an option:\n    1 - Stand Pat (Keep your hand)\n    2 - Exchange cards"
            );
            let _ = player.tx.send(Message::text(message));

            let input = player.get_player_input().await;
            println!("Player input for drawing round: {}", input);

            match input.as_str() {
                "1" => {
                    let _ = player.tx.send(Message::text("You chose to Stand Pat."));
                    break;
                }
                "2" => {
                    let _ = player.tx.send(Message::text("Enter the indices of the cards you want to exchange (comma-separated, e.g., '1,2,3')"));

                    loop {
                        let input = player.get_player_input().await;

                        if let Some(indices_str) = input.strip_prefix("") {
                            if !indices_str
                                .chars()
                                .all(|c| c.is_digit(10) || c == ',' || c.is_whitespace())
                            {
                                let _ = player.tx.send(Message::text("Invalid format. Use numbers separated by commas (e.g., '1,2,3')."));
                                continue;
                            }

                            let indices: Vec<usize> = indices_str
                                .split(',')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .filter_map(|s| s.parse().ok())
                                .collect();

                            let mut valid_indices: Vec<usize> = indices
                                .iter()
                                .cloned()
                                .filter(|&i| i > 0 && i <= player.hand.len()) // Ensure within bounds
                                .map(|i| i - 1) // Convert to zero-based index
                                .collect();

                            valid_indices.sort();
                            valid_indices.dedup();

                            if valid_indices.len() == indices.len() && !valid_indices.is_empty() {
                                let mut new_hand = Vec::new();
                                for (i, card) in player.hand.iter().enumerate() {
                                    if !valid_indices.contains(&i) {
                                        new_hand.push(*card);
                                    }
                                }
                                for _ in &valid_indices {
                                    new_hand.push(lobby.deck.deal());
                                }
                                player.hand = new_hand;
                                lobby
                                    .lobby_wide_send(
                                        players_tx.clone(),
                                        format!(
                                            "{} has exchanged {} cards.",
                                            player.name,
                                            valid_indices.len()
                                        ),
                                    )
                                    .await;

                                // Display the new hand to the player
                                let updated_hand = vec![player.hand.clone()];
                                display_hand(vec![player.tx.clone()], updated_hand).await;

                                break;
                            } else {
                                let _ = player.tx.send(Message::text("Invalid indices. Ensure they are within range and correctly formatted."));
                            }
                        }
                    }
                    break;
                }
                "Disconnect" => {
                    lobby
                        .lobby_wide_send(
                            players_tx.clone(),
                            format!("{} has disconnected.", player.name),
                        )
                        .await;
                    break;
                }
                _ => {
                    let _ = player
                        .tx
                        .send(Message::text("Invalid choice. Please enter 1 or 2."));
                }
            }
        }
        current_player_index = (current_player_index + 1) % lobby.current_player_count;
        count += 1;
    }
}

pub async fn showdown(lobby: &mut Lobby) {
    let mut players = lobby.players.lock().await;
    let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    let mut winning_players: Vec<Player> = Vec::new(); // keeps track of winning players at the end, accounting for draws
    let mut winning_players_names: Vec<String> = Vec::new();
    let mut winning_hand = (-1, -1, -1, -1, -1, -1); // keeps track of current highest hand, could change when incrementing between players
    let mut winning_players_indices: Vec<i32> = Vec::new();
    let mut player_hand_type: (i32, i32, i32, i32, i32, i32);
    for player in players.iter_mut() {
        if player.state == FOLDED {
            continue;
        };
        let player_hand = player.hand.clone();
        if lobby.game_type == SEVEN_CARD_STUD || lobby.game_type == TEXAS_HOLD_EM {
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
    let pot_share = lobby.pot / winning_player_count as i32;
    for i in 0..winning_player_count {
        for j in 0..players.len() {
            if players[j].name == winning_players[i].name {
                players[j].games_won += 1;
                players[j].wallet += pot_share;
                println!("Player {} wins {}!", players[j].name, pot_share);
                println!("Player {} wallet: {}", players[j].name, players[j].wallet);
            }
        }
    }
    let winner_names = winning_players_names.join(", ");
    lobby.lobby_wide_send(players_tx, format!("Winner: {}", winner_names)).await;
}

pub async fn translate_card(card: i32) -> String {
    //if card is greater than 52 during the very final round of 7 card stud which is the showdown round
    //We will do card -53 to get the actual card value else if it is not that round yet we will just display X
        // let mut card_clone = card.clone();
        
        if card > 52 {
            return "X".to_string(); // Face-down card
        }


    let mut cardStr: String = Default::default();
    let rank: i32 = card % 13;

    if rank == 0 {
        cardStr.push_str("Ace");
    } else if rank <= 9 {
        cardStr.push_str(&(rank + 1).to_string());
    } else if rank == 10 {
        cardStr.push_str("Jack");
    } else if rank == 11 {
        cardStr.push_str("Queen");
    } else if rank == 12 {
        cardStr.push_str("King");
    }

    let suit: i32 = card / 13;
    if suit == 0 {
        cardStr.push_str(" Hearts");
    } else if suit == 1 {
        cardStr.push_str(" Diamond");
    } else if suit == 2 {
        cardStr.push_str(" Spade");
    } else if suit == 3 {
        cardStr.push_str(" Club");
    }
    return cardStr;
}

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
fn get_best_hand(hand: &[i32]) -> (i32, i32, i32, i32, i32, i32) {
    assert!(hand.len() == 7);
    println!("Hand: {:?}", hand);
    let mut best_hand = (-1, -1, -1, -1, -1, -1);
    let mut best_hand_type = -1;
    for i in 0..=2 {
        for j in (i + 1)..=3 {
            for k in (j + 1)..=4 {
                for l in (k + 1)..=5 {
                    for m in (l + 1)..=6 {
                        let current_hand = vec![hand[i], hand[j], hand[k], hand[l], hand[m]];
                        let current_hand_type = get_hand_type(&current_hand);
                        if current_hand_type.0 > best_hand_type || (current_hand_type.0 == best_hand_type && current_hand_type.1 > best_hand.1)
                        {
                            best_hand = current_hand_type;
                            best_hand_type = current_hand_type.0;
                        }
                    }
                }
            }
        }
    }
    println!("Best hand: {:?}", best_hand);
    best_hand
    
}

fn get_hand_type(hand: &[i32]) -> (i32, i32, i32, i32, i32, i32) {
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
        return (
            2,
            ranks[0].max(ranks[2]),
            ranks[0].min(ranks[2]),
            ranks[4],
            0,
            0,
        );
    } else if ranks[0] == ranks[1] && ranks[3] == ranks[4] {
        return (
            2,
            ranks[0].max(ranks[3]),
            ranks[0].min(ranks[3]),
            ranks[2],
            0,
            0,
        );
    } else if ranks[1] == ranks[2] && ranks[3] == ranks[4] {
        return (
            2,
            ranks[1].max(ranks[3]),
            ranks[1].min(ranks[3]),
            ranks[0],
            0,
            0,
        );
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
pub async fn update_players_hand(lobby: &Lobby) {
    let mut players = lobby.players.lock().await;
    for player in players.iter_mut() {
        if player.state == FOLDED {
            continue;
        }
        // println!("Player hand before update {} hand: {:?}", player.name, player.hand);
        let player_hand = if lobby.game_type == TEXAS_HOLD_EM {
            let community_cards = lobby.community_cards.lock().await.clone();
            [player.hand.clone(), community_cards].concat() // make 7 cards
        } else {
            player.hand.clone()
        };
        
        let mut translated_cards: String = Default::default();
        for (count, card) in player.hand.iter().enumerate() {
            translated_cards.push_str(&format!("{}. ", count + 1));
            translated_cards.push_str(translate_card(*card).await.as_str());
            translated_cards.push_str("\n");
        }
        // println!("Player {} hand:\n{}", player.name, translated_cards.trim_end_matches(", "));
        
        let best_hand = get_best_hand(&player_hand);
        player.hand = vec![best_hand.0, best_hand.1, best_hand.2, best_hand.3, best_hand.4, best_hand.5];
        // println!("Player hand updated {} hand: {:?}", player.name, player.hand);
    }
}

pub async fn get_rid_of_x(lobby: &Lobby) {
    let mut players = lobby.players.lock().await;
    for player in players.iter_mut() {
        if player.state == FOLDED {
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


    let players_tx = players.iter().map(|p| p.tx.clone()).collect::<Vec<_>>();
    lobby.lobby_wide_send(players_tx, format!("{} has paid the small blind of {}\n{} has paid the big blind of {}", names[0], small_blind, names[1], big_blind)).await;
}

pub async fn find_next_start(lobby: &mut Lobby, dealer_index: i32) {
    let players = lobby.players.lock().await;
    let mut next_start = 0;
    for i in 1..=lobby.current_player_count {
        let index = (dealer_index + i) % lobby.current_player_count;
        if players[index as usize].state != FOLDED {
            next_start = index;
            break;
        }
    }
    lobby.first_betting_player = next_start;
}
pub async fn five_card_game_state_machine(lobby: &mut Lobby) {
    loop {
        match lobby.game_state {
            START_OF_ROUND => {
                lobby.first_betting_player =(lobby.first_betting_player + 1) % lobby.current_player_count;
                lobby.game_state = ANTE;
            }
            ANTE => {
                lobby.broadcast("Ante round!\nEveryone adds $10 to the pot.".to_string()).await;
                ante(lobby).await;
                lobby.broadcast(format!("Current pot: {}", lobby.pot)).await;
                lobby.game_state = DEAL_CARDS;
            }
            DEAL_CARDS => {
                lobby.broadcast("Dealing cards...".to_string()).await;
                lobby.deck.shuffle(); // shuffle card deck
                deal_cards(lobby).await; // deal and display each players hands to them
                lobby.game_state = FIRST_BETTING_ROUND;
            }
            FIRST_BETTING_ROUND => {
                lobby.broadcast("------First betting round!------".to_string()).await;
                betting_round(lobby).await;
                if lobby.game_state == SHOWDOWN {
                    continue;
                } else {
                    lobby.game_state = DRAW;
                    lobby.broadcast(format!("First betting round complete!\nCurrent pot: {}",lobby.pot)).await;
                }
            }
            DRAW => {
                lobby.broadcast("------Drawing round!------".to_string()).await;
                drawing_round(lobby).await;
                lobby.game_state = SECOND_BETTING_ROUND;
            }
            SECOND_BETTING_ROUND => {
                lobby.broadcast("Second betting round!".to_string()).await;
                betting_round(lobby).await;
                lobby.broadcast(format!("Second betting round complete!\nCurrent pot: {}",lobby.pot)).await;
                lobby.game_state = SHOWDOWN;
            }
            SHOWDOWN => {
                lobby.broadcast("------Showdown Round!------".to_string()).await;
                showdown(lobby).await;
                lobby.game_state = END_OF_ROUND;
            }
            END_OF_ROUND => {
                lobby.game_state = UPDATE_DB;
            }
            UPDATE_DB => {
                lobby.pot = 0;
                lobby.update_db().await;
                break;
            }
            _ => {
                panic!("Invalid game state: {}", lobby.game_state);
            }
        }
    }
}

pub async fn seven_card_game_state_machine(lobby: &mut Lobby) {
    let mut betting_round_count = 1;
    let mut deal_card_counter = 1;
    loop {
        match lobby.game_state {
            START_OF_ROUND => {
                lobby.first_betting_player =(lobby.first_betting_player + 1) % lobby.current_player_count;
                lobby.game_state = DEAL_CARDS;
            }
            DEAL_CARDS => {
                lobby.broadcast("Dealing cards...".to_string()).await;
                if deal_card_counter == 1 {
                    lobby.deck.shuffle(); // shuffle card deck
                    deal_cards_7(lobby, deal_card_counter).await; // deal first 2 face down then deal 3rd face up
                    deal_card_counter += 1;
                    lobby.game_state = BRING_IN;
                    continue;
                }
                deal_cards_7(lobby, deal_card_counter).await; // deal 1 face up card
                if deal_card_counter != 5 {
                    deal_card_counter += 1;
                }
                lobby.game_state = BETTING_ROUND;

                // for i in 1..6 {
                //     deal_cards_7(lobby, i).await;
                // }
                // lobby.game_state = SHOWDOWN;
            }
            BRING_IN => {
                lobby.broadcast("Bring In stage".to_string()).await;
                bring_in(lobby).await;
                lobby.game_state = BETTING_ROUND;
                
            }
            BETTING_ROUND => {
                lobby.broadcast(format!("------Betting round {}!------", betting_round_count)).await;
                // add fix later for right after the bring in, the next player can not check, they must call the bring in or raise i believe
                betting_round(lobby).await;
                if lobby.game_state == SHOWDOWN {
                    continue;
                } else if betting_round_count == 5 {
                    lobby.game_state = SHOWDOWN;
                    continue;
                } else {
                    lobby.game_state = DEAL_CARDS;
                    lobby.broadcast(format!("Betting round {} complete!\nCurrent pot: {}", betting_round_count, lobby.pot)).await;
                }
                betting_round_count += 1;
            }
            SHOWDOWN => {
                lobby.broadcast("------Showdown Round!------".to_string()).await;
                // change to 7 card showdown
                get_rid_of_x(&lobby).await;
                update_players_hand(lobby).await; // first update the players hands
                showdown(lobby).await; // then should be able to call the same showdown function
                lobby.game_state = END_OF_ROUND;
            }
            END_OF_ROUND => {
                lobby.game_state = UPDATE_DB;
            }
            UPDATE_DB => {
                lobby.pot = 0;
                lobby.update_db().await;
                break;
            }
            _ => {
                panic!("Invalid game state: {}", lobby.game_state);
            }
        }
    }
}

pub async fn texas_holdem_game_state_machine(lobby: &mut Lobby) {
    let mut betting_round_count = 1;
    let mut deal_card_counter = 1;
    let dealer_index = lobby.first_betting_player;
    loop {
        match lobby.game_state {
            START_OF_ROUND => {
                // lobby.first_betting_player =(lobby.first_betting_player + 1) % lobby.current_player_count;
                lobby.game_state = SMALL_AND_BIG_BLIND;
            }
            SMALL_AND_BIG_BLIND => {
                lobby.broadcast("Adding Small and Big Blind".to_string()).await;
                blinds(lobby).await;
                println!("First betting player: {}", lobby.first_betting_player);
                lobby.first_betting_player = (lobby.first_betting_player + 1) % lobby.current_player_count;
                lobby.first_betting_player = (lobby.first_betting_player + 1) % lobby.current_player_count;
                lobby.first_betting_player = (lobby.first_betting_player + 1) % lobby.current_player_count;
                lobby.game_state = DEAL_CARDS;
            }
            DEAL_CARDS => {
                lobby.broadcast("Dealing cards...".to_string()).await;
                if deal_card_counter == 1 {
                    lobby.deck.shuffle(); // shuffle card deck
                }
                deal_cards_texas(lobby, deal_card_counter).await;
                if deal_card_counter != 4 {
                    deal_card_counter += 1;
                }
                lobby.game_state = BETTING_ROUND;
            }
            BETTING_ROUND => {
                lobby.broadcast(format!("------Betting round {}!------", betting_round_count)).await;
                if betting_round_count !=1 {
                    find_next_start(lobby, dealer_index).await; // find the next player to start the betting round (left most player thats not folded)
                }
                betting_round(lobby).await;
                if lobby.game_state == SHOWDOWN {
                    continue;
                } else if betting_round_count == 4 {
                    lobby.game_state = SHOWDOWN;
                    continue;
                } else {
                    lobby.game_state = DEAL_CARDS;
                    lobby.broadcast(format!("Betting round {} complete!\nCurrent pot: {}", betting_round_count, lobby.pot)).await;
                }
                betting_round_count += 1;
            }
            SHOWDOWN => {
                lobby.broadcast("------Showdown Round!------".to_string()).await;
                // get_rid_of_x(&lobby).await;
                update_players_hand(lobby).await; // first update the players hands
                showdown(lobby).await; // then should be able to call the same showdown function
                lobby.game_state = END_OF_ROUND;
            }
            END_OF_ROUND => {
                // clear the community cards
                let mut community_cards = lobby.community_cards.lock().await;
                community_cards.clear();
                // clear players bets
                let mut players = lobby.players.lock().await;
                for player in players.iter_mut() {
                    player.current_bet = 0;
                }
                lobby.pot = 0;
                
                lobby.first_betting_player = (dealer_index+ 1) % lobby.current_player_count;
                lobby.game_state = UPDATE_DB;
            }
            UPDATE_DB => {
                lobby.update_db().await;
                break;
            }
            _ => {
                panic!("Invalid game state: {}", lobby.game_state);
            }
        }
    }
}

