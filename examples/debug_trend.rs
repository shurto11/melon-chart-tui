// 一時デバッグ用: daily trend の取得とデシリアライズを検証する
#[path = "../src/models/mod.rs"]
mod models;
#[path = "../src/fetch/mod.rs"]
mod fetch;

use fetch::{fetch_chart, fetch_trend, Chart};

fn main() {
    // 保存済みHTMLでのオフライン検証
    if let Ok(html) = std::fs::read_to_string(std::env::args().nth(1).unwrap_or_default()) {
        const OPEN: &str = "<script id=\"__NEXT_DATA__\" type=\"application/json\">";
        let start = html.find(OPEN).unwrap() + OPEN.len();
        let end = html[start..].find("</script>").unwrap();
        let v: serde_json::Value = serde_json::from_str(&html[start..start + end]).unwrap();
        let pp = v["props"]["pageProps"].clone();
        let pp_str = pp.to_string();
        let mut de = serde_json::Deserializer::from_str(&pp_str);
        match serde_path_to_error::deserialize::<_, models::TrendResponse>(&mut de) {
            Ok(t) => println!("offline: OK points={} song={}", t.data.data.len(), t.song.name),
            Err(e) => println!("offline: ERROR at `{}`: {}", e.path(), e),
        }
        return;
    }

    // ライブ検証: daily 上位数曲の trend を取得
    let daily = fetch_chart(Chart::Daily).expect("daily fetch");
    for row in daily.data.iter().take(5) {
        match fetch_trend(Chart::Daily, &row.song.id) {
            Ok(t) => println!(
                "#{} {} ({}): OK points={}",
                row.ranking,
                row.song.name,
                row.song.id,
                t.data.data.len()
            ),
            Err(e) => println!("#{} {} ({}): ERROR {:#}", row.ranking, row.song.name, row.song.id, e),
        }
    }
}
