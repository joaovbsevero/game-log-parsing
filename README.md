# Game Log Parser

This project parses Quake 3 Arena server log files to extract and analyze game events, player statistics, and match outcomes. It is written in Rust and designed for performance and reliability.

## Features
- Parses game log files (e.g., `resources/qgames.log.txt`).
- Extracts player connections, kills, item pickups, and match results.
- Summarizes scores, kill types, and player actions.
- Handles multiple matches and players.

## Log File Format
The log file (`resources/qgames.log.txt`) contains raw server output from Quake 3 Arena matches. Key lines include:
- `InitGame`: Start of a new match.
- `ClientConnect` / `ClientUserinfoChanged`: Player joins or updates info.
- `Item`: Player picks up an item or weapon.
- `Kill`: Kill event, showing killer, victim, and method.
- `Exit`: End of match (timelimit or fraglimit).
- `ShutdownGame`: Server shutdown after match.

## Rules & Event Parsing
- **Players**: Identified by client numbers and names.
- **Kills**: Tracked by killer, victim, and method (e.g., MOD_ROCKET_SPLASH, MOD_TRIGGER_HURT).
- **Items**: Weapons, armor, health, and powerups are tracked per player.
- **Match End**: Triggered by `Exit` (timelimit/fraglimit) or `ShutdownGame`.
- **Scoreboard**: Final scores are parsed from lines like `score: <score> ping: <ping> client: <client> <name>`.

## How to Run
1. **Build the project:**
	```sh
	cargo build --release
	```
2. **Run the parser:**
	```sh
	cargo run --release
	```
	By default, it will look for the log file at `resources/qgames.log.txt`.
3. **Custom log file:**
	You can specify a different log file path as an argument:
	```sh
	cargo run --release -- <path/to/logfile.txt>
	```

## Output
The parser will print a summary of matches, player statistics, kill counts, and other relevant information to the console.

## Requirements
- Rust (https://rust-lang.org)
- A valid Quake 3 Arena log file (see `resources/qgames.log.txt` for an example)

## Example
```
score: 20  ping: 4  client: 2 Oootsimo
score: 16  ping: 31  client: 7 Assasinu Credi
score: 12  ping: 2  client: 3 Isgalamido
...etc...
```
