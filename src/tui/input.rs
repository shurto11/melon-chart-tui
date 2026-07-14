use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::fetch::{self, Chart};

use super::app::{App, Mode, Screen};

/// pending_g（gg チョード用）はここでは App に持たず、簡易的に thread_local を使わず
/// App にフィールドを足す代わりに、モジュール内 static は使えないため App に持たせる。
/// → app.rs に pending_g を持たせている想定だが、簡潔さのため input 内で管理する。
///
/// 実際には App に pending_g フィールドを追加する。

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Ctrl+C は常に終了
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.running = false;
        return;
    }

    // キー入力があったらメッセージ/エラーをクリア（検索中は除く）
    if app.mode == Mode::Normal {
        app.message = None;
        app.error = None;
    }

    match app.mode {
        Mode::Normal => handle_normal(app, key),
        Mode::Search => handle_search(app, key),
    }
}

fn handle_normal(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    // gg チョード処理
    if app.pending_g {
        app.pending_g = false;
        if let KeyCode::Char('g') = key.code {
            go_top(app);
            return;
        }
        // g 以外は通常処理へフォールスルー
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => move_down(app, 1),
        KeyCode::Char('k') | KeyCode::Up => move_up(app, 1),
        KeyCode::Char('g') => app.pending_g = true,
        KeyCode::Char('G') => go_bottom(app),
        KeyCode::Char('d') if ctrl => {
            let half = (app.visible_rows / 2).max(1);
            move_down(app, half);
        }
        KeyCode::Char('u') if ctrl => {
            let half = (app.visible_rows / 2).max(1);
            move_up(app, half);
        }
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => enter_detail(app),
        KeyCode::Char('h') | KeyCode::Left => go_back(app),
        KeyCode::Char('[') => toggle_chart(app),
        KeyCode::Char(']') => toggle_chart(app),
        KeyCode::Char('/') => {
            if app.screen == Screen::List {
                app.mode = Mode::Search;
                app.input_buffer.clear();
            }
        }
        KeyCode::Char('n') => search_jump(app, 1),
        KeyCode::Char('N') => search_jump(app, -1),
        KeyCode::Char('c') => handle_play(app),
        KeyCode::Char('r') => reload(app),
        KeyCode::Esc => {
            app.search_matches.clear();
            app.search_query.clear();
        }
        _ => {}
    }
}

fn handle_search(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.mode = Mode::Normal;
            app.input_buffer.clear();
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
            app.search_query = app.input_buffer.to_lowercase();
            execute_search(app);
        }
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        KeyCode::Char(c) => {
            app.input_buffer.push(c);
        }
        _ => {}
    }
}

// ---- ナビゲーション ----

fn list_len(app: &App) -> usize {
    app.rows().len()
}

/// 曲情報画面のスクロール可能行数（daily=trend点数, grid=ユニーク日付数）
fn detail_len(app: &App) -> usize {
    let Some(trend) = app.current_trend() else {
        return 0;
    };
    match app.chart {
        Chart::Daily => trend.data.data.len(),
        Chart::Top100 => {
            let mut set = std::collections::HashSet::new();
            for p in &trend.data.data {
                if let Some(d) = p.time.get(0..10) {
                    set.insert(d.to_string());
                }
            }
            set.len()
        }
    }
}

fn move_down(app: &mut App, n: usize) {
    match app.screen {
        Screen::List => {
            let len = list_len(app);
            if len == 0 {
                return;
            }
            app.list_index = (app.list_index + n).min(len - 1);
        }
        Screen::SongDetail => {
            let len = detail_len(app);
            let max_off = len.saturating_sub(1);
            app.detail_offset = (app.detail_offset + n).min(max_off);
        }
    }
}

fn move_up(app: &mut App, n: usize) {
    match app.screen {
        Screen::List => {
            app.list_index = app.list_index.saturating_sub(n);
        }
        Screen::SongDetail => {
            app.detail_offset = app.detail_offset.saturating_sub(n);
        }
    }
}

fn go_top(app: &mut App) {
    match app.screen {
        Screen::List => {
            app.list_index = 0;
            app.list_offset = 0;
        }
        Screen::SongDetail => app.detail_offset = 0,
    }
}

fn go_bottom(app: &mut App) {
    match app.screen {
        Screen::List => {
            let len = list_len(app);
            if len > 0 {
                app.list_index = len - 1;
            }
        }
        Screen::SongDetail => {
            let len = detail_len(app);
            app.detail_offset = len.saturating_sub(1);
        }
    }
}

fn enter_detail(app: &mut App) {
    if app.screen != Screen::List {
        return;
    }
    let Some(song_id) = app.selected_song_id() else {
        return;
    };
    app.list_pos_stack.push(super::app::ListPos {
        chart: app.chart,
        index: app.list_index,
        offset: app.list_offset,
    });
    app.detail_song_id = Some(song_id.clone());
    app.detail_offset = 0;
    app.screen = Screen::SongDetail;
    ensure_trend(app, app.chart, &song_id);
}

fn go_back(app: &mut App) {
    if app.screen != Screen::List {
        if let Some(pos) = app.list_pos_stack.pop() {
            app.chart = pos.chart;
            app.list_index = pos.index;
            app.list_offset = pos.offset;
        }
        app.screen = Screen::List;
    }
}

fn toggle_chart(app: &mut App) {
    app.chart = match app.chart {
        Chart::Top100 => Chart::Daily,
        Chart::Daily => Chart::Top100,
    };
    match app.screen {
        Screen::List => {
            // 選択位置をクランプ、検索リセット
            let len = list_len(app);
            if len == 0 {
                app.list_index = 0;
            } else {
                app.list_index = app.list_index.min(len - 1);
            }
            app.list_offset = 0;
            app.search_matches.clear();
            app.search_query.clear();
        }
        Screen::SongDetail => {
            app.detail_offset = 0;
            if let Some(id) = app.detail_song_id.clone() {
                ensure_trend(app, app.chart, &id);
            }
        }
    }
}

// ---- 検索 ----

fn execute_search(app: &mut App) {
    app.search_matches.clear();
    app.search_pos = 0;
    if app.search_query.is_empty() {
        return;
    }
    let q = &app.search_query;
    let matches: Vec<usize> = app
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, r)| {
            r.song.name.to_lowercase().contains(q)
                || r.song.artist_names().to_lowercase().contains(q)
        })
        .map(|(i, _)| i)
        .collect();
    app.search_matches = matches;
    if let Some(&first) = app.search_matches.first() {
        app.list_index = first;
        app.show_message(&format!("{} matches", app.search_matches.len()));
    } else {
        app.show_error("No matches");
    }
}

fn search_jump(app: &mut App, dir: i32) {
    if app.search_matches.is_empty() || app.screen != Screen::List {
        return;
    }
    let n = app.search_matches.len() as i32;
    app.search_pos = (((app.search_pos as i32 + dir) % n + n) % n) as usize;
    app.list_index = app.search_matches[app.search_pos];
}

// ---- trend 取得 ----

fn ensure_trend(app: &mut App, chart: Chart, song_id: &str) {
    let key = App::trend_key(chart, song_id);
    if app.trend_cache.contains_key(&key) {
        return;
    }
    if app.trend_pending {
        return; // single-flight
    }
    let (tx, rx) = std::sync::mpsc::channel();
    app.trend_rx = Some(rx);
    app.trend_pending = true;
    let song_id = song_id.to_string();
    let key2 = key.clone();
    std::thread::spawn(move || {
        let res = fetch::fetch_trend(chart, &song_id).map_err(|e| e.to_string());
        let _ = tx.send((key2, song_id, res));
    });
}

/// メインループから呼ぶ: trend受信処理
pub fn process_trend_result(app: &mut App, key: String, song_id: String, res: Result<crate::models::TrendResponse, String>) {
    app.trend_pending = false;
    match res {
        Ok(trend) => {
            let spotify = trend.song.spotify_url();
            app.spotify_cache.insert(song_id.clone(), spotify.clone());
            app.trend_cache.insert(key, trend);
            // 再生待ちならこのタイミングで再生
            if app.pending_play_song.as_deref() == Some(song_id.as_str()) {
                app.pending_play_song = None;
                match spotify {
                    Some(url) => start_spotify_play(app, &url),
                    None => app.show_error("No Spotify URL for this song"),
                }
            }
        }
        Err(e) => {
            if app.pending_play_song.as_deref() == Some(song_id.as_str()) {
                app.pending_play_song = None;
            }
            app.show_error(&format!("Trend fetch failed: {}", e));
        }
    }
}

// ---- リロード ----

fn reload(app: &mut App) {
    app.show_message("Reloading...");
    // 同期取得（短時間の待ちを許容）
    match fetch::fetch_chart(Chart::Top100) {
        Ok(d) => {
            app.top100_time = d.time.clone();
            app.top100_rows = d.data;
        }
        Err(e) => app.show_error(&format!("TOP100 reload failed: {}", e)),
    }
    match fetch::fetch_chart(Chart::Daily) {
        Ok(d) => {
            app.daily_time = d.time.clone();
            app.daily_rows = d.data;
        }
        Err(e) => app.show_error(&format!("Daily reload failed: {}", e)),
    }
    // trendキャッシュも無効化（順位が変わっているため）
    app.trend_cache.clear();
    let len = list_len(app);
    if len > 0 {
        app.list_index = app.list_index.min(len - 1);
    } else {
        app.list_index = 0;
    }
    if app.error.is_none() {
        app.show_message("Reloaded");
    }
}

// ---- Spotify ----

/// Spotify URLをURIに変換
fn spotify_url_to_uri(url: &str) -> Option<String> {
    if url.starts_with("spotify:") {
        return Some(url.split('?').next()?.to_string());
    }
    let path = url.split('?').next()?.trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    let n = parts.len();
    if n >= 2 && !parts[n - 1].is_empty() {
        Some(format!("spotify:{}:{}", parts[n - 2], parts[n - 1]))
    } else {
        None
    }
}

fn handle_play(app: &mut App) {
    // 対象の曲ID
    let song_id = match app.screen {
        Screen::List => app.selected_song_id(),
        Screen::SongDetail => app.detail_song_id.clone(),
    };
    let Some(song_id) = song_id else {
        app.show_error("No song selected");
        return;
    };

    match app.spotify_cache.get(&song_id) {
        Some(Some(url)) => {
            let url = url.clone();
            start_spotify_play(app, &url);
        }
        Some(None) => {
            app.show_error("No Spotify URL for this song");
        }
        None => {
            // trend取得してURL解決 → 取得後に再生
            app.pending_play_song = Some(song_id.clone());
            app.show_message("Resolving Spotify...");
            ensure_trend(app, app.chart, &song_id);
        }
    }
}

/// Spotify再生を開始
fn start_spotify_play(app: &mut App, url: &str) {
    let Some(uri) = spotify_url_to_uri(url) else {
        app.show_error("Invalid Spotify URL");
        return;
    };
    app.spotify_playing = false;
    app.spotify_url = url.to_string();

    let (tx, rx) = std::sync::mpsc::channel();
    app.spotify_rx = Some(rx);

    std::thread::spawn(move || {
        let result = std::process::Command::new("spotatui")
            .args(["play", "--uri", &uri])
            .output();
        match result {
            Ok(o) if o.status.success() => {
                let _ = tx.send(Ok("Playing".to_string()));
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                let msg = if err.is_empty() {
                    String::from_utf8_lossy(&o.stdout).trim().to_string()
                } else {
                    err
                };
                let _ = tx.send(Err(if msg.is_empty() {
                    "Play failed".to_string()
                } else {
                    msg
                }));
            }
            Err(e) => {
                let _ = tx.send(Err(format!("spotatui not found: {}", e)));
            }
        }
    });
}
