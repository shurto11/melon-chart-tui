use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::fetch::Chart;
use crate::models::AsciiArt;

use super::app::{App, Mode, Screen};

/// 一覧サムネイルのASCIIアートサイズ
pub const ART_W: u32 = 8;
pub const ART_H: u32 = 4;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(1),    // body
            Constraint::Length(1), // footer
        ])
        .split(frame.area());

    draw_header(frame, app, chunks[0]);
    match app.screen {
        Screen::List => draw_list(frame, app, chunks[1]),
        Screen::SongDetail => match app.chart {
            Chart::Top100 => draw_grid(frame, app, chunks[1]),
            Chart::Daily => draw_daily_song(frame, app, chunks[1]),
        },
    }
    draw_footer(frame, app, chunks[1], chunks[2]);
}

fn chart_label(chart: Chart) -> &'static str {
    match chart {
        Chart::Top100 => "TOP100",
        Chart::Daily => "일간",
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let scope = match app.screen {
        Screen::List => "Ranking".to_string(),
        Screen::SongDetail => {
            let title = app
                .current_trend()
                .map(|t| {
                    format!("{} - {}", t.song.name, t.song.artist_names())
                })
                .unwrap_or_else(|| "Song".to_string());
            title
        }
    };
    let time = app.chart_time().unwrap_or("");
    let spinner = if app.loading || app.trend_pending || app.older_pending {
        let frames = ["|", "/", "-", "\\"];
        format!(" {}", frames[(app.tick / 2 % 4) as usize])
    } else {
        String::new()
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" Melon {} ", chart_label(app.chart)),
            Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  {}", scope)),
        Span::styled(format!("   {}", time), Style::default().fg(Color::DarkGray)),
        Span::styled(spinner, Style::default().fg(Color::Yellow)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_footer(frame: &mut Frame, app: &App, body: Rect, area: Rect) {
    // visible_rows をここで更新する必要があるが描画は不変参照。
    // メッセージ優先、それ以外はキーヒント。
    let _ = body;
    let content = if let Some(err) = &app.error {
        Line::from(Span::styled(
            format!(" ✗ {}", err),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))
    } else if let Some(msg) = &app.message {
        Line::from(Span::styled(
            format!(" {}", msg),
            Style::default().fg(Color::Green),
        ))
    } else if app.mode == Mode::Search {
        Line::from(Span::styled(
            format!(" /{}", app.input_buffer),
            Style::default().fg(Color::Yellow),
        ))
    } else {
        let hint = match app.screen {
            Screen::List => "j/k scroll  l detail  [/] chart  / search  n/N  c play  r reload  ^C quit",
            Screen::SongDetail => "j/k scroll  h back  [/] chart  c play  r reload  ^C quit",
        };
        Line::from(Span::styled(format!(" {}", hint), Style::default().fg(Color::DarkGray)))
    };
    frame.render_widget(Paragraph::new(content), area);
}

/// RGBを256色パレットのインデックスに変換（6x6x6カラーキューブ: 16-231）
fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let r6 = ((r as u16 * 5 + 127) / 255) as u8;
    let g6 = ((g as u16 * 5 + 127) / 255) as u8;
    let b6 = ((b as u16 * 5 + 127) / 255) as u8;
    16 + 36 * r6 + 6 * g6 + b6
}

/// ASCIIアートを Text（色付き）に変換
fn art_to_text(art: &AsciiArt) -> Text<'static> {
    let lines: Vec<Line> = art
        .iter()
        .map(|row| {
            let spans: Vec<Span> = row
                .iter()
                .map(|(c, r, g, b)| {
                    Span::styled(
                        c.to_string(),
                        Style::default().fg(Color::Indexed(rgb_to_256(*r, *g, *b))),
                    )
                })
                .collect();
            Line::from(spans)
        })
        .collect();
    Text::from(lines)
}

/// アートプレースホルダ（未取得）
fn art_placeholder() -> Text<'static> {
    let mut lines = Vec::new();
    for _ in 0..ART_H {
        lines.push(Line::from(Span::styled(
            " ".repeat(ART_W as usize),
            Style::default(),
        )));
    }
    Text::from(lines)
}

const ROW_H: u16 = ART_H as u16;

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let len = app.rows().len();
    if len == 0 {
        let msg = if app.loading {
            "Loading..."
        } else {
            "No data (press r to reload)"
        };
        frame.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let visible = (area.height / ROW_H).max(1) as usize;
    app.visible_rows = visible;
    // オフセット調整（選択行が可視範囲に入るように）
    if app.list_index < app.list_offset {
        app.list_offset = app.list_index;
    } else if app.list_index >= app.list_offset + visible {
        app.list_offset = app.list_index + 1 - visible;
    }

    let is_daily = app.chart == Chart::Daily;
    let start = app.list_offset;
    let end = (start + visible).min(len);
    let rows_data = app.rows();

    let mut table_rows: Vec<Row> = Vec::new();
    let mut art_requests: Vec<(String, String)> = Vec::new(); // (url, song_id) 未使用idだが将来用

    for i in start..end {
        let r = &rows_data[i];
        let selected = i == app.list_index;
        let matched = app.search_matches.contains(&i);

        // 順位 & 変動
        let delta = if r.previous == 0 {
            Span::styled("NEW", Style::default().fg(Color::Green))
        } else if r.ranking < r.previous {
            Span::styled(format!("▲{}", r.previous - r.ranking), Style::default().fg(Color::Red))
        } else if r.ranking > r.previous {
            Span::styled(format!("▼{}", r.ranking - r.previous), Style::default().fg(Color::Blue))
        } else {
            Span::styled("–", Style::default().fg(Color::DarkGray))
        };
        let rank_cell = Cell::from(Text::from(vec![
            Line::from(Span::styled(
                format!("{}", r.ranking),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(delta),
        ]));

        // アルバムアート
        let art_cell = if let Some(url) = r.song.album_image_url() {
            if let Some(art) = app.album_art_cache.get(&url) {
                Cell::from(art_to_text(art))
            } else {
                art_requests.push((url, r.song.id.clone()));
                Cell::from(art_placeholder())
            }
        } else {
            Cell::from(art_placeholder())
        };

        // 曲情報
        let name_style = if matched {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().add_modifier(Modifier::BOLD)
        };
        let song_cell = Cell::from(Text::from(vec![
            Line::from(Span::styled(r.song.name.clone(), name_style)),
            Line::from(Span::styled(
                r.song.artist_names(),
                Style::default().fg(Color::Cyan),
            )),
        ]));
        let album_cell = Cell::from(Span::styled(
            r.song.album_name().to_string(),
            Style::default().fg(Color::DarkGray),
        ));

        let mut cells = vec![rank_cell, art_cell, song_cell, album_cell];
        if is_daily {
            let count = r.count.unwrap_or(0);
            let diff = r.count_diff.unwrap_or(0);
            let diff_span = if diff > 0 {
                Span::styled(format!("+{}", diff), Style::default().fg(Color::Red))
            } else if diff < 0 {
                Span::styled(format!("{}", diff), Style::default().fg(Color::Blue))
            } else {
                Span::styled("0".to_string(), Style::default().fg(Color::DarkGray))
            };
            cells.push(Cell::from(Text::from(vec![
                Line::from(Span::styled(
                    format!("{}", count),
                    Style::default().fg(Color::Gray),
                )),
                Line::from(diff_span),
            ])));
        }

        let mut row = Row::new(cells).height(ROW_H);
        if selected {
            row = row.style(Style::default().bg(Color::Rgb(40, 40, 60)));
        }
        table_rows.push(row);
    }

    // 可視分のアート取得リクエスト
    for (url, _id) in art_requests {
        request_album_art(app, url);
    }

    let mut widths = vec![
        Constraint::Length(5),
        Constraint::Length(ART_W as u16),
        Constraint::Min(20),      // 曲名 / アーティスト名
        Constraint::Percentage(30), // アルバム名
    ];
    if is_daily {
        widths.push(Constraint::Length(12)); // 이용자 수 / 변화량
    }
    let table = Table::new(table_rows, widths)
        .column_spacing(1)
        .block(Block::default().borders(Borders::NONE));
    frame.render_widget(table, area);
}

/// アルバムアートをバックグラウンドで取得
fn request_album_art(app: &mut App, url: String) {
    if app.album_art_cache.contains_key(&url) || app.album_art_requested.contains(&url) {
        return;
    }
    app.album_art_requested.insert(url.clone());
    let tx = app.art_tx.clone();
    std::thread::spawn(move || {
        if let Ok(art) = crate::fetch::fetch_album_art_ascii(&url, ART_W, ART_H) {
            let _ = tx.send((url, art));
        }
    });
}

/// 일간の曲情報（日付 / 순위 / 이용자 수 / 변화량）
fn draw_daily_song(frame: &mut Frame, app: &mut App, area: Rect) {
    let len = match app.current_trend() {
        Some(t) => t.data.data.len(),
        None => {
            let msg = if app.trend_pending { "Loading trend..." } else { "No trend data" };
            frame.render_widget(
                Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
                area,
            );
            return;
        }
    };
    let visible = area.height.saturating_sub(1).max(1) as usize; // header分-1
    if app.detail_offset + visible > len {
        app.detail_offset = len.saturating_sub(visible);
    }
    let start = app.detail_offset;
    let end = (start + visible).min(len);
    let trend = app.current_trend().unwrap();
    let points = &trend.data.data;

    let header = Row::new(vec![
        Cell::from("날짜"),
        Cell::from("순위"),
        Cell::from("이용자 수"),
        Cell::from("변화량"),
    ])
    .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));

    let mut rows: Vec<Row> = Vec::new();
    for p in &points[start..end] {
        let date = p.time.get(0..10).unwrap_or(&p.time);
        let count = p.count.unwrap_or(0);
        let diff = p.count_diff.unwrap_or(0);
        let diff_cell = if diff > 0 {
            Cell::from(format!("+{}", diff)).style(Style::default().fg(Color::Red))
        } else if diff < 0 {
            Cell::from(format!("{}", diff)).style(Style::default().fg(Color::Blue))
        } else {
            Cell::from("0").style(Style::default().fg(Color::DarkGray))
        };
        rows.push(Row::new(vec![
            date_cell(date, date),
            Cell::from(p.ranking.map_or("-".to_string(), |r| r.to_string())),
            Cell::from(format!("{}", count)),
            diff_cell,
        ]));
    }

    let widths = [
        Constraint::Length(13),
        Constraint::Length(6),
        Constraint::Length(12),
        Constraint::Length(12),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(2);
    frame.render_widget(table, area);
}

/// TOP100の曲情報グリッド（縦=日付、横=時刻、セル=順位）
fn draw_grid(frame: &mut Frame, app: &mut App, area: Rect) {
    // 日付×時刻にピボット（所有データを構築して借用を切る）
    let grid = match app.current_trend() {
        Some(trend) => trend.data.hourly_grid(),
        None => {
            let msg = if app.trend_pending { "Loading trend..." } else { "No trend data" };
            frame.render_widget(
                Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
                area,
            );
            return;
        }
    };
    if grid.is_empty() {
        // 直近31日が圏外なら過去ページを遡っている途中
        let msg = if app.older_pending { "Loading trend..." } else { "No TOP100 ranking" };
        frame.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let visible = area.height.saturating_sub(1).max(1) as usize;
    if app.detail_offset + visible > grid.len() {
        app.detail_offset = grid.len().saturating_sub(visible);
    }
    let start = app.detail_offset;
    let end = (start + visible).min(grid.len());

    // ヘッダ: 날짜 + 0..23
    let mut header_cells = vec![Cell::from("날짜")];
    for h in 0..24 {
        header_cells.push(Cell::from(format!("{:02}", h)));
    }
    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));

    let mut rows: Vec<Row> = Vec::new();
    for (date, hours) in &grid[start..end] {
        // 複数年に渡るので YY-MM-DD
        let mut cells = vec![date_cell(date.get(2..).unwrap_or(date), date)];
        for h in 0..24 {
            let cell = match hours[h] {
                Some(rank) => {
                    let (bg, fg) = rank_colors(rank);
                    Cell::from(format!("{:>3}", rank)).style(Style::default().bg(bg).fg(fg))
                }
                None => Cell::from("   "),
            };
            cells.push(cell);
        }
        rows.push(Row::new(cells));
    }

    let mut widths = vec![Constraint::Length(12)];
    for _ in 0..24 {
        widths.push(Constraint::Length(3));
    }
    let table = Table::new(rows, widths).header(header).column_spacing(0);
    frame.render_widget(table, area);
}

/// 曜日（日曜始まり）
const WEEKDAYS_KO: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];

/// "YYYY-MM-DD..." の曜日（0=日曜〜6=土曜）。Sakamotoの方法
fn weekday(date: &str) -> Option<usize> {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y: i32 = date.get(0..4)?.parse().ok()?;
    let m: usize = date.get(5..7)?.parse().ok()?;
    let d: i32 = date.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    let y = if m < 3 { y - 1 } else { y };
    Some(((y + y / 4 - y / 100 + y / 400 + T[m - 1] + d) % 7) as usize)
}

/// 日付セル（label の右に曜日。土曜=青、日曜=赤）
fn date_cell(label: &str, date: &str) -> Cell<'static> {
    let Some(wd) = weekday(date) else {
        return Cell::from(label.to_string());
    };
    let style = match wd {
        0 => Style::default().fg(Color::Red),
        6 => Style::default().fg(Color::Blue),
        _ => Style::default(),
    };
    Cell::from(format!("{} {}", label, WEEKDAYS_KO[wd])).style(style)
}

/// 順位に応じたセル配色（背景, 前景）
fn rank_colors(rank: i64) -> (Color, Color) {
    // 1위=黄, 2위=橙, 3위=濃橙, 以降10位単位
    let bg = match rank {
        1 => Color::Indexed(226),       // 黄
        2 => Color::Indexed(214),       // オレンジ
        3 => Color::Indexed(208),       // 濃いオレンジ
        r if r <= 10 => Color::Indexed(202), // 赤橙
        r if r <= 20 => Color::Indexed(203),
        r if r <= 30 => Color::Indexed(168),
        r if r <= 40 => Color::Indexed(133),
        r if r <= 50 => Color::Indexed(97),
        r if r <= 60 => Color::Indexed(61),
        r if r <= 70 => Color::Indexed(67),
        r if r <= 80 => Color::Indexed(73),
        r if r <= 90 => Color::Indexed(66),
        _ => Color::Indexed(240),
    };
    // 明るい背景（上位）は黒文字、暗い背景は白文字
    let fg = if rank <= 3 { Color::Black } else { Color::White };
    (bg, fg)
}

/// 文字列の表示幅（検索ハイライト等の補助、現状未使用だが将来用）
#[allow(dead_code)]
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

#[cfg(test)]
mod tests {
    use super::weekday;

    #[test]
    fn weekday_from_date() {
        assert_eq!(weekday("2026-10-21T09:00:00"), Some(3)); // 수
        assert_eq!(weekday("2026-10-18"), Some(0)); // 일
        assert_eq!(weekday("2026-10-17"), Some(6)); // 토
        assert_eq!(weekday("2024-02-29"), Some(4)); // 목
        assert_eq!(weekday("2026-01-01"), Some(4)); // 목
        assert_eq!(weekday("bad"), None);
    }
}
