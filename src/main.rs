mod fetch;
mod models;
mod tui;

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use fetch::Chart;
use tui::app::App;
use tui::input::{ensure_older_trend, handle_key, process_older_trend_result, process_trend_result};
use tui::ui::draw;

fn main() -> Result<()> {
    let mut app = App::new();

    // 起動時にチャートを取得
    eprintln!("Loading Melon charts...");
    match fetch::fetch_chart(Chart::Top100) {
        Ok(d) => {
            app.top100_time = d.time.clone();
            app.top100_rows = d.data;
        }
        Err(e) => eprintln!("TOP100 fetch failed: {}", e),
    }
    match fetch::fetch_chart(Chart::Daily) {
        Ok(d) => {
            app.daily_time = d.time.clone();
            app.daily_rows = d.data;
        }
        Err(e) => eprintln!("Daily fetch failed: {}", e),
    }

    // ターミナル初期化
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app);

    // ターミナル復元
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    while app.running {
        terminal.draw(|f| draw(f, app))?;
        app.tick = app.tick.wrapping_add(1);

        // アルバムアート受信（全件ドレイン）
        loop {
            match app.art_rx.try_recv() {
                Ok((url, art)) => {
                    app.album_art_cache.insert(url, art);
                }
                Err(_) => break,
            }
        }

        // trend受信
        if let Some(rx) = app.trend_rx.take() {
            match rx.try_recv() {
                Ok((key, song_id, res)) => {
                    process_trend_result(app, key, song_id, res);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    app.trend_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    app.trend_pending = false;
                }
            }
        }

        // TOP100 trend過去ページ受信
        if let Some(rx) = app.older_rx.take() {
            match rx.try_recv() {
                Ok((key, oldest, res)) => {
                    process_older_trend_result(app, key, oldest, res);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    app.older_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    app.older_pending = false;
                }
            }
        }
        ensure_older_trend(app);

        // Spotify再生結果受信
        if let Some(rx) = app.spotify_rx.take() {
            match rx.try_recv() {
                Ok(Ok(msg)) => {
                    app.spotify_playing = true;
                    app.show_message(&msg);
                }
                Ok(Err(msg)) => {
                    app.spotify_playing = false;
                    app.show_error(&msg);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    app.spotify_rx = Some(rx);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    app.spotify_playing = false;
                }
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press {
                    handle_key(app, key);
                }
            }
        }
    }
    Ok(())
}
