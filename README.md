# Poker WebSocket Server

A real-time, multiplayer poker server built with Rust and WebSockets supporting 5 Card Draw, 7 Card Stud, and Texas Hold'em.

---

## Features
- **Three Poker Variants:**
  - 5 Card Draw
  - 7 Card Stud
  - Texas Hold'em
- **Real-Time Multiplayer:** WebSocket-based real-time communication.
- **Lobby System:** Create, join, and manage private or public poker lobbies.
- **Persistent Player Accounts:** Track player stats, chip balances, and game history with SQLite.
- **Robust Game Logic:** Automated dealing, betting, and hand evaluation.

---

## Technology Stack
- **Rust** (Tokio async runtime)
- **Warp Web Framework** (WebSocket support)
- **SQLite** (via `sqlx` for persistent storage)
- **JSON Messaging Protocol**

---

## Getting Started

### Installation

Clone the repository:
```bash
git clone https://github.com/" "/poker-ws-server.git
cd src
```

Build the project:
```bash
cargo build --release
```

### Running the Server

Launch the WebSocket server:
```bash
cargo run 
```

Server starts at:
```
ws://localhost:1112
```

---

## How to Play

Use any WebSocket-compatible client (e.g., browser app or CLI tool like `wscat`) to connect:
```bash
wscat -c ws://localhost:1112
```


### Poker Variants

- **5 Card Draw**: Classic draw poker with one discard phase.
- **7 Card Stud**: Seven-card poker, with mixed face-up/down dealing.
- **Texas Hold'em**: Popular community card poker game.


## License

Distributed under the **MIT License**. See `LICENSE` for details.

---

Enjoy the game!


