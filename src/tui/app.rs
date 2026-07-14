use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};

use crate::fetch::Chart;
use crate::models::{AsciiArt, ChartRow, TrendResponse};

/// 入力モード
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
}

/// 画面
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// ランキング一覧（chartでTOP100/일간を区別）
    List,
    /// 曲情報（chartでグリッド/일간テーブルを区別）
    SongDetail,
}

/// アルバムアート取得スレッドからの結果
pub type ArtMsg = (String, AsciiArt);
/// trend取得スレッドからの結果 (cache_key, song_id, result)
pub type TrendMsg = (String, String, Result<TrendResponse, String>);

/// 一覧に戻るためのスクロール位置保存
#[derive(Debug, Clone, Copy)]
pub struct ListPos {
    pub chart: Chart,
    pub index: usize,
    pub offset: usize,
}

pub struct App {
    pub running: bool,
    pub tick: u64,

    pub screen: Screen,
    pub mode: Mode,
    pub chart: Chart,

    // 一覧データ（起動時取得、rで再取得）
    pub top100_rows: Vec<ChartRow>,
    pub daily_rows: Vec<ChartRow>,
    pub top100_time: Option<String>,
    pub daily_time: Option<String>,

    // 一覧のスクロール状態
    pub list_index: usize,
    pub list_offset: usize,
    pub visible_rows: usize,

    // 曲情報
    pub detail_song_id: Option<String>,
    pub detail_offset: usize,
    pub list_pos_stack: Vec<ListPos>,

    // trendキャッシュ（key = "chart:song_id"）
    pub trend_cache: HashMap<String, TrendResponse>,
    pub trend_pending: bool,
    pub trend_rx: Option<Receiver<TrendMsg>>,

    // Spotify URLキャッシュ（key = song_id、None = Spotify無し）
    pub spotify_cache: HashMap<String, Option<String>>,
    /// c押下でtrend取得待ちの曲（取得後に再生する）
    pub pending_play_song: Option<String>,
    pub spotify_url: String,
    pub spotify_playing: bool,
    pub spotify_rx: Option<Receiver<Result<String, String>>>,

    // アルバムアート
    pub album_art_cache: HashMap<String, AsciiArt>,
    pub album_art_requested: HashSet<String>,
    pub art_tx: Sender<ArtMsg>,
    pub art_rx: Receiver<ArtMsg>,

    // チョード（gg）
    pub pending_g: bool,

    // 検索
    pub input_buffer: String,
    pub search_query: String,
    pub search_matches: Vec<usize>,
    pub search_pos: usize,

    // ステータス表示
    pub message: Option<String>,
    pub error: Option<String>,
    pub loading: bool,
}

impl App {
    pub fn new() -> Self {
        let (art_tx, art_rx) = std::sync::mpsc::channel();
        App {
            running: true,
            tick: 0,
            screen: Screen::List,
            mode: Mode::Normal,
            chart: Chart::Top100,
            top100_rows: Vec::new(),
            daily_rows: Vec::new(),
            top100_time: None,
            daily_time: None,
            list_index: 0,
            list_offset: 0,
            visible_rows: 10,
            detail_song_id: None,
            detail_offset: 0,
            list_pos_stack: Vec::new(),
            trend_cache: HashMap::new(),
            trend_pending: false,
            trend_rx: None,
            spotify_cache: HashMap::new(),
            pending_play_song: None,
            spotify_url: String::new(),
            spotify_playing: false,
            spotify_rx: None,
            pending_g: false,
            album_art_cache: HashMap::new(),
            album_art_requested: HashSet::new(),
            art_tx,
            art_rx,
            input_buffer: String::new(),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_pos: 0,
            message: None,
            error: None,
            loading: false,
        }
    }

    /// 現在のチャートの一覧行
    pub fn rows(&self) -> &[ChartRow] {
        match self.chart {
            Chart::Top100 => &self.top100_rows,
            Chart::Daily => &self.daily_rows,
        }
    }

    /// 現在のチャートのスナップショット時刻
    pub fn chart_time(&self) -> Option<&str> {
        match self.chart {
            Chart::Top100 => self.top100_time.as_deref(),
            Chart::Daily => self.daily_time.as_deref(),
        }
        .filter(|s| !s.is_empty())
    }

    /// trendキャッシュキー
    pub fn trend_key(chart: Chart, song_id: &str) -> String {
        let c = match chart {
            Chart::Top100 => "top100",
            Chart::Daily => "daily",
        };
        format!("{}:{}", c, song_id)
    }

    /// 現在表示中の曲trend
    pub fn current_trend(&self) -> Option<&TrendResponse> {
        let id = self.detail_song_id.as_ref()?;
        self.trend_cache.get(&Self::trend_key(self.chart, id))
    }

    pub fn show_message(&mut self, msg: &str) {
        self.message = Some(msg.to_string());
        self.error = None;
    }

    pub fn show_error(&mut self, msg: &str) {
        self.error = Some(msg.to_string());
        self.message = None;
    }

    /// 現在の一覧選択行の曲ID
    pub fn selected_song_id(&self) -> Option<String> {
        self.rows().get(self.list_index).map(|r| r.song.id.clone())
    }
}
