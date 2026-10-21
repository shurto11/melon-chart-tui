use serde::Deserialize;

/// ASCIIアートの1ピクセル（文字 + RGB色）
pub type AsciiPixel = (char, u8, u8, u8);
/// ASCIIアート全体（行×列）
pub type AsciiArt = Vec<Vec<AsciiPixel>>;

/// アルバム情報
#[derive(Debug, Clone, Deserialize)]
pub struct Album {
    pub id: String,
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub image: Option<String>,
}

/// アーティスト情報（必要なフィールドのみ）
#[derive(Debug, Clone, Deserialize)]
pub struct Artist {
    pub name: String,
}

/// 他プラットフォームの同一曲リンク
#[derive(Debug, Clone, Deserialize)]
pub struct SameSong {
    pub platform: String,
    #[serde(default)]
    pub link: Option<String>,
}

/// 曲情報
#[derive(Debug, Clone, Deserialize)]
pub struct Song {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub album: Option<Album>,
    #[serde(default)]
    pub artists: Vec<Artist>,
    #[serde(default)]
    #[serde(rename = "sameSongs")]
    pub same_songs: Vec<SameSong>,
}

impl Song {
    /// アーティスト名を「, 」区切りで結合
    pub fn artist_names(&self) -> String {
        self.artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// アルバム名（無ければ空文字）
    pub fn album_name(&self) -> &str {
        self.album.as_ref().map(|a| a.name.as_str()).unwrap_or("")
    }

    /// アルバム画像URL（guyso CDN）
    pub fn album_image_url(&self) -> Option<String> {
        self.album
            .as_ref()
            .map(|a| format!("https://cdn.guyso.me/image/album/melon/{}.jpg", a.id))
    }

    /// Spotify URL（sameSongsから解決）
    pub fn spotify_url(&self) -> Option<String> {
        self.same_songs
            .iter()
            .find(|s| s.platform == "spotify")
            .and_then(|s| s.link.clone())
    }
}

/// ランキング一覧の1行
#[derive(Debug, Clone, Deserialize)]
pub struct ChartRow {
    pub ranking: i64,
    #[serde(default)]
    pub previous: i64,
    /// 이용자 수（일간のみ。TOP100では無い）
    #[serde(default)]
    pub count: Option<i64>,
    /// 変化量（일간のみ）
    #[serde(default)]
    #[serde(rename = "countDiff")]
    pub count_diff: Option<i64>,
    pub song: Song,
}

/// チャート一覧のペイロード（pageProps.data）
#[derive(Debug, Clone, Deserialize)]
pub struct ChartData {
    #[serde(default)]
    pub time: Option<String>,
    pub data: Vec<ChartRow>,
}

/// 曲trendの1点（daily/top100共通で受け取れるようOption化）
#[derive(Debug, Clone, Deserialize)]
pub struct TrendPoint {
    pub time: String,
    /// 順位（チャート圏外の日はnull。countのみ記録される）
    #[serde(default)]
    pub ranking: Option<i64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub previous: Option<i64>,
    /// 이용자 수（dailyのみ）
    #[serde(default)]
    pub count: Option<i64>,
    #[serde(default)]
    #[serde(rename = "countDiff")]
    pub count_diff: Option<i64>,
}

/// 曲trendのペイロード（pageProps）
#[derive(Debug, Clone, Deserialize)]
pub struct TrendResponse {
    pub data: TrendData,
    pub song: Song,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrendData {
    pub data: Vec<TrendPoint>,
}

/// 1日分の時刻別順位（0〜23時）
pub type HourlyRanks = [Option<i64>; 24];

impl TrendData {
    /// 時間単位trendを日付×時刻にピボット（日付昇順）
    ///
    /// TOP100は圏外の時刻もnullで返るため、最初にランクインした日より前の空行は除く
    pub fn hourly_grid(&self) -> Vec<(String, HourlyRanks)> {
        let mut grid: std::collections::BTreeMap<&str, HourlyRanks> =
            std::collections::BTreeMap::new();
        for p in &self.data {
            let Some(date) = p.time.get(0..10) else {
                continue;
            };
            let Some(hour) = p.time.get(11..13).and_then(|h| h.parse::<usize>().ok()) else {
                continue;
            };
            if hour >= 24 {
                continue;
            }
            grid.entry(date).or_insert([None; 24])[hour] = p.ranking;
        }
        grid.into_iter()
            .skip_while(|(_, hours)| hours.iter().all(Option::is_none))
            .map(|(date, hours)| (date.to_string(), hours))
            .collect()
    }
}
