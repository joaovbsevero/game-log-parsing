use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "log-parser")]
#[command(about = "A log parser for game logs")]
struct Args {
    /// Path to the log file to parse
    #[arg(value_name = "FILE")]
    log_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameEvent {
    pub timestamp: String,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    InitGame { details: String },
    ShutdownGame,
    ClientConnect { player_id: u32 },
    ClientUserinfoChanged { player_id: u32, info: String },
    ClientBegin { player_id: u32 },
    Item { item_id: u32, description: String },
    Kill {
        kill_id: u32,
        player_id: u32,
        victim_id: u32,
        player_name: String,
        victim_name: String,
        method: String,
    },
    ClientDisconnect { player_id: u32 },
    Other { action_name: String, details: String },
}

#[derive(Debug, Clone)]
pub struct Game {
    pub id: u32,
    pub events: Vec<GameEvent>,
    pub init_details: Option<String>,
    pub completed: bool,
    pub kills_by_means: HashMap<String, u32>,
    pub killers: HashMap<String, u32>,
}

impl Game {
    pub fn new(id: u32) -> Self {
        Game {
            id,
            events: Vec::new(),
            init_details: None,
            completed: false,
            kills_by_means: HashMap::new(),
            killers: HashMap::new(),
        }
    }

    pub fn add_event(&mut self, event: GameEvent) {
        if let Action::InitGame { details } = &event.action {
            self.init_details = Some(details.clone());
        } else if matches!(event.action, Action::ShutdownGame) {
            self.completed = true;
        } else if let Action::Kill { method, player_name, .. } = &event.action {
            // Update kills by means
            *self.kills_by_means.entry(method.clone()).or_insert(0) += 1;

            // Update killers (exclude <world> as it's not a real player)
            if player_name != "<world>" {
                *self.killers.entry(player_name.clone()).or_insert(0) += 1;
            }
        }
        self.events.push(event);
    }

    pub fn get_players(&self) -> HashMap<u32, String> {
        let mut players = HashMap::new();

        for event in &self.events {
            match &event.action {
                Action::ClientUserinfoChanged { player_id, info } => {
                    if let Some(name) = extract_player_name(info) {
                        players.insert(*player_id, name);
                    }
                }
                _ => {}
            }
        }

        players
    }

    pub fn get_kills(&self) -> Vec<&GameEvent> {
        self.events
            .iter()
            .filter(|e| matches!(e.action, Action::Kill { .. }))
            .collect()
    }
}

#[derive(Debug)]
pub struct LogParser {
    games: Vec<Game>,
    current_game: Option<Game>,
    game_counter: u32,
    overall_kills_by_means: HashMap<String, u32>,
    overall_killers: HashMap<String, u32>,
}

impl LogParser {
    pub fn new() -> Self {
        LogParser {
            games: Vec::new(),
            current_game: None,
            game_counter: 0,
            overall_kills_by_means: HashMap::new(),
            overall_killers: HashMap::new(),
        }
    }

    pub fn parse_file(&mut self, file_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let content = fs::read_to_string(file_path)?;

        for line in content.lines() {
            if let Some(event) = self.parse_line(line) {
                self.handle_event(event);
            }
        }

        if let Some(game) = self.current_game.take() {
            self.update_overall_stats(&game);
            self.games.push(game);
        }

        Ok(())
    }

    fn parse_line(&self, line: &str) -> Option<GameEvent> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }

        let re = Regex::new(r"^\s*(\d{1,2}:\d{2})\s+(.+)$").unwrap();
        let captures = re.captures(line)?;

        let timestamp = captures.get(1)?.as_str().to_string();
        let content = captures.get(2)?.as_str();

        let action = self.parse_action(content)?;

        Some(GameEvent { timestamp, action })
    }

    fn parse_action(&self, content: &str) -> Option<Action> {
        if content.starts_with("InitGame:") {
            let details = content.strip_prefix("InitGame:")?.trim().to_string();
            return Some(Action::InitGame { details });
        }

        if content == "ShutdownGame:" {
            return Some(Action::ShutdownGame);
        }

        if content.starts_with("ClientConnect:") {
            let id_str = content.strip_prefix("ClientConnect:")?.trim();
            let player_id = id_str.parse::<u32>().ok()?;
            return Some(Action::ClientConnect { player_id });
        }

        if content.starts_with("ClientUserinfoChanged:") {
            let details = content.strip_prefix("ClientUserinfoChanged:")?.trim();
            let parts: Vec<&str> = details.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                let player_id = parts[0].parse::<u32>().ok()?;
                let info = parts[1].to_string();
                return Some(Action::ClientUserinfoChanged { player_id, info });
            }
        }

        if content.starts_with("ClientBegin:") {
            let id_str = content.strip_prefix("ClientBegin:")?.trim();
            let player_id = id_str.parse::<u32>().ok()?;
            return Some(Action::ClientBegin { player_id });
        }

        if content.starts_with("ClientDisconnect:") {
            let id_str = content.strip_prefix("ClientDisconnect:")?.trim();
            let player_id = id_str.parse::<u32>().ok()?;
            return Some(Action::ClientDisconnect { player_id });
        }

        if content.starts_with("Item:") {
            let details = content.strip_prefix("Item:")?.trim();
            let parts: Vec<&str> = details.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                let item_id = parts[0].parse::<u32>().ok()?;
                let description = parts[1].to_string();
                return Some(Action::Item { item_id, description });
            }
        }

        if content.starts_with("Kill:") {
            let details = content.strip_prefix("Kill:")?.trim();
            return self.parse_kill_action(details);
        }

        if let Some(colon_pos) = content.find(':') {
            let action_name = content[..colon_pos].to_string();
            let details = content[colon_pos + 1..].trim().to_string();
            return Some(Action::Other { action_name, details });
        }

        None
    }

    fn parse_kill_action(&self, details: &str) -> Option<Action> {
        let parts: Vec<&str> = details.splitn(2, ':').collect();
        if parts.len() != 2 {
            return None;
        }

        let ids_part = parts[0].trim();
        let description_part = parts[1].trim();

        let id_parts: Vec<&str> = ids_part.split_whitespace().collect();
        if id_parts.len() != 3 {
            return None;
        }

        let kill_id = id_parts[0].parse::<u32>().ok()?;
        let player_id = id_parts[1].parse::<u32>().ok()?;
        let victim_id = id_parts[2].parse::<u32>().ok()?;

        let re = Regex::new(r"^(.+?)\s+killed\s+(.+?)\s+by\s+(.+)$").unwrap();
        let captures = re.captures(description_part)?;

        let player_name = captures.get(1)?.as_str().to_string();
        let victim_name = captures.get(2)?.as_str().to_string();
        let method = captures.get(3)?.as_str().to_string();

        Some(Action::Kill {
            kill_id,
            player_id,
            victim_id,
            player_name,
            victim_name,
            method,
        })
    }

    fn handle_event(&mut self, event: GameEvent) {
        match &event.action {
            Action::InitGame { .. } => {
                if let Some(game) = self.current_game.take() {
                    self.update_overall_stats(&game);
                    self.games.push(game);
                }

                self.game_counter += 1;
                let mut new_game = Game::new(self.game_counter);
                new_game.add_event(event);
                self.current_game = Some(new_game);
            }
            Action::ShutdownGame => {
                if let Some(ref mut game) = self.current_game {
                    game.add_event(event);
                    let completed_game = self.current_game.take().unwrap();
                    self.update_overall_stats(&completed_game);
                    self.games.push(completed_game);
                }
            }
            _ => {
                if let Some(ref mut game) = self.current_game {
                    game.add_event(event);
                }
            }
        }
    }

    fn update_overall_stats(&mut self, game: &Game) {
        // Update overall kills by means
        for (method, count) in &game.kills_by_means {
            *self.overall_kills_by_means.entry(method.clone()).or_insert(0) += count;
        }

        // Update overall killers
        for (killer, count) in &game.killers {
            *self.overall_killers.entry(killer.clone()).or_insert(0) += count;
        }
    }

    pub fn get_games(&self) -> &[Game] {
        &self.games
    }

    pub fn print_summary(&self) {
        println!("Parsed {} games:", self.games.len());

        for game in &self.games {
            println!("\nGame {}: {} events ({})",
                game.id,
                game.events.len(),
                if game.completed { "completed" } else { "incomplete" }
            );

            let players = game.get_players();
            println!("  Players: {}", players.len());
            for (id, name) in &players {
                println!("    {}: {}", id, name);
            }

            let kills = game.get_kills();
            println!("  Kills: {}", kills.len());

            // Show kills by means for this game
            if !game.kills_by_means.is_empty() {
                println!("  Kills by means:");
                let mut sorted_means: Vec<_> = game.kills_by_means.iter().collect();
                sorted_means.sort_by(|a, b| b.1.cmp(a.1)); // Sort by kill count descending
                for (method, count) in sorted_means {
                    println!("    {}: {}", method, count);
                }
            }

            // Show killers for this game
            if !game.killers.is_empty() {
                println!("  Killers:");
                let mut sorted_killers: Vec<_> = game.killers.iter().collect();
                sorted_killers.sort_by(|a, b| b.1.cmp(a.1)); // Sort by kill count descending
                for (killer, count) in sorted_killers {
                    println!("    {}: {} kills", killer, count);
                }
            }
        }

        // Show overall statistics
        println!("\n=== Overall Statistics ===");

        // Overall kills by means
        if !self.overall_kills_by_means.is_empty() {
            println!("\nOverall kills by means:");
            let mut sorted_means: Vec<_> = self.overall_kills_by_means.iter().collect();
            sorted_means.sort_by(|a, b| b.1.cmp(a.1)); // Sort by kill count descending
            for (method, count) in sorted_means {
                println!("  {}: {}", method, count);
            }
        }

        // Overall killers
        if !self.overall_killers.is_empty() {
            println!("\nOverall killers (top players by kills):");
            let mut sorted_killers: Vec<_> = self.overall_killers.iter().collect();
            sorted_killers.sort_by(|a, b| b.1.cmp(a.1)); // Sort by kill count descending
            for (killer, count) in sorted_killers {
                println!("  {}: {} kills", killer, count);
            }
        }

        // Player Ranking Report
        if !self.overall_killers.is_empty() {
            println!("\n=== PLAYER RANKING REPORT ===");
            let mut sorted_killers: Vec<_> = self.overall_killers.iter().collect();
            sorted_killers.sort_by(|a, b| b.1.cmp(a.1)); // Sort by kill count descending

            for (rank, (player, kills)) in sorted_killers.iter().enumerate() {
                let position = match rank + 1 {
                    1 => "1st".to_string(),
                    2 => "2nd".to_string(),
                    3 => "3rd".to_string(),
                    n => format!("{}th", n),
                };
                println!("{:>4} place: {} with {} kills", position, player, kills);
            }
        }
    }
}

fn extract_player_name(userinfo: &str) -> Option<String> {
    let re = Regex::new(r"n\\([^\\]+)").unwrap();
    let captures = re.captures(userinfo)?;
    Some(captures.get(1)?.as_str().to_string())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let mut parser = LogParser::new();
    parser.parse_file(&args.log_file)?;

    parser.print_summary();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_client_connect() {
        let parser = LogParser::new();
        let event = parser.parse_line("20:34 ClientConnect: 2").unwrap();

        assert_eq!(event.timestamp, "20:34");
        assert!(matches!(event.action, Action::ClientConnect { player_id: 2 }));
    }

    #[test]
    fn test_parse_client_disconnect() {
        let parser = LogParser::new();
        let event = parser.parse_line("21:10 ClientDisconnect: 2").unwrap();

        assert_eq!(event.timestamp, "21:10");
        assert!(matches!(event.action, Action::ClientDisconnect { player_id: 2 }));
    }

    #[test]
    fn test_parse_client_userinfo_changed() {
        let parser = LogParser::new();
        let event = parser.parse_line("20:34 ClientUserinfoChanged: 2 n\\Isgalamido\\t\\0\\model\\xian/default").unwrap();

        assert_eq!(event.timestamp, "20:34");
        if let Action::ClientUserinfoChanged { player_id, info } = event.action {
            assert_eq!(player_id, 2);
            assert!(info.contains("Isgalamido"));
        } else {
            panic!("Expected ClientUserinfoChanged action");
        }
    }

    #[test]
    fn test_parse_client_begin() {
        let parser = LogParser::new();
        let event = parser.parse_line("20:37 ClientBegin: 2").unwrap();

        assert_eq!(event.timestamp, "20:37");
        assert!(matches!(event.action, Action::ClientBegin { player_id: 2 }));
    }

    #[test]
    fn test_parse_item() {
        let parser = LogParser::new();
        let event = parser.parse_line("20:40 Item: 2 weapon_rocketlauncher").unwrap();

        assert_eq!(event.timestamp, "20:40");
        if let Action::Item { item_id, description } = event.action {
            assert_eq!(item_id, 2);
            assert_eq!(description, "weapon_rocketlauncher");
        } else {
            panic!("Expected Item action");
        }
    }

    #[test]
    fn test_parse_kill() {
        let parser = LogParser::new();
        let event = parser.parse_line("22:06 Kill: 2 3 7: Isgalamido killed Mocinha by MOD_ROCKET_SPLASH").unwrap();

        assert_eq!(event.timestamp, "22:06");
        if let Action::Kill { kill_id, player_id, victim_id, player_name, victim_name, method } = event.action {
            assert_eq!(kill_id, 2);
            assert_eq!(player_id, 3);
            assert_eq!(victim_id, 7);
            assert_eq!(player_name, "Isgalamido");
            assert_eq!(victim_name, "Mocinha");
            assert_eq!(method, "MOD_ROCKET_SPLASH");
        } else {
            panic!("Expected Kill action");
        }
    }

    #[test]
    fn test_parse_init_game() {
        let parser = LogParser::new();
        let event = parser.parse_line("0:00 InitGame: \\sv_floodProtect\\1\\sv_maxPing\\0").unwrap();

        assert_eq!(event.timestamp, "0:00");
        if let Action::InitGame { details } = event.action {
            assert!(details.contains("sv_floodProtect"));
        } else {
            panic!("Expected InitGame action");
        }
    }

    #[test]
    fn test_parse_shutdown_game() {
        let parser = LogParser::new();
        let event = parser.parse_line("20:37 ShutdownGame:").unwrap();

        assert_eq!(event.timestamp, "20:37");
        assert!(matches!(event.action, Action::ShutdownGame));
    }

    #[test]
    fn test_extract_player_name() {
        let userinfo = "n\\Isgalamido\\t\\0\\model\\xian/default\\hmodel\\xian/default";
        let name = extract_player_name(userinfo).unwrap();
        assert_eq!(name, "Isgalamido");
    }

    #[test]
    fn test_game_parser_multiple_games() {
        let mut parser = LogParser::new();

        let events = vec![
            "0:00 InitGame: \\sv_hostname\\Test Server",
            "0:01 ClientConnect: 1",
            "0:02 ShutdownGame:",
            "0:03 InitGame: \\sv_hostname\\Test Server 2",
            "0:04 ClientConnect: 2",
        ];

        for line in events {
            if let Some(event) = parser.parse_line(line) {
                parser.handle_event(event);
            }
        }

        if let Some(game) = parser.current_game.take() {
            parser.games.push(game);
        }

        assert_eq!(parser.games.len(), 2);
        assert!(parser.games[0].completed);
        assert!(!parser.games[1].completed);
    }

    #[test]
    fn test_game_parser_duplicate_init() {
        let mut parser = LogParser::new();

        let events = vec![
            "0:00 InitGame: \\sv_hostname\\Test Server 1",
            "0:01 ClientConnect: 1",
            "0:02 InitGame: \\sv_hostname\\Test Server 2",
            "0:03 ClientConnect: 2",
            "0:04 ShutdownGame:",
        ];

        for line in events {
            if let Some(event) = parser.parse_line(line) {
                parser.handle_event(event);
            }
        }

        assert_eq!(parser.games.len(), 2);
        assert!(!parser.games[0].completed);
        assert!(parser.games[1].completed);
    }

    #[test]
    fn test_parse_file_integration() {
        let temp_content = r#"0:00 InitGame: \sv_hostname\Test Server
0:01 ClientConnect: 1
0:02 ClientUserinfoChanged: 1 n\TestPlayer\t\0
0:03 Item: 1 weapon_shotgun
0:04 Kill: 1 1 2: TestPlayer killed Bot by MOD_SHOTGUN
0:05 ShutdownGame:"#;

        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join("test_log.txt");
        std::fs::write(&temp_file_path, temp_content).unwrap();

        let mut parser = LogParser::new();
        parser.parse_file(&temp_file_path).unwrap();

        std::fs::remove_file(&temp_file_path).unwrap();

        assert_eq!(parser.games.len(), 1);
        let game = &parser.games[0];
        assert!(game.completed);
        assert_eq!(game.events.len(), 6);

        let players = game.get_players();
        assert_eq!(players.len(), 1);
        assert_eq!(players.get(&1), Some(&"TestPlayer".to_string()));

        let kills = game.get_kills();
        assert_eq!(kills.len(), 1);
    }

    #[test]
    fn test_parse_kill_with_world() {
        let parser = LogParser::new();
        let event = parser.parse_line("20:54 Kill: 1022 2 22: <world> killed Isgalamido by MOD_TRIGGER_HURT").unwrap();

        assert_eq!(event.timestamp, "20:54");
        if let Action::Kill { kill_id, player_id, victim_id, player_name, victim_name, method } = event.action {
            assert_eq!(kill_id, 1022);
            assert_eq!(player_id, 2);
            assert_eq!(victim_id, 22);
            assert_eq!(player_name, "<world>");
            assert_eq!(victim_name, "Isgalamido");
            assert_eq!(method, "MOD_TRIGGER_HURT");
        } else {
            panic!("Expected Kill action");
        }
    }

    #[test]
    fn test_parse_other_actions() {
        let parser = LogParser::new();
        let event = parser.parse_line("15:00 Exit: Timelimit hit.").unwrap();

        assert_eq!(event.timestamp, "15:00");
        if let Action::Other { action_name, details } = event.action {
            assert_eq!(action_name, "Exit");
            assert_eq!(details, "Timelimit hit.");
        } else {
            panic!("Expected Other action");
        }
    }

    #[test]
    fn test_empty_and_invalid_lines() {
        let parser = LogParser::new();

        assert!(parser.parse_line("").is_none());
        assert!(parser.parse_line("   ").is_none());
        assert!(parser.parse_line("invalid line without timestamp").is_none());
        assert!(parser.parse_line("20:34").is_none());
    }

    #[test]
    fn test_kills_by_means_aggregation() {
        let mut game = Game::new(1);

        // Add some kill events with different methods
        let kill1 = GameEvent {
            timestamp: "20:00".to_string(),
            action: Action::Kill {
                kill_id: 1,
                player_id: 2,
                victim_id: 3,
                player_name: "Alice".to_string(),
                victim_name: "Bob".to_string(),
                method: "MOD_ROCKET_SPLASH".to_string(),
            }
        };

        let kill2 = GameEvent {
            timestamp: "20:01".to_string(),
            action: Action::Kill {
                kill_id: 2,
                player_id: 2,
                victim_id: 4,
                player_name: "Alice".to_string(),
                victim_name: "Charlie".to_string(),
                method: "MOD_ROCKET_SPLASH".to_string(),
            }
        };

        let kill3 = GameEvent {
            timestamp: "20:02".to_string(),
            action: Action::Kill {
                kill_id: 3,
                player_id: 3,
                victim_id: 2,
                player_name: "Bob".to_string(),
                victim_name: "Alice".to_string(),
                method: "MOD_SHOTGUN".to_string(),
            }
        };

        game.add_event(kill1);
        game.add_event(kill2);
        game.add_event(kill3);

        // Test kills by means aggregation
        assert_eq!(game.kills_by_means.get("MOD_ROCKET_SPLASH"), Some(&2));
        assert_eq!(game.kills_by_means.get("MOD_SHOTGUN"), Some(&1));

        // Test killers aggregation
        assert_eq!(game.killers.get("Alice"), Some(&2));
        assert_eq!(game.killers.get("Bob"), Some(&1));
        assert_eq!(game.killers.get("<world>"), None); // <world> should not be included
    }

    #[test]
    fn test_overall_aggregation() {
        let mut parser = LogParser::new();

        let events = vec![
            "0:00 InitGame: \\sv_hostname\\Test Server",
            "0:01 Kill: 1 2 3: Alice killed Bob by MOD_ROCKET_SPLASH",
            "0:02 Kill: 2 2 4: Alice killed Charlie by MOD_SHOTGUN",
            "0:03 ShutdownGame:",
            "0:04 InitGame: \\sv_hostname\\Test Server 2",
            "0:05 Kill: 3 3 2: Bob killed Alice by MOD_ROCKET_SPLASH",
            "0:06 ShutdownGame:",
        ];

        for line in events {
            if let Some(event) = parser.parse_line(line) {
                parser.handle_event(event);
            }
        }

        // Test overall aggregations
        assert_eq!(parser.overall_kills_by_means.get("MOD_ROCKET_SPLASH"), Some(&2));
        assert_eq!(parser.overall_kills_by_means.get("MOD_SHOTGUN"), Some(&1));

        assert_eq!(parser.overall_killers.get("Alice"), Some(&2));
        assert_eq!(parser.overall_killers.get("Bob"), Some(&1));
    }

    #[test]
    fn test_world_kills_excluded() {
        let mut game = Game::new(1);

        let world_kill = GameEvent {
            timestamp: "20:00".to_string(),
            action: Action::Kill {
                kill_id: 1022,
                player_id: 2,
                victim_id: 22,
                player_name: "<world>".to_string(),
                victim_name: "Alice".to_string(),
                method: "MOD_TRIGGER_HURT".to_string(),
            }
        };

        game.add_event(world_kill);

        // <world> kills should be counted in kills_by_means but not in killers
        assert_eq!(game.kills_by_means.get("MOD_TRIGGER_HURT"), Some(&1));
        assert_eq!(game.killers.get("<world>"), None);
        assert!(game.killers.is_empty());
    }

    #[test]
    fn test_ranking_order() {
        let mut parser = LogParser::new();

        let events = vec![
            "0:00 InitGame: \\sv_hostname\\Test Server",
            "0:01 Kill: 1 1 2: Charlie killed Alice by MOD_ROCKET_SPLASH", // Charlie: 1 kill
            "0:02 Kill: 2 2 1: Alice killed Charlie by MOD_SHOTGUN",      // Alice: 1 kill
            "0:03 Kill: 3 2 3: Alice killed Bob by MOD_RAILGUN",          // Alice: 2 kills total
            "0:04 Kill: 4 3 2: Bob killed Alice by MOD_MACHINEGUN",       // Bob: 1 kill
            "0:05 Kill: 5 2 3: Alice killed Bob by MOD_ROCKET_SPLASH",    // Alice: 3 kills total
            "0:06 ShutdownGame:",
        ];

        for line in events {
            if let Some(event) = parser.parse_line(line) {
                parser.handle_event(event);
            }
        }

        // Test that killers are properly sorted
        let mut sorted_killers: Vec<_> = parser.overall_killers.iter().collect();
        sorted_killers.sort_by(|a, b| b.1.cmp(a.1));

        // Alice should be first with 3 kills, then Charlie and Bob tied with 1 kill each
        assert_eq!(sorted_killers[0], (&"Alice".to_string(), &3));
        assert!(sorted_killers[1].1 == &1); // Either Charlie or Bob
        assert!(sorted_killers[2].1 == &1); // Either Charlie or Bob
        assert_eq!(sorted_killers.len(), 3);
    }
}
