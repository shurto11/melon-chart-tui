use anyhow::{anyhow, Context, Result};
use serde::de::DeserializeOwned;

use crate::models::{AsciiArt, AsciiPixel, ChartData, TrendPoint, TrendResponse};

const BASE: &str = "https://guyso.me";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// どのチャートを対象にするか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chart {
    Top100,
    Daily,
}

impl Chart {
    /// 一覧ページのパス
    fn list_path(self) -> &'static str {
        match self {
            Chart::Top100 => "/chart/melon/top100",
            Chart::Daily => "/chart/melon/daily",
        }
    }

    /// 曲trendページのパスを組み立て
    fn trend_path(self, song_id: &str) -> String {
        match self {
            Chart::Top100 => format!("/chart/melon/top100/trend/ranking/{}", song_id),
            Chart::Daily => format!("/chart/melon/daily/trend/ranking/{}", song_id),
        }
    }
}

fn client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(UA)
        // dailyページは非圧縮1MB超・サーバーが遅く、デフォルト30sでは足りない
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to build HTTP client")
}

/// ページHTMLから __NEXT_DATA__ のJSONを抽出
fn extract_next_data(html: &str) -> Result<serde_json::Value> {
    const OPEN: &str = "<script id=\"__NEXT_DATA__\" type=\"application/json\">";
    let start = html
        .find(OPEN)
        .ok_or_else(|| anyhow!("__NEXT_DATA__ not found"))?
        + OPEN.len();
    let rest = &html[start..];
    let end = rest
        .find("</script>")
        .ok_or_else(|| anyhow!("__NEXT_DATA__ end tag not found"))?;
    let json_str = &rest[..end];
    serde_json::from_str(json_str).context("Failed to parse __NEXT_DATA__ JSON")
}

/// URLを取得し、__NEXT_DATA__.props.pageProps を目的の型にデシリアライズ
fn fetch_page_props<T: DeserializeOwned>(url: &str) -> Result<T> {
    let html = client()?
        .get(url)
        .send()
        .with_context(|| format!("Failed to GET {}", url))?
        .text()
        .context("Failed to read response body")?;
    let data = extract_next_data(&html)?;
    let page_props = data
        .get("props")
        .and_then(|p| p.get("pageProps"))
        .ok_or_else(|| anyhow!("props.pageProps not found"))?
        .clone();
    serde_json::from_value(page_props).context("Failed to deserialize pageProps")
}

/// pageProps.data 部分を型付きで取得（一覧ページ用）
#[derive(serde::Deserialize)]
struct ListPageProps {
    data: ChartData,
}

/// チャート一覧を取得
pub fn fetch_chart(chart: Chart) -> Result<ChartData> {
    let url = format!("{}{}", BASE, chart.list_path());
    let props: ListPageProps = fetch_page_props(&url)?;
    Ok(props.data)
}

/// 曲trendを取得（TOP100は直近31日分のみ。それより前は fetch_top100_trend_older）
pub fn fetch_trend(chart: Chart, song_id: &str) -> Result<TrendResponse> {
    let url = format!("{}{}", BASE, chart.trend_path(song_id));
    fetch_page_props(&url)
}

/// TOP100 trendの過去31日分を取得
///
/// サイトは `?end=YYYY-MM-DD` を終了日とする31日窓で返す（圏外の時刻もnullで埋まる）。
/// `oldest` は取得済みの最古の時刻で、その前日を終了日として要求する
pub fn fetch_top100_trend_older(song_id: &str, oldest: &str) -> Result<Vec<TrendPoint>> {
    let end = prev_day(oldest).ok_or_else(|| anyhow!("Invalid trend time: {}", oldest))?;
    let url = format!("{}{}?end={}", BASE, Chart::Top100.trend_path(song_id), end);
    let props: TrendResponse = fetch_page_props(&url)?;
    Ok(props.data.data)
}

/// "YYYY-MM-DD..." の前日を "YYYY-MM-DD" で返す
fn prev_day(time: &str) -> Option<String> {
    let y: i32 = time.get(0..4)?.parse().ok()?;
    let m: u32 = time.get(5..7)?.parse().ok()?;
    let d: u32 = time.get(8..10)?.parse().ok()?;
    let (y, m, d) = if d > 1 {
        (y, m, d - 1)
    } else if m > 1 {
        (y, m - 1, days_in_month(y, m - 1))
    } else {
        (y - 1, 12, 31)
    };
    Some(format!("{:04}-{:02}-{:02}", y, m, d))
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// アルバム画像URLからカラーASCIIアートに変換
pub fn fetch_album_art_ascii(image_url: &str, width: u32, height: u32) -> Result<AsciiArt> {
    let img_bytes = client()?
        .get(image_url)
        .send()
        .context("Failed to fetch album image")?
        .bytes()
        .context("Failed to read album image bytes")?;

    let img = image::load_from_memory(&img_bytes)
        .context("Failed to decode image")?
        .resize_exact(width, height, image::imageops::FilterType::Lanczos3)
        .to_rgb8();

    // ASCII文字セット（暗い→明るい、ターミナルdark bg向け）
    let ascii_chars: &[u8] = b" .:-=+*#%@";

    let mut lines: AsciiArt = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut row: Vec<AsciiPixel> = Vec::with_capacity(width as usize);
        for x in 0..width {
            let pixel = img.get_pixel(x, y);
            let r = pixel[0];
            let g = pixel[1];
            let b = pixel[2];
            // ITU-R BT.601 輝度計算
            let luminance = 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
            let idx = ((luminance / 255.0) * (ascii_chars.len() - 1) as f64).round() as usize;
            let idx = idx.min(ascii_chars.len() - 1);
            row.push((ascii_chars[idx] as char, r, g, b));
        }
        lines.push(row);
    }

    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::prev_day;

    #[test]
    fn prev_day_handles_month_and_year_boundaries() {
        assert_eq!(prev_day("2026-09-21T00:00:00").as_deref(), Some("2026-09-20"));
        assert_eq!(prev_day("2026-10-01T00:00:00").as_deref(), Some("2026-09-30"));
        assert_eq!(prev_day("2024-03-01").as_deref(), Some("2024-02-29"));
        assert_eq!(prev_day("2026-03-01").as_deref(), Some("2026-02-28"));
        assert_eq!(prev_day("2026-01-01").as_deref(), Some("2025-12-31"));
        assert_eq!(prev_day("bad"), None);
    }
}
