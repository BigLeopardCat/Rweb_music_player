#![windows_subsystem = "windows"]

use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use lofty::prelude::AudioFile;
use lofty::probe::Probe;
use rodio::{Decoder, OutputStream, Sink, Source};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::future::IntoFuture;
use std::io::{BufReader, Cursor};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

// --- Audio Engine ---

enum AudioCommand {
    PlayFile(PathBuf),
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
    Seek(Duration),
}

enum AudioStatus {
    Status {
        position: Duration,
        duration: Duration,
        is_playing: bool,
    },
    Finished,
    Started(PathBuf),
}

fn start_audio_thread() -> (Sender<AudioCommand>, Receiver<AudioStatus>) {
    let (cmd_tx, cmd_rx) = unbounded();
    let (status_tx, status_rx) = unbounded();

    thread::spawn(move || {
        let (_stream, stream_handle) = OutputStream::try_default().unwrap();
        let mut sink = Sink::try_new(&stream_handle).unwrap();
        
        let mut last_played_path: Option<PathBuf> = None;
        let mut total_duration = Duration::from_secs(0);
        
        // Time tracking
        let mut playback_start: Option<Instant> = None;
        let mut pause_start: Option<Instant> = None;
        let mut accumulated_pause = Duration::from_secs(0);
        let mut seek_offset = Duration::from_secs(0);
        let mut is_playing = false;
        let mut has_started = false;
        
        // Status update throttling
        let mut last_status_time = Instant::now();
        let mut force_status_update = false;

        loop {
            // Check for commands (non-blocking or with timeout)
            // We use recv_timeout to allow sending status updates periodically
            match cmd_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(cmd) => {
                    match cmd {
                        AudioCommand::PlayFile(path) => {
                            last_played_path = Some(path.clone());
                            // Load entire file into memory to avoid I/O stuttering completely
                            if let Ok(file_content) = std::fs::read(&path) {
                                let cursor = Cursor::new(file_content);
                                if let Ok(source) = Decoder::new(cursor) {
                                    // Try to get duration from lofty first, then rodio
                                    total_duration = if let Ok(tagged_file) = Probe::open(&path).and_then(|p| p.read()) {
                                        tagged_file.properties().duration()
                                    } else {
                                        source.total_duration().unwrap_or(Duration::from_secs(0))
                                    };
                                    
                                    // Recreate sink to prevent sample rate mismatch glitches
                                    sink = Sink::try_new(&stream_handle).unwrap();
                                    // No need for buffered() anymore since data is in RAM
                                    sink.append(source);
                                    sink.play();
                                    
                                    // Reset timing
                                    playback_start = Some(Instant::now());
                                    pause_start = None;
                                    accumulated_pause = Duration::from_secs(0);
                                    seek_offset = Duration::from_secs(0);
                                    is_playing = true;
                                    has_started = true;
                                    let _ = status_tx.send(AudioStatus::Started(path.clone()));
                                }
                            }
                        }
                        AudioCommand::Pause => {
                            if !sink.is_paused() {
                                sink.pause();
                                pause_start = Some(Instant::now());
                                is_playing = false;
                            }
                        }
                        AudioCommand::Resume => {
                            if sink.empty() && last_played_path.is_some() {
                                // Replay logic if stopped
                                 if let Ok(file_content) = std::fs::read(last_played_path.as_ref().unwrap()) {
                                    let cursor = Cursor::new(file_content);
                                    if let Ok(source) = Decoder::new(cursor) {
                                        total_duration = source.total_duration().unwrap_or(Duration::from_secs(0));
                                        sink.append(source);
                                        sink.play();
                                        playback_start = Some(Instant::now());
                                        pause_start = None;
                                        accumulated_pause = Duration::from_secs(0);
                                        seek_offset = Duration::from_secs(0);
                                        is_playing = true;
                                        has_started = true;
                                    }
                                }
                            } else if sink.is_paused() {
                                sink.play();
                                if let Some(start) = pause_start {
                                    accumulated_pause += start.elapsed();
                                }
                                pause_start = None;
                                is_playing = true;
                            }
                        }
                        AudioCommand::Stop => {
                            sink.stop();
                            is_playing = false;
                            has_started = false;
                            playback_start = None;
                        }
                        AudioCommand::SetVolume(v) => sink.set_volume(v),
                        AudioCommand::Seek(pos) => {
                            if let Err(_) = sink.try_seek(pos) {
                                // Fallback: Manual seek by recreating source
                                if let Some(path) = &last_played_path {
                                    if let Ok(file_content) = std::fs::read(path) {
                                        let cursor = Cursor::new(file_content);
                                        if let Ok(source) = Decoder::new(cursor) {
                                            let new_source = source.skip_duration(pos);
                                            sink.stop();
                                            sink.append(new_source);
                                            sink.play();
                                            
                                            // Reset timing for manual seek
                                            playback_start = Some(Instant::now());
                                            accumulated_pause = Duration::from_secs(0);
                                            seek_offset = pos;
                                            
                                            if !is_playing {
                                                pause_start = Some(Instant::now());
                                                sink.pause();
                                            }
                                        }
                                    }
                                }
                            } else {
                                // Adjust timing to match new position
                                playback_start = Some(Instant::now());
                                accumulated_pause = Duration::from_secs(0);
                                seek_offset = pos;
                                
                                // If we were paused, we need to remain paused but update the "visual" position
                                if !is_playing {
                                    pause_start = Some(Instant::now());
                                }
                            }
                        }
                    }
                    // Force status update after any command
                    force_status_update = true;
                },
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // No command, just update status
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            }

            // Calculate current position
            let mut current_pos = Duration::from_secs(0);
            if let Some(start) = playback_start {
                let raw_elapsed = start.elapsed();
                let pause_duration = if let Some(p_start) = pause_start {
                    accumulated_pause + p_start.elapsed()
                } else {
                    accumulated_pause
                };
                
                if raw_elapsed + seek_offset > pause_duration {
                     current_pos = (raw_elapsed + seek_offset) - pause_duration;
                }
            }
            
            // Clamp to total duration
            if total_duration.as_secs() > 0 && current_pos > total_duration {
                current_pos = total_duration;
            }

            // Check if finished
            if has_started && sink.empty() {
                has_started = false;
                is_playing = false;
                playback_start = None;
                let _ = status_tx.send(AudioStatus::Finished);
            } else {
                // Send status update if forced (command processed) or enough time passed (100ms)
                // This prevents flooding the UI thread with updates, allowing interpolation to do its job
                if force_status_update || last_status_time.elapsed() >= Duration::from_millis(100) {
                    let _ = status_tx.send(AudioStatus::Status {
                        position: current_pos,
                        duration: total_duration,
                        is_playing,
                    });
                    last_status_time = Instant::now();
                    force_status_update = false;
                }
            }
        }
    });
    (cmd_tx, status_rx)
}

// --- Persistence ---

fn get_config_path(filename: &str) -> PathBuf {
    if let Ok(mut exe_path) = std::env::current_exe() {
        exe_path.pop();
        exe_path.push(filename);
        return exe_path;
    }
    PathBuf::from(filename)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct PlaylistItem {
    path: PathBuf,
    name: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct PlaylistsManager {
    current_name: String,
    lists: HashMap<String, Vec<PlaylistItem>>,
}

impl PlaylistsManager {
    fn load() -> Self {
        let path = get_config_path("playlists.json");
        if let Ok(file) = File::open(&path) {
            if let Ok(data) = serde_json::from_reader(BufReader::new(file)) {
                return data;
            }
        }
        // Fallback or migration could go here, but for now we start fresh if schema mismatches
        let mut lists = HashMap::new();
        lists.insert("Default List".to_string(), Vec::new());
        lists.insert("默认列表".to_string(), Vec::new());
        Self {
            current_name: "Default List".to_string(),
            lists,
        }
    }

    fn save(&self) {
        let path = get_config_path("playlists.json");
        if let Ok(file) = File::create(&path) {
            let _ = serde_json::to_writer(file, self);
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct AppConfig {
    port: u16,
    #[serde(default = "default_font_size")]
    lyrics_font_size: f32,
    #[serde(default = "default_text_color")]
    lyrics_text_color: [u8; 4],
    #[serde(default = "default_bg_color")]
    lyrics_bg_color: [u8; 4],
    #[serde(default = "default_ui_bg_color")]
    ui_bg_color: [u8; 4],
}

fn default_font_size() -> f32 { 32.0 }
fn default_text_color() -> [u8; 4] { [255, 255, 255, 255] }
fn default_bg_color() -> [u8; 4] { [0, 0, 0, 0] }
fn default_ui_bg_color() -> [u8; 4] { [30, 30, 30, 255] }

impl AppConfig {
    fn load() -> Self {
        let path = get_config_path("config.json");
        if let Ok(file) = File::open(&path) {
            if let Ok(config) = serde_json::from_reader(BufReader::new(file)) {
                return config;
            }
        }
        let config = Self { 
            port: 3000,
            lyrics_font_size: default_font_size(),
            lyrics_text_color: default_text_color(),
            lyrics_bg_color: default_bg_color(),
            ui_bg_color: default_ui_bg_color(),
        };
        if let Ok(file) = File::create(&path) {
            let _ = serde_json::to_writer_pretty(file, &config);
        }
        config
    }

    fn save(&self) {
        let path = get_config_path("config.json");
        if let Ok(file) = File::create(&path) {
            let _ = serde_json::to_writer_pretty(file, self);
        }
    }
}

// --- Shared State ---

#[derive(Clone)]
struct AppState {
    audio_tx: Sender<AudioCommand>,
    data: Arc<Mutex<PlaylistsManager>>,
}

// --- API Models ---

#[derive(Deserialize)]
struct PlayRequest {
    path: Option<String>,
    index: Option<usize>,
    playlist: Option<String>,
}

#[derive(Deserialize)]
struct RemoveRequest {
    index: usize,
    playlist: Option<String>,
}

#[derive(Deserialize)]
struct RenamePlaylistRequest {
    old_name: String,
    new_name: String,
}

#[derive(Deserialize)]
struct DeletePlaylistRequest {
    name: String,
}

#[derive(Deserialize)]
struct SwitchPlaylistRequest {
    name: String,
}

#[derive(Serialize)]
struct PlaylistFile {
    path: String,
    name: String,
    exists: bool,
}

#[derive(Serialize)]
struct PlaylistResponse {
    current: String,
    files: Vec<PlaylistFile>,
    all_playlists: Vec<String>,
}

// --- API Handlers ---

async fn api_play(
    State(state): State<AppState>,
    Json(payload): Json<PlayRequest>,
) -> Json<String> {
    let mut data = state.data.lock().unwrap();
    let target_list_name = payload.playlist.clone().unwrap_or_else(|| data.current_name.clone());
    let list = data.lists.entry(target_list_name.clone()).or_default();

    let path_to_play = if let Some(idx) = payload.index {
        list.get(idx).map(|item| item.path.clone())
    } else if let Some(path_str) = payload.path {
        let path = PathBuf::from(path_str);
        if path.exists() {
            if !list.iter().any(|item| item.path == path) {
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let mut final_name = name.clone();
                let mut count = 1;
                while list.iter().any(|item| item.name == final_name) {
                    final_name = format!("{} ({})", name, count);
                    count += 1;
                }
                // Insert at top (newest first)
                list.insert(0, PlaylistItem { path: path.clone(), name: final_name });
                data.save();
            }
            Some(path)
        } else {
            None
        }
    } else {
        None
    };

    drop(data);

    if let Some(path) = path_to_play {
        let _ = state.audio_tx.send(AudioCommand::PlayFile(path));
        Json(format!("Playing in {}", target_list_name))
    } else {
        Json("File not found or invalid request".to_string())
    }
}

async fn api_remove_from_playlist(
    State(state): State<AppState>,
    Json(payload): Json<RemoveRequest>,
) -> Json<String> {
    let mut data = state.data.lock().unwrap();
    let target_list_name = payload.playlist.unwrap_or_else(|| data.current_name.clone());
    
    if let Some(list) = data.lists.get_mut(&target_list_name) {
        if payload.index < list.len() {
            list.remove(payload.index);
            data.save();
            Json(format!("Removed item {} from {}", payload.index, target_list_name))
        } else {
            Json("Index out of bounds".to_string())
        }
    } else {
        Json("Playlist not found".to_string())
    }
}

async fn api_get_playlist(State(state): State<AppState>) -> Json<PlaylistResponse> {
    let data = state.data.lock().unwrap();
    let current = data.current_name.clone();
    let files = data.lists.get(&current).unwrap_or(&vec![]).iter()
        .map(|p| PlaylistFile {
            path: p.path.to_string_lossy().to_string(),
            name: p.name.clone(),
            exists: p.path.exists()
        }).collect();
    let all_playlists = data.lists.keys().cloned().collect();
    
    Json(PlaylistResponse { current, files, all_playlists })
}

async fn api_rename_playlist(
    State(state): State<AppState>,
    Json(payload): Json<RenamePlaylistRequest>,
) -> Json<String> {
    let mut data = state.data.lock().unwrap();
    if !data.lists.contains_key(&payload.old_name) {
        return Json("Playlist not found".to_string());
    }
    if data.lists.contains_key(&payload.new_name) {
        return Json("New name already exists".to_string());
    }
    
    if let Some(list) = data.lists.remove(&payload.old_name) {
        data.lists.insert(payload.new_name.clone(), list);
        if data.current_name == payload.old_name {
            data.current_name = payload.new_name;
        }
        data.save();
        Json("Playlist renamed".to_string())
    } else {
        Json("Error renaming playlist".to_string())
    }
}

async fn api_delete_playlist(
    State(state): State<AppState>,
    Json(payload): Json<DeletePlaylistRequest>,
) -> Json<String> {
    let mut data = state.data.lock().unwrap();
    if data.lists.len() <= 1 {
        return Json("Cannot delete the last playlist".to_string());
    }
    
    if data.lists.remove(&payload.name).is_some() {
        if data.current_name == payload.name {
            if let Some(first) = data.lists.keys().next().cloned() {
                data.current_name = first;
            } else {
                data.lists.insert("Default List".to_string(), Vec::new());
                data.current_name = "Default List".to_string();
            }
        }
        data.save();
        Json("Playlist deleted".to_string())
    } else {
        Json("Playlist not found".to_string())
    }
}

async fn api_switch_playlist(
    State(state): State<AppState>,
    Json(payload): Json<SwitchPlaylistRequest>,
) -> Json<String> {
    let mut data = state.data.lock().unwrap();
    if data.lists.contains_key(&payload.name) {
        data.current_name = payload.name.clone();
        data.save();
        Json(format!("Switched to playlist: {}", payload.name))
    } else {
        Json("Playlist not found".to_string())
    }
}

// --- UI ---

#[derive(PartialEq, Clone, Copy, Debug)]
enum PlaybackMode {
    Order,      // 顺序播放
    ListLoop,   // 列表循环
    SingleLoop, // 单曲循环
    Single,     // 单曲播放
}

impl PlaybackMode {
    fn as_str(&self, lang: Language) -> &'static str {
        match lang {
            Language::Chinese => match self {
                PlaybackMode::Order => "顺序播放",
                PlaybackMode::ListLoop => "列表循环",
                PlaybackMode::SingleLoop => "单曲循环",
                PlaybackMode::Single => "单曲播放",
            },
            Language::English => match self {
                PlaybackMode::Order => "Order",
                PlaybackMode::ListLoop => "List Loop",
                PlaybackMode::SingleLoop => "Single Loop",
                PlaybackMode::Single => "Single",
            },
        }
    }
}

// --- Helper Functions ---

fn parse_lrc(content: &str) -> Vec<(Duration, String)> {
    let mut lyrics = Vec::new();
    for line in content.lines() {
        if let Some(start) = line.find('[') {
            if let Some(end) = line.find(']') {
                let time_part = &line[start+1..end];
                let text_part = &line[end+1..];
                let parts: Vec<&str> = time_part.split(':').collect();
                if parts.len() == 2 {
                    if let Ok(min) = parts[0].parse::<u64>() {
                        let sec_parts: Vec<&str> = parts[1].split('.').collect();
                        if !sec_parts.is_empty() {
                             if let Ok(sec) = sec_parts[0].parse::<u64>() {
                                 let millis = if sec_parts.len() > 1 {
                                     let m_str = sec_parts[1];
                                     let m = m_str.parse::<u64>().unwrap_or(0);
                                     // .xx is hundredths (10ms), .xxx is millis (1ms)
                                     if m_str.len() == 2 { m * 10 } else { m }
                                 } else { 0 };
                                 
                                 let duration = Duration::from_secs(min * 60 + sec) + Duration::from_millis(millis);
                                 lyrics.push((duration, text_part.trim().to_string()));
                             }
                        }
                    }
                }
            }
        }
    }
    lyrics.sort_by_key(|k| k.0);
    lyrics
}

fn scroll_label(ui: &mut egui::Ui, text: &str, max_width: f32, should_scroll: bool, time: f64) {
    let font_id = egui::FontId::proportional(14.0);
    let text_galley = ui.painter().layout_no_wrap(text.to_string(), font_id.clone(), ui.visuals().text_color());
    let text_width = text_galley.size().x;

    if text_width <= max_width || !should_scroll {
        ui.add(egui::Label::new(egui::RichText::new(text)).truncate());
    } else {
        let speed = 30.0; 
        let gap = 50.0;
        let cycle_len = text_width + gap;
        let offset = (time * speed as f64) % cycle_len as f64;
        
        let (rect, _) = ui.allocate_exact_size(egui::vec2(max_width, text_galley.size().y), egui::Sense::hover());
        
        ui.painter().with_clip_rect(rect).add(egui::Shape::Text(egui::epaint::TextShape {
            pos: rect.min - egui::vec2(offset as f32, 0.0),
            galley: text_galley.clone(),
            underline: egui::Stroke::NONE,
            override_text_color: None,
            angle: 0.0,
            fallback_color: egui::Color32::WHITE,
            opacity_factor: 1.0,
        }));
        
        if offset as f32 + max_width > text_width + gap {
             ui.painter().with_clip_rect(rect).add(egui::Shape::Text(egui::epaint::TextShape {
                pos: rect.min - egui::vec2(offset as f32, 0.0) + egui::vec2(cycle_len, 0.0),
                galley: text_galley,
                underline: egui::Stroke::NONE,
                override_text_color: None,
                angle: 0.0,
                fallback_color: egui::Color32::WHITE,
                opacity_factor: 1.0,
            }));
        }
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Language {
    Chinese,
    English,
}

impl Language {
    fn as_str(&self) -> &'static str {
        match self {
            Language::Chinese => "中文",
            Language::English => "English",
        }
    }
}

#[derive(Clone, Debug)]
enum PlayerStatus {
    Ready,
    Playing(String),
    Finished,
    Stopped,
    Paused,
}

struct LyricsState {
    current_lyric: String,
    show_desktop_lyrics: bool,
    font_size: f32,
    text_color: egui::Color32,
    bg_color: egui::Color32,
}

struct MusicPlayerApp {
    audio_tx: Sender<AudioCommand>,
    audio_rx: Receiver<AudioStatus>,
    data: Arc<Mutex<PlaylistsManager>>,
    volume: f32,
    player_status: PlayerStatus,
    api_port: u16,
    port_tx: mpsc::UnboundedSender<u16>,
    port_input: String,
    new_playlist_name: String,
    current_playing_file: Option<PathBuf>,
    
    // Lyrics
    lyrics: Vec<(Duration, String)>,
    lyrics_state: Arc<Mutex<LyricsState>>,
    show_lyrics_settings: bool,
    
    // UI Settings
    ui_bg_color: egui::Color32,

    // Playback State
    playback_mode: PlaybackMode,
    current_position: Duration,
    total_duration: Duration,
    is_playing: bool,
    is_seeking: bool, // To prevent updates while dragging slider
    seek_target: Option<Duration>, // For optimistic updates
    last_sync_time: Option<Instant>, // For interpolation

    // Duplicate Handling
    show_duplicate_dialog: bool,
    pending_files: Vec<PathBuf>,

    // Playlist Management Dialogs
    show_rename_dialog: bool,
    rename_playlist_name: String,
    show_delete_playlist_dialog: bool,
    playlist_to_delete: Option<String>,

    // Language
    language: Language,
}

impl MusicPlayerApp {
    fn new(audio_tx: Sender<AudioCommand>, audio_rx: Receiver<AudioStatus>, data: Arc<Mutex<PlaylistsManager>>, port: u16, port_tx: mpsc::UnboundedSender<u16>, cc: &eframe::CreationContext<'_>) -> Self {
        // Load Config for Lyrics
        let config = AppConfig::load();
        
        // Load Chinese font
        let mut fonts = egui::FontDefinitions::default();
        if let Ok(font_data) = std::fs::read("C:\\Windows\\Fonts\\msyh.ttc") {
            fonts.font_data.insert("msyh".to_owned(), egui::FontData::from_owned(font_data).into());
            fonts.families.get_mut(&egui::FontFamily::Proportional).unwrap().insert(0, "msyh".to_owned());
            fonts.families.get_mut(&egui::FontFamily::Monospace).unwrap().push("msyh".to_owned());
            cc.egui_ctx.set_fonts(fonts);
        }

        let lyrics_state = Arc::new(Mutex::new(LyricsState {
            current_lyric: "".to_string(),
            show_desktop_lyrics: false,
            font_size: config.lyrics_font_size,
            text_color: egui::Color32::from_rgba_unmultiplied(config.lyrics_text_color[0], config.lyrics_text_color[1], config.lyrics_text_color[2], config.lyrics_text_color[3]),
            bg_color: egui::Color32::from_rgba_unmultiplied(config.lyrics_bg_color[0], config.lyrics_bg_color[1], config.lyrics_bg_color[2], config.lyrics_bg_color[3]),
        }));

        Self {
            audio_tx,
            audio_rx,
            data,
            volume: 1.0,
            player_status: PlayerStatus::Ready,
            api_port: port,
            port_tx,
            port_input: port.to_string(),
            new_playlist_name: "".to_string(),
            current_playing_file: None,
            lyrics: Vec::new(),
            lyrics_state,
            show_lyrics_settings: false,
            ui_bg_color: egui::Color32::from_rgba_unmultiplied(config.ui_bg_color[0], config.ui_bg_color[1], config.ui_bg_color[2], config.ui_bg_color[3]),
            playback_mode: PlaybackMode::Order,
            current_position: Duration::from_secs(0),
            total_duration: Duration::from_secs(0),
            is_playing: false,
            is_seeking: false,
            seek_target: None,
            last_sync_time: None,
            show_duplicate_dialog: false,
            pending_files: Vec::new(),
            show_rename_dialog: false,
            rename_playlist_name: "".to_string(),
            show_delete_playlist_dialog: false,
            playlist_to_delete: None,
            language: Language::Chinese,
        }
    }

    fn play_file(&mut self, path: PathBuf) {
        let _ = self.audio_tx.send(AudioCommand::PlayFile(path.clone()));
        self.current_playing_file = Some(path.clone());
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        self.player_status = PlayerStatus::Playing(file_name.to_string());
        self.is_playing = true;
        self.last_sync_time = Some(Instant::now());
        self.current_position = Duration::from_secs(0);
    }

    fn play_next(&mut self) {
        let data = self.data.lock().unwrap();
        if let Some(list) = data.lists.get(&data.current_name) {
            if list.is_empty() { return; }
            
            let current_idx = if let Some(curr) = &self.current_playing_file {
                list.iter().position(|p| &p.path == curr)
            } else {
                None
            };

            let next_idx = match self.playback_mode {
                PlaybackMode::Single => None,
                PlaybackMode::SingleLoop => current_idx, // Replay same
                PlaybackMode::Order => {
                    if let Some(idx) = current_idx {
                        if idx + 1 < list.len() { Some(idx + 1) } else { None }
                    } else {
                        Some(0)
                    }
                },
                PlaybackMode::ListLoop => {
                    if let Some(idx) = current_idx {
                        Some((idx + 1) % list.len())
                    } else {
                        Some(0)
                    }
                }
            };

            let next_path = if let Some(idx) = next_idx {
                list.get(idx).map(|item| item.path.clone())
            } else {
                None
            };
            
            // Release lock before playing to avoid deadlocks or borrow issues
            drop(data); 

            if let Some(path) = next_path {
                self.play_file(path);
            } else if next_idx.is_none() {
                self.is_playing = false;
                self.player_status = PlayerStatus::Finished;
            }
        }
    }
}

impl eframe::App for MusicPlayerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply UI Settings
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = self.ui_bg_color;
        style.visuals.window_fill = egui::Color32::TRANSPARENT; // Ensure transparent background for viewports
        ctx.set_style(style);

        // Handle Audio Events
        while let Ok(status) = self.audio_rx.try_recv() {
            match status {
                AudioStatus::Status { position, duration, is_playing } => {
                    if !self.is_seeking {
                        if let Some(target) = self.seek_target {
                            let diff = if position > target { position - target } else { target - position };
                            if diff < Duration::from_secs(1) {
                                self.seek_target = None;
                                self.current_position = position;
                                self.last_sync_time = Some(Instant::now());
                            }
                        } else {
                            self.current_position = position;
                            self.last_sync_time = Some(Instant::now());
                        }
                    }
                    self.total_duration = duration;
                    self.is_playing = is_playing;
                }
                AudioStatus::Finished => {
                    self.play_next();
                }
                AudioStatus::Started(path) => {
                    self.current_playing_file = Some(path.clone());
                    let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                    self.player_status = PlayerStatus::Playing(file_name);
                    self.is_playing = true;
                    self.last_sync_time = Some(Instant::now());
                    self.current_position = Duration::from_secs(0);
                    
                    // Load Lyrics
                    self.lyrics.clear();
                    self.lyrics_state.lock().unwrap().current_lyric.clear();
                    let lrc_path = path.with_extension("lrc");
                    if lrc_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(lrc_path) {
                            self.lyrics = parse_lrc(&content);
                        }
                    }
                }
            }
        }
        
        // Calculate current display position
        let mut display_pos = self.current_position;
        if self.is_playing && !self.is_seeking && self.seek_target.is_none() {
            if let Some(last_time) = self.last_sync_time {
                let elapsed = last_time.elapsed();
                display_pos += elapsed;
                if display_pos > self.total_duration {
                    display_pos = self.total_duration;
                }
            }
        }

        // Update Lyrics
        if !self.lyrics.is_empty() {
            let mut new_lyric = "";
            for (time, text) in &self.lyrics {
                if *time <= display_pos {
                    new_lyric = text;
                } else {
                    break;
                }
            }
            
            let mut state = self.lyrics_state.lock().unwrap();
            if state.current_lyric != new_lyric {
                state.current_lyric = new_lyric.to_string();
            }
        }

        // Desktop Lyrics Window
        let show_desktop_lyrics = self.lyrics_state.lock().unwrap().show_desktop_lyrics;
        if show_desktop_lyrics {
            let lyrics_viewport_id = egui::ViewportId::from_hash_of("lyrics_viewport");
            let lyrics_state = self.lyrics_state.clone();
            ctx.show_viewport_deferred(
                lyrics_viewport_id,
                egui::ViewportBuilder::default()
                    .with_title("Lyrics")
                    .with_inner_size([800.0, 100.0])
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_resizable(true),
                move |ctx, _class| {
                    let mut state = lyrics_state.lock().unwrap();

                    // Use Frame::NONE to ensure no default background/shadow/stroke is drawn
                    let frame = egui::Frame::NONE;

                    egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                        let rect = ui.max_rect();
                        
                        // Manually paint background ONLY if alpha > 0
                        if state.bg_color.a() > 0 {
                            ui.painter().rect_filled(rect, 0.0, state.bg_color);
                        }
                        
                        // Draw text
                        ui.centered_and_justified(|ui| {
                             ui.label(egui::RichText::new(&state.current_lyric)
                                .size(state.font_size)
                                .color(state.text_color)
                                .strong());
                        });

                        // Close button (top-right)
                        let close_btn_size = 24.0;
                        let close_rect = egui::Rect::from_min_size(
                            rect.right_top() + egui::vec2(-close_btn_size - 5.0, 5.0), 
                            egui::vec2(close_btn_size, close_btn_size)
                        );
                        
                        // Handle drag for the whole window
                        let response = ui.interact(rect, ui.id(), egui::Sense::drag());
                        if response.drag_started() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }

                        // Custom Close Button
                        let close_id = ui.id().with("close_btn");
                        let close_response = ui.interact(close_rect, close_id, egui::Sense::click());
                        
                        // Draw button background and icon
                        let hovered = close_response.hovered();
                        
                        // Icon: Faint normally (20), white on hover (255)
                        let stroke_alpha = if hovered { 255 } else { 20 };
                        let stroke_color = egui::Color32::from_white_alpha(stroke_alpha);
                        
                        // Paint 'X' shape manually
                        let painter = ui.painter();
                        let center = close_rect.center();
                        let radius = close_btn_size / 3.5;
                        
                        let stroke = egui::Stroke::new(2.0, stroke_color);
                        painter.line_segment([center + egui::vec2(-radius, -radius), center + egui::vec2(radius, radius)], stroke);
                        painter.line_segment([center + egui::vec2(radius, -radius), center + egui::vec2(-radius, radius)], stroke);

                        if close_response.clicked() {
                            state.show_desktop_lyrics = false;
                        }
                    });
                    
                    if ctx.input(|i| i.viewport().close_requested()) {
                        state.show_desktop_lyrics = false;
                    }
                }
            );
        }
        
        // Request repaint for smooth progress bar and lyrics
        if self.is_playing || show_desktop_lyrics {
            ctx.request_repaint();
        }
        if show_desktop_lyrics {
            ctx.request_repaint_of(egui::ViewportId::from_hash_of("lyrics_viewport"));
        }

        // Status Bar (Bottom)
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let status_label = match self.language {
                    Language::Chinese => "状态",
                    Language::English => "Status",
                };
                let status_text = match &self.player_status {
                    PlayerStatus::Ready => match self.language {
                        Language::Chinese => "就绪".to_string(),
                        Language::English => "Ready".to_string(),
                    },
                    PlayerStatus::Playing(name) => match self.language {
                        Language::Chinese => format!("正在播放: {}", name),
                        Language::English => format!("Playing: {}", name),
                    },
                    PlayerStatus::Finished => match self.language {
                        Language::Chinese => "播放结束".to_string(),
                        Language::English => "Playback Finished".to_string(),
                    },
                    PlayerStatus::Stopped => match self.language {
                        Language::Chinese => "已停止".to_string(),
                        Language::English => "Stopped".to_string(),
                    },
                    PlayerStatus::Paused => match self.language {
                        Language::Chinese => "已暂停".to_string(),
                        Language::English => "Paused".to_string(),
                    },
                };
                let full_text = format!("{}: {}", status_label, status_text);
                let available_width = ui.available_width();
                scroll_label(ui, &full_text, available_width.max(100.0), true, ui.input(|i| i.time));
            });
            ui.horizontal(|ui| {
                let port_label = match self.language {
                    Language::Chinese => "API 端口:",
                    Language::English => "API Port:",
                };
                ui.label(port_label);
                ui.add(egui::TextEdit::singleline(&mut self.port_input).desired_width(50.0));
                let apply_label = match self.language {
                    Language::Chinese => "应用",
                    Language::English => "Apply",
                };
                if ui.button(apply_label).clicked() {
                    if let Ok(new_port) = self.port_input.parse::<u16>() {
                        if new_port != self.api_port {
                            self.api_port = new_port;
                            let state = self.lyrics_state.lock().unwrap();
                            let config = AppConfig { 
                                port: new_port,
                                lyrics_font_size: state.font_size,
                                lyrics_text_color: state.text_color.to_array(),
                                lyrics_bg_color: state.bg_color.to_array(),
                                ui_bg_color: self.ui_bg_color.to_array(),
                            };
                            config.save();
                            let _ = self.port_tx.send(new_port);
                        }
                    }
                }
                let current_label = match self.language {
                    Language::Chinese => "当前",
                    Language::English => "Current",
                };
                ui.label(format!("({}: {})", current_label, self.api_port));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Rweb Music Player");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    egui::ComboBox::from_id_salt("lang_selector")
                        .selected_text(self.language.as_str())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.language, Language::Chinese, "中文");
                            ui.selectable_value(&mut self.language, Language::English, "English");
                        });
                    
                    let lyrics_label = match self.language {
                        Language::Chinese => "桌面歌词",
                        Language::English => "Desktop Lyrics",
                    };
                    let mut state = self.lyrics_state.lock().unwrap();
                    if ui.selectable_label(state.show_desktop_lyrics, lyrics_label).clicked() {
                        state.show_desktop_lyrics = !state.show_desktop_lyrics;
                    }
                    
                    if ui.button("⚙").on_hover_text(match self.language {
                        Language::Chinese => "歌词设置",
                        Language::English => "Lyrics Settings",
                    }).clicked() {
                        self.show_lyrics_settings = !self.show_lyrics_settings;
                    }
                });
            });

            // Playback Controls
            ui.horizontal(|ui| {
                let play_label = if self.is_playing { 
                    match self.language {
                        Language::Chinese => "⏸ 暂停",
                        Language::English => "⏸ Pause",
                    }
                } else { 
                    match self.language {
                        Language::Chinese => "▶ 播放",
                        Language::English => "▶ Play",
                    }
                };
                if ui.button(play_label).clicked() {
                    if self.is_playing {
                        let _ = self.audio_tx.send(AudioCommand::Pause);
                        self.is_playing = false; // Immediate feedback
                        self.player_status = PlayerStatus::Paused;
                        self.last_sync_time = None;
                    } else {
                        if self.current_playing_file.is_some() {
                            let _ = self.audio_tx.send(AudioCommand::Resume);
                            self.is_playing = true;
                            if let Some(path) = &self.current_playing_file {
                                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                self.player_status = PlayerStatus::Playing(name);
                            }
                            self.last_sync_time = Some(Instant::now());
                        } else {
                            // Try play first in list
                            self.play_next();
                        }
                    }
                }
                let stop_label = match self.language {
                    Language::Chinese => "⏹ 停止",
                    Language::English => "⏹ Stop",
                };
                if ui.button(stop_label).clicked() {
                    let _ = self.audio_tx.send(AudioCommand::Stop);
                    self.current_position = Duration::from_secs(0);
                    self.is_playing = false;
                    self.player_status = PlayerStatus::Stopped;
                    self.last_sync_time = None;
                }
                
                // Mode Selector
                egui::ComboBox::from_id_salt("mode_selector")
                    .selected_text(self.playback_mode.as_str(self.language))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.playback_mode, PlaybackMode::Order, PlaybackMode::Order.as_str(self.language));
                        ui.selectable_value(&mut self.playback_mode, PlaybackMode::ListLoop, PlaybackMode::ListLoop.as_str(self.language));
                        ui.selectable_value(&mut self.playback_mode, PlaybackMode::SingleLoop, PlaybackMode::SingleLoop.as_str(self.language));
                        ui.selectable_value(&mut self.playback_mode, PlaybackMode::Single, PlaybackMode::Single.as_str(self.language));
                    });
            });

            // Main Window Lyrics Display
            let state = self.lyrics_state.lock().unwrap();
            if !state.current_lyric.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(&state.current_lyric)
                        .size(18.0)
                        .color(state.text_color)
                        .strong());
                });
            }
            drop(state);

            // Progress Bar
            ui.horizontal(|ui| {
                // Calculate display time (interpolated)
                let mut display_pos = self.current_position;
                if self.is_playing && !self.is_seeking && self.seek_target.is_none() {
                    if let Some(last_time) = self.last_sync_time {
                        let elapsed = last_time.elapsed();
                        display_pos += elapsed;
                        if display_pos > self.total_duration {
                            display_pos = self.total_duration;
                        }
                    }
                }
                // If seeking, show the slider value (handled by slider itself mostly, but we need to init it)
                // Actually, if we bind the slider to a variable, that variable updates.
                // We should use a separate variable for the slider interaction to avoid fighting with updates.
                
                let format_time = |d: Duration| {
                    let seconds = d.as_secs();
                    format!("{:02}:{:02}", seconds / 60, seconds % 60)
                };
                
                ui.label(format_time(display_pos));
                
                let mut value = display_pos.as_secs_f32();
                let max = self.total_duration.as_secs_f32().max(0.1); // Avoid div by zero
                
                let slider = egui::Slider::new(&mut value, 0.0..=max)
                    .show_value(false)
                    .text("");
                
                let response = ui.add(slider);
                
                if response.drag_started() {
                    self.is_seeking = true;
                }
                if response.changed() {
                    // When dragging, we update current_position to reflect drag
                    // But we shouldn't update last_sync_time because we are overriding auto-update
                    self.current_position = Duration::from_secs_f32(value);
                    // Disable interpolation while dragging
                    self.last_sync_time = None; 
                }
                if response.drag_stopped() {
                    let target = Duration::from_secs_f32(value);
                    let _ = self.audio_tx.send(AudioCommand::Seek(target));
                    self.seek_target = Some(target);
                    self.is_seeking = false;
                    // Don't enable interpolation yet, wait for sync
                }
                
                ui.label(format_time(self.total_duration));
            });

            // Volume Control
            ui.horizontal(|ui| {
                let vol_label = match self.language {
                    Language::Chinese => "音量",
                    Language::English => "Volume",
                };
                ui.label(vol_label);
                if ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0)).changed() {
                    let _ = self.audio_tx.send(AudioCommand::SetVolume(self.volume));
                }
            });

            ui.separator();

            // Playlist Management
            let mut data = self.data.lock().unwrap();
            
            ui.horizontal(|ui| {
                let playlist_label = match self.language {
                    Language::Chinese => "当前歌单:",
                    Language::English => "Playlist:",
                };
                ui.label(playlist_label);
                egui::ComboBox::from_id_salt("playlist_selector")
                    .selected_text(&data.current_name)
                    .show_ui(ui, |ui| {
                        for name in data.lists.keys().cloned().collect::<Vec<_>>() {
                            if ui.selectable_value(&mut data.current_name, name.clone(), &name).clicked() {
                                data.save();
                            }
                        }
                    });
                
                if ui.button("✏").on_hover_text(match self.language {
                    Language::Chinese => "重命名当前歌单",
                    Language::English => "Rename current playlist",
                }).clicked() {
                    self.rename_playlist_name = data.current_name.clone();
                    self.show_rename_dialog = true;
                }

                if ui.button("🗑").on_hover_text(match self.language {
                    Language::Chinese => "删除当前歌单",
                    Language::English => "Delete current playlist",
                }).clicked() {
                    if data.lists.len() > 1 {
                        self.playlist_to_delete = Some(data.current_name.clone());
                        self.show_delete_playlist_dialog = true;
                    }
                }
            });

            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.new_playlist_name);
                let new_playlist_label = match self.language {
                    Language::Chinese => "新建歌单",
                    Language::English => "New Playlist",
                };
                if ui.button(new_playlist_label).clicked() {
                    if !self.new_playlist_name.is_empty() {
                        data.lists.entry(self.new_playlist_name.clone()).or_default();
                        data.current_name = self.new_playlist_name.clone();
                        self.new_playlist_name.clear();
                        data.save();
                    }
                }
            });

            ui.separator();

            // Drop lock before file dialog to avoid deadlock
            drop(data);

            // File Management
            ui.horizontal(|ui| {
                let add_file_label = match self.language {
                    Language::Chinese => "添加文件",
                    Language::English => "Add Files",
                };
                if ui.button(add_file_label).clicked() {
                    if let Some(paths) = rfd::FileDialog::new().pick_files() {
                        let mut data = self.data.lock().unwrap();
                        let current_name = data.current_name.clone();
                        let list = data.lists.entry(current_name.clone()).or_default();
                        
                        let mut duplicates = Vec::new();
                        let mut non_duplicates = Vec::new();
                        
                        for path in paths {
                            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            if list.iter().any(|item| item.name == name) {
                                duplicates.push(path);
                            } else {
                                non_duplicates.push(path);
                            }
                        }
                        
                        // Add non-duplicates immediately
                        for path in non_duplicates {
                            let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            list.insert(0, PlaylistItem { path, name });
                        }
                        
                        if !duplicates.is_empty() {
                            self.pending_files = duplicates;
                            self.show_duplicate_dialog = true;
                        }
                        
                        data.save();
                    }
                }
                let clear_list_label = match self.language {
                    Language::Chinese => "清空当前列表",
                    Language::English => "Clear List",
                };
                if ui.button(clear_list_label).clicked() {
                    let mut data = self.data.lock().unwrap();
                    let current_name = data.current_name.clone();
                    if let Some(list) = data.lists.get_mut(&current_name) {
                        list.clear();
                    }
                    data.save();
                }
            });

            // Re-acquire lock for display
            let mut data = self.data.lock().unwrap();

            let list_content_label = match self.language {
                Language::Chinese => format!("列表内容 ({}) :", data.current_name),
                Language::English => format!("Playlist Content ({}) :", data.current_name),
            };
            ui.label(list_content_label);
            
            // Display Playlist
            let current_list = data.lists.get(&data.current_name).cloned().unwrap_or_default();
            let mut file_to_play = None;
            let mut item_to_delete = None;

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (index, item) in current_list.iter().enumerate() {
                    let is_current = Some(&item.path) == self.current_playing_file.as_ref();
                    let exists = item.path.exists();
                    
                    ui.horizontal(|ui| {
                        let text = format!("{}. {}", index, item.name);
                        
                        // Custom selectable label with scrolling
                        let available_width = ui.available_width();
                        let (rect, response) = ui.allocate_exact_size(egui::vec2(available_width, 20.0), egui::Sense::click());
                        let is_hovered = response.hovered();

                        if response.clicked() {
                            if exists {
                                file_to_play = Some(item.path.clone());
                            }
                        }

                        // Draw background
                        if is_current {
                            ui.painter().rect_filled(rect, 2.0, ui.visuals().selection.bg_fill);
                        } else if is_hovered {
                            ui.painter().rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
                        }

                        // Draw text
                        let text_color = if !exists {
                            egui::Color32::RED
                        } else if is_current {
                            ui.visuals().strong_text_color()
                        } else {
                            ui.visuals().text_color()
                        };
                        
                        // Use a child UI to clip and scroll
                        let child_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
                        // We need to manually implement scrolling here because scroll_label allocates its own rect
                        // which might conflict or be easier to just inline.
                        
                        let font_id = egui::FontId::proportional(14.0);
                        let text_galley = child_ui.painter().layout_no_wrap(text.clone(), font_id, text_color);
                        let text_width = text_galley.size().x;

                        // When playback is paused, the app may not repaint continuously.
                        // For hover-scrolling labels, request a repaint while hovered so the animation keeps running.
                        if is_hovered && text_width > rect.width() {
                            ctx.request_repaint();
                        }
                        
                        if text_width <= rect.width() || !is_hovered {
                            // Center vertically
                            let pos = rect.min + egui::vec2(0.0, (rect.height() - text_galley.size().y) / 2.0);
                            child_ui.painter().galley(pos, text_galley, egui::Color32::TRANSPARENT);
                        } else {
                            let speed = 30.0; 
                            let gap = 50.0;
                            let cycle_len = text_width + gap;
                            let time = child_ui.input(|i| i.time);
                            let offset = (time * speed as f64) % cycle_len as f64;
                            
                            child_ui.painter().with_clip_rect(rect).add(egui::Shape::Text(egui::epaint::TextShape {
                                pos: rect.min + egui::vec2(-offset as f32, (rect.height() - text_galley.size().y) / 2.0),
                                galley: text_galley.clone(),
                                underline: egui::Stroke::NONE,
                                override_text_color: None,
                                angle: 0.0,
                                fallback_color: egui::Color32::WHITE,
                                opacity_factor: 1.0,
                            }));
                            
                            if offset as f32 + rect.width() > text_width + gap {
                                 child_ui.painter().with_clip_rect(rect).add(egui::Shape::Text(egui::epaint::TextShape {
                                    pos: rect.min + egui::vec2(-offset as f32 + cycle_len as f32, (rect.height() - text_galley.size().y) / 2.0),
                                    galley: text_galley,
                                    underline: egui::Stroke::NONE,
                                    override_text_color: None,
                                    angle: 0.0,
                                    fallback_color: egui::Color32::WHITE,
                                    opacity_factor: 1.0,
                                }));
                            }
                        }
                        
                        if !exists {
                            let not_exist_text = match self.language {
                                Language::Chinese => format!("文件不存在: {:?}", item.path),
                                Language::English => format!("File not found: {:?}", item.path),
                            };
                            response.clone().on_hover_text(not_exist_text);
                        }

                        response.context_menu(|ui| {
                            let remove_label = match self.language {
                                Language::Chinese => "从列表中删除",
                                Language::English => "Remove from list",
                            };
                            if ui.button(remove_label).clicked() {
                                item_to_delete = Some(index);
                                ui.close();
                            }
                        });
                    });
                }
            });
            
            if let Some(index) = item_to_delete {
                let current_name = data.current_name.clone();
                if let Some(list) = data.lists.get_mut(&current_name) {
                    if index < list.len() {
                        list.remove(index);
                        data.save();
                    }
                }
            }
            
            drop(data); // Release lock
            
            if let Some(path) = file_to_play {
                self.play_file(path);
            }
        });

        if self.show_duplicate_dialog {
            let title = match self.language {
                Language::Chinese => "发现同名文件",
                Language::English => "Duplicate Files Found",
            };
            let mut open = true;
            let mut should_close = false;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    let msg = match self.language {
                        Language::Chinese => format!("发现 {} 个同名文件，是否重命名并添加？", self.pending_files.len()),
                        Language::English => format!("Found {} duplicate files. Rename and add?", self.pending_files.len()),
                    };
                    ui.label(msg);
                    ui.horizontal(|ui| {
                        let add_rename_label = match self.language {
                            Language::Chinese => "添加并重命名",
                            Language::English => "Add & Rename",
                        };
                        if ui.button(add_rename_label).clicked() {
                            let mut data = self.data.lock().unwrap();
                            let current_name = data.current_name.clone();
                            let list = data.lists.entry(current_name).or_default();
                            
                            for path in self.pending_files.drain(..) {
                                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                                let mut final_name = name.clone();
                                let mut count = 1;
                                while list.iter().any(|item| item.name == final_name) {
                                    final_name = format!("{} ({})", name, count);
                                    count += 1;
                                }
                                list.insert(0, PlaylistItem { path, name: final_name });
                            }
                            data.save();
                            should_close = true;
                        }
                        let cancel_label = match self.language {
                            Language::Chinese => "取消",
                            Language::English => "Cancel",
                        };
                        if ui.button(cancel_label).clicked() {
                            self.pending_files.clear();
                            should_close = true;
                        }
                    });
                });
            if !open || should_close {
                self.show_duplicate_dialog = false;
            }
        }

        if self.show_lyrics_settings {
            let title = match self.language {
                Language::Chinese => "歌词设置",
                Language::English => "Lyrics Settings",
            };
            let mut open = true;
            let mut should_close = false;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(true)
                .default_size([300.0, 300.0])
                .frame(egui::Frame::window(ctx.style().as_ref()).fill(self.ui_bg_color))
                .open(&mut open)
                .show(ctx, |ui| {
                    let mut state = self.lyrics_state.lock().unwrap();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading(match self.language {
                            Language::Chinese => "歌词设置",
                            Language::English => "Lyrics Settings",
                        });
                        egui::Grid::new("lyrics_settings_grid").num_columns(2).show(ui, |ui| {
                            ui.label(match self.language {
                                Language::Chinese => "字体大小",
                                Language::English => "Font Size",
                            });
                            ui.add(egui::Slider::new(&mut state.font_size, 10.0..=100.0));
                            ui.end_row();

                            ui.label(match self.language {
                                Language::Chinese => "字体颜色",
                                Language::English => "Text Color",
                            });
                            ui.color_edit_button_srgba(&mut state.text_color);
                            ui.end_row();

                            ui.label(match self.language {
                                Language::Chinese => "背景颜色",
                                Language::English => "Background Color",
                            });
                            ui.color_edit_button_srgba(&mut state.bg_color);
                            ui.end_row();
                        });

                        ui.separator();
                        
                        ui.heading(match self.language {
                            Language::Chinese => "界面设置",
                            Language::English => "UI Settings",
                        });
                        ui.label(match self.language {
                            Language::Chinese => "主界面背景",
                            Language::English => "Main UI Background",
                        });
                        ui.color_edit_button_srgba(&mut self.ui_bg_color);

                        ui.add_space(10.0);

                        ui.horizontal(|ui| {
                            if ui.button(match self.language {
                                Language::Chinese => "保存设置",
                                Language::English => "Save Settings",
                            }).clicked() {
                                let mut config = AppConfig::load();
                                config.lyrics_font_size = state.font_size;
                                config.lyrics_text_color = state.text_color.to_array();
                                config.lyrics_bg_color = state.bg_color.to_array();
                                config.ui_bg_color = self.ui_bg_color.to_array();
                                if let Ok(file) = File::create(get_config_path("config.json")) {
                                    let _ = serde_json::to_writer_pretty(file, &config);
                                }
                                should_close = true;
                            }
                            if ui.button(match self.language {
                                Language::Chinese => "关闭",
                                Language::English => "Close",
                            }).clicked() {
                                should_close = true;
                            }
                        });
                    });
                });
            if !open || should_close {
                self.show_lyrics_settings = false;
            }
        }

        if self.show_rename_dialog {
            let title = match self.language {
                Language::Chinese => "重命名歌单",
                Language::English => "Rename Playlist",
            };
            let mut open = true;
            let mut should_close = false;
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.rename_playlist_name);
                    ui.horizontal(|ui| {
                        let ok_label = match self.language {
                            Language::Chinese => "确定",
                            Language::English => "OK",
                        };
                        if ui.button(ok_label).clicked() {
                            if !self.rename_playlist_name.is_empty() {
                                let mut data = self.data.lock().unwrap();
                                let old_name = data.current_name.clone();
                                if !data.lists.contains_key(&self.rename_playlist_name) {
                                    if let Some(list) = data.lists.remove(&old_name) {
                                        data.lists.insert(self.rename_playlist_name.clone(), list);
                                        data.current_name = self.rename_playlist_name.clone();
                                        data.save();
                                    }
                                }
                            }
                            should_close = true;
                        }
                        let cancel_label = match self.language {
                            Language::Chinese => "取消",
                            Language::English => "Cancel",
                        };
                        if ui.button(cancel_label).clicked() {
                            should_close = true;
                        }
                    });
                });
            if !open || should_close {
                self.show_rename_dialog = false;
            }
        }

        if self.show_delete_playlist_dialog {
             let title = match self.language {
                 Language::Chinese => "确认删除",
                 Language::English => "Confirm Delete",
             };
             let mut open = true;
             let mut should_close = false;
             egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    if let Some(name) = &self.playlist_to_delete {
                        let msg = match self.language {
                            Language::Chinese => format!("确定要删除歌单 '{}' 吗？", name),
                            Language::English => format!("Are you sure you want to delete playlist '{}'?", name),
                        };
                        ui.label(msg);
                    }
                    ui.horizontal(|ui| {
                        let ok_label = match self.language {
                            Language::Chinese => "确定",
                            Language::English => "OK",
                        };
                        if ui.button(ok_label).clicked() {
                            if let Some(name) = &self.playlist_to_delete {
                                let mut data = self.data.lock().unwrap();
                                data.lists.remove(name);
                                // If we deleted the current one, switch to another
                                if data.current_name == *name {
                                    if let Some(first) = data.lists.keys().next().cloned() {
                                        data.current_name = first;
                                    } else {
                                        data.lists.insert("Default List".to_string(), Vec::new());
                                        data.current_name = "Default List".to_string();
                                    }
                                }
                                data.save();
                            }
                            should_close = true;
                            self.playlist_to_delete = None;
                        }
                        let cancel_label = match self.language {
                            Language::Chinese => "取消",
                            Language::English => "Cancel",
                        };
                        if ui.button(cancel_label).clicked() {
                            should_close = true;
                            self.playlist_to_delete = None;
                        }
                    });
                });
            if !open || should_close {
                self.show_delete_playlist_dialog = false;
            }
        }
    }
}

fn main() -> eframe::Result<()> {
    // 1. Load Config
    let config = AppConfig::load();
    let port = config.port;
    let (port_tx, mut port_rx) = mpsc::unbounded_channel::<u16>();

    // 2. Start Audio Thread
    let (audio_tx, audio_rx) = start_audio_thread();

    // 3. Shared State (Load from file)
    let playlists_manager = PlaylistsManager::load();
    let data = Arc::new(Mutex::new(playlists_manager));
    
    let app_state = AppState {
        audio_tx: audio_tx.clone(),
        data: data.clone(),
    };

    // 4. Start API Server in a separate thread
    thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            let mut active_port = port;
            loop {
                let app = Router::new()
                    .route("/play", post(api_play))
                    .route("/playlist", get(api_get_playlist))
                    .route("/playlist/remove", post(api_remove_from_playlist))
                    .route("/playlist/rename", post(api_rename_playlist))
                    .route("/playlist/delete", post(api_delete_playlist))
                    .route("/playlist/switch", post(api_switch_playlist))
                    .with_state(app_state.clone());

                let addr = format!("0.0.0.0:{}", active_port);
                match tokio::net::TcpListener::bind(&addr).await {
                    Ok(listener) => {
                        println!("API Server listening on port {}", active_port);
                        let server = axum::serve(listener, app);
                        
                        // Run server until a new port is received
                        tokio::select! {
                            _ = server.into_future() => {
                                break; // Server exited unexpectedly
                            }
                            new_port_opt = port_rx.recv() => {
                                if let Some(new_port) = new_port_opt {
                                    println!("Switching to port {}", new_port);
                                    active_port = new_port;
                                } else {
                                    break; // Channel closed
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to bind port {}: {}", active_port, e);
                        // Wait for new port if bind failed
                        if let Some(new_port) = port_rx.recv().await {
                            active_port = new_port;
                        } else {
                            break;
                        }
                    }
                }
            }
        });
    });

    // 5. Run UI
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([400.0, 600.0])
            .with_transparent(true),
        ..Default::default()
    };
    
    eframe::run_native(
        "Music Player",
        options,
        Box::new(move |cc| Ok(Box::new(MusicPlayerApp::new(audio_tx, audio_rx, data, port, port_tx, cc)))),
    )
}
